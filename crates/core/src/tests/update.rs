use std::collections::HashMap;
use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use gpui_kit::TestAppContext;
use kg_domain::{InstallKind, Op, OpError, OpResult, PreferenceOp, UpdateOp, UpdateState};
use serde_json::json;

use super::Fixture;
use crate::{Target, Updater, Workspace};

const PAYLOAD: &[u8] = include_bytes!("update/payload");

/// A throwaway minisign key, signing the way cargo-packager does.
struct Signer {
    key: String,
    secret: minisign::SecretKey,
}

impl Signer {
    fn new() -> Self {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        Self {
            key: pair.pk.to_base64(),
            secret: pair.sk,
        }
    }

    fn sign(&self, bytes: &[u8]) -> String {
        let signature = minisign::sign(None, &self.secret, bytes, None, None).unwrap();
        base64::engine::general_purpose::STANDARD.encode(signature.to_string())
    }
}

#[derive(Clone)]
struct Server {
    url: String,
    hits: Arc<AtomicUsize>,
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = Self {
            url: format!("http://{}", listener.local_addr().unwrap()),
            hits: Arc::default(),
            files: Arc::default(),
        };
        let shared = server.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let path = line.split(' ').nth(1).unwrap_or_default().to_owned();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap() <= 2 {
                        break;
                    }
                }
                shared.hits.fetch_add(1, Ordering::SeqCst);
                let body = shared.files.lock().unwrap().get(&path).cloned();
                let (status, body) =
                    body.map_or(("404 Not Found", Vec::new()), |body| ("200 OK", body));
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        server
    }

    fn put(&self, path: &str, body: impl Into<Vec<u8>>) {
        self.files.lock().unwrap().insert(path.into(), body.into());
    }

    /// Serves a signed `update.json` that offers `version` as `url`.
    fn offer(&self, signer: &Signer, version: &str, url: &str, signature: &str) {
        let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
        let manifest = json!({
            "version": version,
            "platforms": { platform: { "url": url, "signature": signature } },
        })
        .to_string();
        self.put("/update.json.sig", signer.sign(manifest.as_bytes()));
        self.put("/update.json", manifest);
    }

    fn file(&self, name: &str) -> String {
        format!("{}/{name}", self.url)
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

fn appimage(dir: &Path) -> (Target, PathBuf) {
    let image = dir.join("kagan.AppImage");
    std::fs::write(&image, "old kagan").unwrap();
    let target = Target::find(
        "linux",
        Path::new("/tmp/.mount_kagan/usr/bin/kg"),
        Some(image.clone()),
        Some(Path::new("/tmp/.mount_kagan")),
        &[],
    );
    (target, image)
}

fn source(cx: &mut TestAppContext, server: &Server, key: &str, target: Target) {
    let endpoint = server.file("update.json");
    cx.update(|cx| {
        Updater::global(cx).update(cx, |updater, _| {
            updater.use_source(endpoint, key.into(), target);
        });
    });
}

async fn op(cx: &mut TestAppContext, op: Op) -> Result<OpResult, OpError> {
    cx.update(|cx| Workspace::dispatch(op, cx)).await
}

async fn check(cx: &mut TestAppContext) -> Result<OpResult, OpError> {
    op(cx, Op::Update(UpdateOp::Check)).await
}

fn state(cx: &mut TestAppContext) -> UpdateState {
    cx.update(|cx| Updater::global(cx).read(cx).state().clone())
}

async fn download(cx: &mut TestAppContext, at_quit: bool) -> Result<(), OpError> {
    cx.update(|cx| Updater::global(cx).update(cx, |updater, cx| updater.download(at_quit, cx)))
        .await
}

async fn replace(cx: &mut TestAppContext) -> Result<(), OpError> {
    cx.update(|cx| Updater::global(cx).update(cx, Updater::replace))
        .await
}

#[gpui_kit::test]
async fn the_automatic_check_is_off_by_default(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let server = Server::start();
    let signer = Signer::new();
    server.offer(
        &signer,
        "99.0.0",
        &server.file("payload"),
        &signer.sign(PAYLOAD),
    );
    source(cx, &server, &signer.key, appimage(fixture.dir.path()).0);
    cx.executor().advance_clock(Duration::from_hours(25));
    cx.run_until_parked();
    assert_eq!(server.hits(), 0);
    let Ok(OpResult::Update(status)) = op(cx, Op::Update(UpdateOp::Status)).await else {
        panic!("status failed");
    };
    assert!(!status.automatic);
    assert_eq!(status.state, UpdateState::Unchecked);

    op(
        cx,
        Op::Preference(PreferenceOp::Set {
            key: "update-check".into(),
            value: json!(true),
        }),
    )
    .await
    .unwrap();
    cx.executor().advance_clock(Duration::from_hours(1));
    cx.run_until_parked();
    assert_eq!(server.hits(), 2);
    assert!(matches!(state(cx), UpdateState::Available { .. }));
}

#[gpui_kit::test]
async fn a_check_offers_only_a_newer_release(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let server = Server::start();
    let signer = Signer::new();
    source(cx, &server, &signer.key, appimage(fixture.dir.path()).0);
    for (version, offered) in [
        ("0.0.1", false),
        (env!("CARGO_PKG_VERSION"), false),
        ("99.0.0-rc.1", false),
        ("v99.0.0", true),
    ] {
        server.offer(&signer, version, &server.file("payload"), "");
        let Ok(OpResult::Update(status)) = check(cx).await else {
            panic!("check failed");
        };
        assert_eq!(
            status.state
                == UpdateState::Available {
                    version: "99.0.0".into(),
                    notes: "https://github.com/kagan-sh/kagan/releases/tag/v99.0.0".into(),
                },
            offered,
            "{version}: {:?}",
            status.state
        );
        assert_eq!(status.state == UpdateState::Current, !offered);
    }
}

#[gpui_kit::test]
async fn a_failed_check_says_why(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let server = Server::start();
    source(
        cx,
        &server,
        &Signer::new().key,
        appimage(fixture.dir.path()).0,
    );
    assert!(check(cx).await.is_err());
    assert!(matches!(state(cx), UpdateState::Failed { .. }));
}

#[gpui_kit::test]
async fn a_list_that_is_unsigned_tampered_or_signed_by_another_key_is_refused(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let server = Server::start();
    let signer = Signer::new();
    source(cx, &server, &signer.key, appimage(fixture.dir.path()).0);

    server.offer(&Signer::new(), "99.0.0", &server.file("payload"), "");
    let refused = check(cx).await.unwrap_err().to_string();
    assert!(refused.contains("signature"), "{refused}");

    server.offer(&signer, "99.0.0", &server.file("payload"), "");
    let list = server.files.lock().unwrap()["/update.json"].clone();
    server.put(
        "/update.json",
        String::from_utf8(list).unwrap().replace("99.0.0", "99.0.1"),
    );
    let refused = check(cx).await.unwrap_err().to_string();
    assert!(refused.contains("signature"), "{refused}");

    server.files.lock().unwrap().remove("/update.json.sig");
    assert!(check(cx).await.is_err());
    assert!(matches!(state(cx), UpdateState::Failed { .. }));
}

#[gpui_kit::test]
async fn an_update_link_off_https_is_refused(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let server = Server::start();
    let signer = Signer::new();
    let target = appimage(fixture.dir.path()).0;
    cx.update(|cx| {
        Updater::global(cx).update(cx, |updater, _| {
            updater.use_source(
                "http://example.com/update.json".into(),
                signer.key.clone(),
                target.clone(),
            );
        });
    });
    let refused = check(cx).await.unwrap_err().to_string();
    assert!(refused.contains("HTTPS"), "{refused}");

    source(cx, &server, &signer.key, target);
    server.offer(
        &signer,
        "99.0.0",
        "http://example.com/payload",
        &signer.sign(PAYLOAD),
    );
    check(cx).await.unwrap();
    let refused = download(cx, false).await.unwrap_err().to_string();
    assert!(refused.contains("HTTPS"), "{refused}");
}

#[gpui_kit::test]
async fn the_signature_check_refuses_a_tampered_file_a_wrong_key_and_no_signature(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let server = Server::start();
    let signer = Signer::new();
    let (target, image) = appimage(fixture.dir.path());
    let mut tampered = PAYLOAD.to_vec();
    tampered[0] ^= 1;
    server.put("/payload", PAYLOAD);
    server.put("/tampered", tampered);
    source(cx, &server, &signer.key, target);

    for (file, signature) in [
        ("tampered", signer.sign(PAYLOAD)),
        ("payload", Signer::new().sign(PAYLOAD)),
        ("payload", String::new()),
    ] {
        server.offer(&signer, "99.0.0", &server.file(file), &signature);
        check(cx).await.unwrap();
        let refused = download(cx, false).await.unwrap_err().to_string();
        assert!(refused.contains("signature"), "{file}: {refused}");
        assert!(matches!(state(cx), UpdateState::Available { .. }));
        assert_eq!(std::fs::read(&image).unwrap(), b"old kagan");
    }

    server.offer(
        &signer,
        "99.0.0",
        &server.file("payload"),
        &signer.sign(PAYLOAD),
    );
    check(cx).await.unwrap();
    download(cx, true).await.unwrap();
    assert!(matches!(
        state(cx),
        UpdateState::Ready { at_quit: true, .. }
    ));
    download(cx, false).await.unwrap();
    assert!(matches!(
        state(cx),
        UpdateState::Ready { at_quit: false, .. }
    ));
    replace(cx).await.unwrap();
    assert_eq!(std::fs::read(&image).unwrap(), PAYLOAD);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_folder_kagan_cannot_write_fails_before_the_download(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = Fixture::new(cx).await;
    let server = Server::start();
    let signer = Signer::new();
    let folder = fixture.dir.path().join("locked");
    std::fs::create_dir(&folder).unwrap();
    let (target, _) = appimage(&folder);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
    server.put("/payload", PAYLOAD);
    server.offer(
        &signer,
        "99.0.0",
        &server.file("payload"),
        &signer.sign(PAYLOAD),
    );
    source(cx, &server, &signer.key, target);
    check(cx).await.unwrap();
    let hits = server.hits();
    let refused = download(cx, false).await.unwrap_err().to_string();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(refused.contains("Move kagan to Applications"), "{refused}");
    assert_eq!(server.hits(), hits);
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
async fn an_app_update_swaps_the_bundle_in_one_step(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let server = Server::start();
    let signer = Signer::new();
    let app = fixture.dir.path().join("kagan.app");
    let exe = app.join("Contents/MacOS/kg");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(&exe, "old kagan\n").unwrap();
    let target = Target::find("macos", &exe, None, None, &[]);
    let archive = include_bytes!("update/kagan.app.tar.gz");
    server.put("/kagan.app.tar.gz", archive.to_vec());
    server.offer(
        &signer,
        "99.0.0",
        &server.file("kagan.app.tar.gz"),
        &signer.sign(archive),
    );
    source(cx, &server, &signer.key, target);
    check(cx).await.unwrap();
    download(cx, false).await.unwrap();
    assert_eq!(std::fs::read(&exe).unwrap(), b"old kagan\n");
    replace(cx).await.unwrap();
    assert_eq!(std::fs::read(&exe).unwrap(), b"new kagan\n");
    let replaced = fixture
        .dir
        .path()
        .join(".kagan.app.update/kagan.app/Contents/MacOS/kg");
    assert_eq!(std::fs::read(replaced).unwrap(), b"old kagan\n");
}

#[test]
fn each_install_kind_updates_its_own_way() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bundle = |folder: &str| {
        let exe = root.join(folder).join("kagan.app/Contents/MacOS/kg");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        exe
    };
    let dmg = bundle("Downloads");
    let cask = bundle("Applications");
    let brew = root.join("brew");
    std::fs::create_dir_all(brew.join("Caskroom/kagan")).unwrap();
    let windows = root.join("Programs/kagan/kg.exe");
    std::fs::create_dir_all(windows.parent().unwrap()).unwrap();
    std::fs::write(windows.with_file_name("uninstall.exe"), "").unwrap();
    let image = root.join("kagan.AppImage");
    let mount = root.join(".mount_kagan");
    let mounted = mount.join("usr/bin/kg");
    let plain = root.join("target/release/kg");
    let none: Option<&Path> = None;

    for (os, exe, appimage, appdir, prefixes, kind) in [
        (
            "macos",
            &dmg,
            None,
            none,
            vec![brew.clone()],
            InstallKind::App,
        ),
        (
            "macos",
            &cask,
            None,
            none,
            vec![brew.clone()],
            InstallKind::Homebrew,
        ),
        (
            "macos",
            &cask,
            None,
            none,
            vec![root.join("nobrew")],
            InstallKind::App,
        ),
        (
            "macos",
            &plain,
            None,
            none,
            vec![brew.clone()],
            InstallKind::Other,
        ),
        (
            "windows",
            &windows,
            None,
            none,
            vec![],
            InstallKind::Installer,
        ),
        ("windows", &plain, None, none, vec![], InstallKind::Other),
        (
            "linux",
            &mounted,
            Some(image.clone()),
            Some(mount.as_path()),
            vec![],
            InstallKind::AppImage,
        ),
        (
            "linux",
            &plain,
            Some(image.clone()),
            Some(mount.as_path()),
            vec![],
            InstallKind::Other,
        ),
        (
            "linux",
            &plain,
            Some(image.clone()),
            none,
            vec![],
            InstallKind::Other,
        ),
        ("linux", &plain, None, none, vec![], InstallKind::Other),
    ] {
        let target = Target::find(os, exe, appimage, appdir, &prefixes);
        assert_eq!(target.install, kind, "{os} {}", exe.display());
    }
    assert!(InstallKind::App.replaces_itself());
    assert!(InstallKind::Installer.replaces_itself());
    assert!(InstallKind::AppImage.replaces_itself());
    assert!(!InstallKind::Homebrew.replaces_itself());
    assert!(!InstallKind::Other.replaces_itself());
}

#[gpui_kit::test]
async fn homebrew_and_other_copies_are_not_replaced(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let server = Server::start();
    let signer = Signer::new();
    server.put("/payload", PAYLOAD);
    server.offer(
        &signer,
        "99.0.0",
        &server.file("payload"),
        &signer.sign(PAYLOAD),
    );
    for target in [
        Target::find("linux", Path::new("/usr/bin/kg"), None, None, &[]),
        Target::find("windows", Path::new("/nowhere/kg.exe"), None, None, &[]),
    ] {
        source(cx, &server, &signer.key, target);
        check(cx).await.unwrap();
        assert!(matches!(
            download(cx, false).await,
            Err(OpError::Invalid(_))
        ));
    }
}
