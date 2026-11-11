use interprocess::local_socket::Name;
use kg_domain::{Op, OpError, OpResult, ThreadId, ThreadOp};

use crate::socket::{self, Client};

fn test_seq() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn test_dir(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("kg-{label}-{}-{}", std::process::id(), test_seq()))
}

fn test_name(label: &str) -> (tempfile::TempDir, Name<'static>) {
    let dir = tempfile::Builder::new()
        .prefix(&format!("kg-{label}-"))
        .tempdir()
        .unwrap();
    #[cfg(unix)]
    let name = socket::name(&socket::location_in(dir.path()).unwrap()).unwrap();
    #[cfg(windows)]
    let name = {
        use interprocess::local_socket::{GenericNamespaced, ToNsName};
        std::ffi::OsString::from(format!("kg-{label}-{}-{}", std::process::id(), test_seq()))
            .to_ns_name::<GenericNamespaced>()
            .unwrap()
    };
    (dir, name)
}

#[cfg(unix)]
#[test]
fn socket_dir_is_private_and_stale_socket_is_replaced() {
    use std::os::unix::fs::PermissionsExt;

    // Not test_dir: a pid-keyed path can collide with a leftover kg.sock after
    // the OS recycles the pid, and bind then fails with EADDRINUSE.
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let sock = dir.path().join("kg.sock");
    drop(std::os::unix::net::UnixListener::bind(&sock).unwrap());
    let location = socket::location_in(dir.path()).unwrap();
    let name = socket::name(&location).unwrap();
    assert_eq!(
        std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(sock.exists(), "stale socket file is gone");
    // Closing the listener does not make connect fail at once. A live socket is left
    // alone; a dead one is removed only after connect fails.
    let started = std::time::Instant::now();
    let listener = loop {
        if let Some(listener) = socket::open(&location).unwrap() {
            break listener;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "stale socket still answers"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    socket::serve(listener, |_, _| Ok(OpResult::Done));
    assert_eq!(
        Client::connect(name).unwrap().call(&Op::Sidebar),
        Ok(OpResult::Done)
    );
}

#[cfg(unix)]
#[test]
fn a_bare_socket_name_replaces_a_stale_socket() {
    let _cwd = Cwd::stay();
    let dir = tempfile::tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let _socket = set_env("KG_SOCKET", std::ffi::OsStr::new("kg.sock"));
    drop(std::os::unix::net::UnixListener::bind("kg.sock").unwrap());
    let location = socket::location().unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(location, "kg.sock");
    let started = std::time::Instant::now();
    let listener = loop {
        if let Some(listener) = socket::open(&location).unwrap_or_else(|error| panic!("{error}")) {
            break listener;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "stale socket still answers"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    socket::serve(listener, |_, _| Ok(OpResult::Done));
    assert_eq!(
        Client::connect(socket::name(&location).unwrap())
            .unwrap()
            .call(&Op::Sidebar),
        Ok(OpResult::Done)
    );
}

#[test]
fn socket_round_trip() {
    let (_dir, name) = test_name("round-trip");
    let listener = socket::bind(name.borrow()).unwrap().unwrap();
    socket::serve(listener, |op, _| match op {
        Op::Sidebar => Ok(OpResult::Done),
        _ => Err(OpError::NotFound("thread thr_9 not found".into())),
    });
    let mut client = Client::connect(name.borrow()).unwrap();
    assert_eq!(client.call(&Op::Sidebar), Ok(OpResult::Done));
    assert_eq!(
        client.call(&Op::Thread(ThreadOp::Get {
            id: ThreadId("thr_9".into())
        })),
        Err(OpError::NotFound("thread thr_9 not found".into()))
    );
    assert!(
        socket::bind(name).unwrap().is_none(),
        "second instance bound"
    );
}

#[test]
fn a_client_that_leaves_closes_its_requester_while_the_op_waits() {
    use std::io::Write as _;
    use std::sync::mpsc;

    use interprocess::local_socket::traits::Stream as _;

    let (_dir, name) = test_name("requester-gone");
    let listener = socket::bind(name.borrow()).unwrap().unwrap();
    let (started, waiting) = mpsc::channel();
    let (ended, gone) = mpsc::channel();
    socket::serve(listener, move |_, requester| {
        started.send(requester.is_closed()).unwrap();
        ended.send(requester.recv_blocking().is_err()).unwrap();
        Ok(OpResult::Done)
    });
    let mut stream = interprocess::local_socket::Stream::connect(name).unwrap();
    let mut line = serde_json::to_vec(&Op::Sidebar).unwrap();
    line.push(b'\n');
    stream.write_all(&line).unwrap();
    let timeout = std::time::Duration::from_secs(5);
    assert!(
        !waiting.recv_timeout(timeout).unwrap(),
        "gone before leaving"
    );
    drop(stream);
    assert!(
        gone.recv_timeout(timeout).unwrap(),
        "the requester stayed open"
    );
}

#[cfg(unix)]
#[test]
fn connect_or_start_uses_the_socket_that_is_already_listening() {
    let dir = tempfile::tempdir().unwrap();
    let location = socket::location_in(dir.path()).unwrap();
    let name = socket::name(&location).unwrap();
    let listener = socket::bind(name.borrow()).unwrap().unwrap();
    socket::serve(listener, |_, _| Ok(OpResult::Done));
    let _guard = socket_var(&location);
    let mut client = Client::connect_or_start().unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(client.call(&Op::Sidebar), Ok(OpResult::Done));
}

#[cfg(unix)]
#[test]
fn a_socket_that_appears_is_accepted_and_a_missed_deadline_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let location = socket::location_in(dir.path()).unwrap();
    let Err(missed) = socket::wait_for_socket(&location, std::time::Duration::ZERO) else {
        panic!("a missing socket answered");
    };
    assert!(missed.raw_os_error().is_some() || missed.kind() == std::io::ErrorKind::NotFound);

    let listen_at = location.clone();
    let ready = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(80));
        socket::bind(socket::name(&listen_at).unwrap())
            .unwrap()
            .unwrap()
    });
    let client = socket::wait_for_socket(&location, std::time::Duration::from_secs(2))
        .unwrap_or_else(|error| panic!("{error}"));
    drop(client);
    drop(ready.join().unwrap());
}

#[cfg(unix)]
#[test]
fn no_start_refuses_to_spawn_an_app() {
    let dir = tempfile::tempdir().unwrap();
    let location = socket::location_in(dir.path()).unwrap();
    let _socket = socket_var(&location);
    let _no_start = set_env("KG_NO_START", std::ffi::OsStr::new("1"));
    let started = std::time::Instant::now();
    let Err(error) = Client::connect_or_start() else {
        panic!("a missing app answered");
    };
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(error.to_string().contains("start it first"), "{error}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "the client waited for an app to start"
    );
    assert!(
        !std::path::Path::new(&location).exists(),
        "a socket appeared; an app was started"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

pub(super) struct EnvVar {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

thread_local! {
    static HELD: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
}

// Removes the thread, project and environment ids this shell passed in, so a test that did not
// set one with `set_env` parses the same inside a kagan thread as anywhere else.
pub(super) fn drop_inherited_ids() {
    for key in ["KG_THREAD_ID", "KG_PROJECT_ID", "KG_ENVIRONMENT_ID"] {
        if !HELD.with_borrow(|held| held.contains(&key)) {
            // SAFETY: nextest runs each test in its own process, so this test owns its variables.
            unsafe { std::env::remove_var(key) };
        }
    }
}

pub(super) fn set_env(key: &'static str, value: &std::ffi::OsStr) -> EnvVar {
    let previous = std::env::var_os(key);
    // SAFETY: nextest runs each test in its own process, so this test owns these
    // process-global variables while the guard is alive and restores them on drop.
    unsafe { std::env::set_var(key, value) };
    HELD.with_borrow_mut(|held| held.push(key));
    EnvVar { key, previous }
}

fn unset_env(key: &'static str) -> EnvVar {
    let previous = std::env::var_os(key);
    // SAFETY: nextest runs each test in its own process, so this test owns these
    // process-global variables while the guard is alive and restores them on drop.
    unsafe { std::env::remove_var(key) };
    EnvVar { key, previous }
}

impl Drop for EnvVar {
    fn drop(&mut self) {
        HELD.with_borrow_mut(|held| held.retain(|key| *key != self.key));
        // SAFETY: nextest runs each test in its own process, so nothing else reads this
        // variable while the guard restores it.
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

#[test]
fn a_data_dir_without_a_socket_keeps_its_own_socket() {
    let data = test_dir("data-socket");
    let _data = set_env("KG_DATA_DIR", data.as_os_str());
    let _socket = unset_env("KG_SOCKET");
    let located = socket::location().unwrap_or_else(|error| panic!("{error}"));
    #[cfg(unix)]
    {
        assert_eq!(located, data.join("kg.sock").into_os_string());
        assert!(!data.exists());
    }
    #[cfg(windows)]
    {
        let text = located.to_string_lossy();
        assert_eq!(text.len(), "kagan-".len() + 64);
        assert!(text.starts_with("kagan-"));
        let user = std::env::var("USERNAME").unwrap_or_default();
        assert_ne!(located, std::ffi::OsString::from(format!("kagan-{user}")));
    }
}

#[test]
fn an_explicit_socket_wins_over_the_data_dir() {
    let data = test_dir("data-socket-explicit");
    let _data = set_env("KG_DATA_DIR", data.as_os_str());
    let chosen = if cfg!(unix) {
        std::ffi::OsString::from("/tmp/kagan-explicit.sock")
    } else {
        std::ffi::OsString::from("kagan-explicit")
    };
    let _socket = set_env("KG_SOCKET", &chosen);
    let located = socket::location().unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(located, chosen);
    #[cfg(unix)]
    assert!(!data.join("kg.sock").exists());
}

#[cfg(unix)]
#[test]
fn a_data_dir_path_that_does_not_fit_uses_a_short_socket() {
    let first = std::path::PathBuf::from(format!("/tmp/{}", "a".repeat(180)));
    let second = std::path::PathBuf::from(format!("/tmp/{}", "b".repeat(180)));
    assert!(first.join("kg.sock").as_os_str().len() > 103);

    let _data = set_env("KG_DATA_DIR", first.as_os_str());
    let _socket = unset_env("KG_SOCKET");
    let located = socket::location().unwrap_or_else(|error| panic!("{error}"));
    let path = std::path::PathBuf::from(&located);
    let text = located.to_string_lossy();
    assert!(text.starts_with("/tmp/kagan-"), "{text}");
    assert!(path.ends_with("kg.sock"));
    assert!(path.as_os_str().len() <= 103);
    assert_ne!(path, first.join("kg.sock"));
    assert!(!text.contains("Application Support"));

    let _other_data = set_env("KG_DATA_DIR", second.as_os_str());
    let other = socket::location().unwrap_or_else(|error| panic!("{error}"));
    assert_ne!(other, located);

    let _ = std::fs::remove_dir_all(path.parent().expect("short socket has a directory"));
    let _ = std::fs::remove_dir_all(
        std::path::Path::new(&other)
            .parent()
            .expect("short socket has a directory"),
    );
}

#[cfg(unix)]
struct Cwd(std::path::PathBuf);

#[cfg(unix)]
impl Cwd {
    fn stay() -> Self {
        Self(std::env::current_dir().unwrap_or_else(|error| panic!("{error}")))
    }
}

#[cfg(unix)]
impl Drop for Cwd {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

#[cfg(unix)]
fn long_data(label: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!(
        "/tmp/{label}-{}-{}",
        std::process::id(),
        "d".repeat(160)
    ))
}

#[cfg(unix)]
fn fallback_socket(label: &str) -> (EnvVar, EnvVar, std::path::PathBuf) {
    let data = long_data(label);
    let data_var = set_env("KG_DATA_DIR", data.as_os_str());
    let socket_var = unset_env("KG_SOCKET");
    let located =
        std::path::PathBuf::from(socket::location().unwrap_or_else(|error| panic!("{error}")));
    (data_var, socket_var, located)
}

#[cfg(unix)]
#[test]
fn a_relative_data_dir_stays_absolute_from_another_folder() {
    let _cwd = Cwd::stay();
    let root = test_dir("rel-root");
    let elsewhere = test_dir("rel-else");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::env::set_current_dir(&root).unwrap();
    let _data = set_env("KG_DATA_DIR", std::ffi::OsStr::new("data"));
    let _socket = unset_env("KG_SOCKET");
    let located = socket::location().unwrap_or_else(|error| panic!("{error}"));
    let path = std::path::PathBuf::from(&located);
    let expected = std::env::current_dir()
        .unwrap_or_else(|error| panic!("{error}"))
        .join("data/kg.sock");
    assert!(path.is_absolute(), "{path:?}");
    assert_eq!(path, expected);

    let listener = socket::open(&located)
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("socket was already open"));
    socket::serve(listener, |_, _| Ok(OpResult::Done));
    std::env::set_current_dir(&elsewhere).unwrap();
    let mut client = Client::connect(socket::name(&located).unwrap()).unwrap();
    assert_eq!(client.call(&Op::Sidebar), Ok(OpResult::Done));
    let _ = std::fs::remove_dir_all(root.join("data"));
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&elsewhere);
}

#[cfg(unix)]
#[test]
fn a_regular_file_at_the_socket_is_left_in_place() {
    let data = test_dir("plain-sock");
    std::fs::create_dir_all(&data).unwrap();
    let sock = data.join("kg.sock");
    std::fs::write(&sock, b"notes").unwrap();
    let error = socket::open(sock.as_os_str()).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&sock).unwrap(), b"notes");
    let _ = std::fs::remove_dir_all(&data);
}

#[cfg(unix)]
#[test]
fn an_untrusted_fallback_directory_is_refused() {
    use std::os::unix::fs::PermissionsExt;

    let (_data, _socket, located) = fallback_socket("writable");
    let dir = located.parent().expect("fallback folder").to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
    let listener = std::os::unix::net::UnixListener::bind(&located).unwrap();
    listener.set_nonblocking(true).unwrap();
    let error = socket::before_connect(located.as_os_str()).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    let Err(error) = Client::connect_or_start() else {
        panic!("connected through an untrusted folder");
    };
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    match listener.accept() {
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
        other => panic!("a client connected to the planted socket: {other:?}"),
    }
    assert_eq!(
        std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o777
    );

    let (_data, _socket, linked) = fallback_socket("linked");
    let link = linked.parent().expect("fallback folder").to_path_buf();
    let real = test_dir("fallback-target");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let error = socket::before_connect(linked.as_os_str()).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(!real.join("kg.sock").exists());

    let (_data, _socket, foreign) = fallback_socket("foreign");
    let owned = foreign.parent().expect("fallback folder").to_path_buf();
    std::fs::create_dir_all(&owned).unwrap();
    std::fs::set_permissions(&owned, std::fs::Permissions::from_mode(0o700)).unwrap();
    if chown_away(&owned) {
        let error = socket::before_connect(foreign.as_os_str()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        let _ = chown_back(&owned);
    }

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&link);
    let _ = std::fs::remove_dir_all(&real);
    let _ = std::fs::remove_dir_all(&owned);
}

#[cfg(unix)]
fn chown_away(dir: &std::path::Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let mut bytes = dir.as_os_str().as_bytes().to_vec();
    bytes.push(0);
    // SAFETY: `bytes` is a NUL-terminated path owned by `dir`, and lchown only reads it.
    unsafe { libc::lchown(bytes.as_ptr().cast(), 65_534, libc::gid_t::MAX) == 0 }
}

#[cfg(unix)]
fn chown_back(dir: &std::path::Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let mut bytes = dir.as_os_str().as_bytes().to_vec();
    bytes.push(0);
    // SAFETY: `bytes` is a NUL-terminated path owned by `dir`, and lchown only reads it.
    unsafe { libc::lchown(bytes.as_ptr().cast(), libc::geteuid(), libc::getegid()) == 0 }
}

#[cfg(unix)]
struct SocketVar(Option<std::ffi::OsString>);

#[cfg(unix)]
fn socket_var(location: &std::ffi::OsStr) -> SocketVar {
    let previous = std::env::var_os("KG_SOCKET");
    // SAFETY: nextest runs each test in its own process, so this test owns the variable while
    // the guard is alive and restores it on drop.
    unsafe { std::env::set_var("KG_SOCKET", location) };
    SocketVar(previous)
}

#[cfg(unix)]
impl Drop for SocketVar {
    fn drop(&mut self) {
        // SAFETY: nextest runs each test in its own process, so nothing else reads this
        // variable while the guard restores it.
        unsafe {
            match self.0.take() {
                Some(value) => std::env::set_var("KG_SOCKET", value),
                None => std::env::remove_var("KG_SOCKET"),
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn a_turn_hook_never_starts_an_app_and_never_fails() {
    let dir = tempfile::tempdir().unwrap();
    let location = socket::location_in(dir.path()).unwrap();
    let _socket = socket_var(&location);
    let started = std::time::Instant::now();
    let op = kg_domain::Op::Thread(kg_domain::ThreadOp::Turn(kg_domain::TurnOp::Start {
        id: kg_domain::ThreadId("thr_hook".into()),
    }));
    crate::cli::run(op, false).expect("a turn hook must not fail the agent's turn");
    assert!(
        !std::path::Path::new(&location).exists(),
        "a turn hook started an app"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}

#[cfg(unix)]
#[test]
fn a_socket_path_over_the_limit_says_to_set_a_shorter_one() {
    let long = std::path::PathBuf::from(format!("/tmp/{}/kg.sock", "a".repeat(200)));
    let error = socket::name(long.as_os_str()).expect_err("the path is too long");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(
        error
            .to_string()
            .contains("set KG_SOCKET to a shorter path")
    );
}
