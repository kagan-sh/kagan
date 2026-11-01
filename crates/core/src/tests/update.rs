use std::collections::HashMap;
use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_domain::{Install, Op, OpError, OpResult, PreferenceOp, UpdateOp, UpdateState};
use serde_json::json;

use super::Fixture;
use crate::{Target, Updater, Workspace};

const KEY: &str = "RWSzD/q2tCpySPtUN4941KW/7HB5Kz7ILtj4uTiCqHFCzi8xT8hhvVY1";
const OTHER_KEY: &str = "RWR5lwcRLb7K0fhTI96pBpA0ePVesZfH6fZJYujRCdvb4G3+9W5rGGqq";
const PAYLOAD: &[u8] = include_bytes!("update/payload");
const SIGNATURE: &str = include_str!("update/payload.sig");

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

    fn offer(&self, version: &str, file: &str, signature: &str) {
        let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
        let manifest = json!({
            "version": version,
            "notes": "https://github.com/kagan-sh/kagan/releases/tag/v99.0.0",
            "platforms": { platform: { "url": format!("{}/{file}", self.url), "signature": signature } },
        });
        self.put("/update.json", manifest.to_string());
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

fn appimage(dir: &Path) -> (Target, PathBuf) {
    let image = dir.join("kagan.AppImage");
    std::fs::write(&image, "old kagan").unwrap();
    (
        Target::find("linux", Path::new("/tmp/kg"), Some(image.clone()), &[]),
        image,
    )
}

fn source(cx: &mut TestAppContext, server: &Server, key: &str, target: Target) {
    cx.update(|cx| {
        Updater::global(cx).update(cx, |updater, _| {
            updater.use_source(format!("{}/update.json", server.url), key.into(), target);
        });
    });
}

async fn op(cx: &mut TestAppContext, op: Op) -> Result<OpResult, OpError> {
    cx.update(|cx| Workspace::dispatch(op, cx)).await
}

fn state(cx: &mut TestAppContext) -> UpdateState {
    cx.update(|cx| Updater::global(cx).read(cx).state().clone())
}

async fn download(cx: &mut TestAppContext) -> Result<(), OpError> {
    cx.update(|cx| Updater::global(cx).update(cx, |updater, cx| updater.download(false, cx)))
        .await
}

async fn replace(cx: &mut TestAppContext) -> Result<(), OpError> {
    cx.update(|cx| Updater::global(cx).update(cx, |updater, cx| updater.replace(cx)))
        .await
}

#[gpui_kit::test]
async fn the_automatic_check_is_off_by_default(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let server = Server::start();
    server.offer("99.0.0", "payload", SIGNATURE);
    source(cx, &server, KEY, appimage(fixture.dir.path()).0);
    cx.executor()
        .advance_clock(Duration::from_secs(25 * 60 * 60));
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
    cx.executor().advance_clock(Duration::from_secs(60 * 60));
    cx.run_until_parked();
    assert_eq!(server.hits(), 1);
    assert!(matches!(state(cx), UpdateState::Available { .. }));
}

#[gpui_kit::test]
async fn a_check_offers_only_a_newer_release(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let server = Server::start();
    source(cx, &server, KEY, appimage(fixture.dir.path()).0);
    for (version, offered) in [
        ("0.0.1", false),
        (env!("CARGO_PKG_VERSION"), false),
        ("99.0.0-rc.1", false),
        ("v99.0.0", true),
    ] {
        server.offer(version, "payload", SIGNATURE);
        let Ok(OpResult::Update(status)) = op(cx, Op::Update(UpdateOp::Check)).await else {
            panic!("check failed");
        };
        assert_eq!(
            matches!(status.state, UpdateState::Available { ref version, .. } if version == "99.0.0"),
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
    source(cx, &server, KEY, appimage(fixture.dir.path()).0);
    assert!(op(cx, Op::Update(UpdateOp::Check)).await.is_err());
    assert!(matches!(state(cx), UpdateState::Failed { .. }));
}

#[gpui_kit::test]
async fn the_signature_check_refuses_a_tampered_file_a_wrong_key_and_no_signature(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let server = Server::start();
    let (target, image) = appimage(fixture.dir.path());
    let mut tampered = PAYLOAD.to_vec();
    tampered[0] ^= 1;
    server.put("/payload", PAYLOAD);
    server.put("/tampered", tampered);

    for (file, signature, key) in [
        ("tampered", SIGNATURE, KEY),
        ("payload", SIGNATURE, OTHER_KEY),
        ("payload", "", KEY),
    ] {
        server.offer("99.0.0", file, signature);
        source(cx, &server, key, target.clone());
        op(cx, Op::Update(UpdateOp::Check)).await.unwrap();
        let refused = download(cx).await.unwrap_err().to_string();
        assert!(refused.contains("signature"), "{file} {key}: {refused}");
        assert!(matches!(state(cx), UpdateState::Available { .. }));
        assert_eq!(std::fs::read(&image).unwrap(), b"old kagan");
    }

    server.offer("99.0.0", "payload", SIGNATURE);
    source(cx, &server, KEY, target);
    op(cx, Op::Update(UpdateOp::Check)).await.unwrap();
    download(cx).await.unwrap();
    assert!(matches!(state(cx), UpdateState::Ready { .. }));
    replace(cx).await.unwrap();
    assert_eq!(std::fs::read(&image).unwrap(), PAYLOAD);
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
async fn an_app_update_replaces_the_bundle(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let server = Server::start();
    let app = fixture.dir.path().join("kagan.app");
    let exe = app.join("Contents/MacOS/kg");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(&exe, "old kagan\n").unwrap();
    let target = Target::find("macos", &exe, None, &[]);
    server.put(
        "/kagan.app.tar.gz",
        include_bytes!("update/kagan.app.tar.gz").to_vec(),
    );
    server.offer(
        "99.0.0",
        "kagan.app.tar.gz",
        include_str!("update/kagan.app.tar.gz.sig"),
    );
    source(cx, &server, KEY, target);
    op(cx, Op::Update(UpdateOp::Check)).await.unwrap();
    download(cx).await.unwrap();
    assert_eq!(std::fs::read(&exe).unwrap(), b"old kagan\n");
    replace(cx).await.unwrap();
    assert_eq!(std::fs::read(&exe).unwrap(), b"new kagan\n");
    assert!(fixture.dir.path().join(".kagan.app.old").exists());
    assert!(!fixture.dir.path().join(".kagan.app.update").exists());
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
    let plain = root.join("target/release/kg");

    for (os, exe, appimage, prefixes, kind) in [
        ("macos", &dmg, None, vec![brew.clone()], Install::App),
        ("macos", &cask, None, vec![brew.clone()], Install::Homebrew),
        (
            "macos",
            &cask,
            None,
            vec![root.join("nobrew")],
            Install::App,
        ),
        ("macos", &plain, None, vec![brew.clone()], Install::Other),
        ("windows", &windows, None, vec![], Install::Installer),
        ("windows", &plain, None, vec![], Install::Other),
        (
            "linux",
            &plain,
            Some(image.clone()),
            vec![],
            Install::AppImage,
        ),
        ("linux", &plain, None, vec![], Install::Other),
    ] {
        let target = Target::find(os, exe, appimage, &prefixes);
        assert_eq!(target.install, kind, "{os} {}", exe.display());
    }
    assert!(Install::App.replaces_itself());
    assert!(Install::Installer.replaces_itself());
    assert!(Install::AppImage.replaces_itself());
    assert!(!Install::Homebrew.replaces_itself());
    assert!(!Install::Other.replaces_itself());
}

#[gpui_kit::test]
async fn homebrew_and_other_copies_are_not_replaced(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let server = Server::start();
    server.put("/payload", PAYLOAD);
    server.offer("99.0.0", "payload", SIGNATURE);
    for target in [
        Target::find("linux", Path::new("/usr/bin/kg"), None, &[]),
        Target::find("windows", Path::new("/nowhere/kg.exe"), None, &[]),
    ] {
        source(cx, &server, KEY, target);
        op(cx, Op::Update(UpdateOp::Check)).await.unwrap();
        assert!(matches!(download(cx).await, Err(OpError::Invalid(_))));
    }
}
