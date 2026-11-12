// Windows only: a started app must not keep the caller's stdout pipe open. Before the fix the
// headless app inherited the pipe, so reading it to the end never finished.
#![cfg(windows)]

use std::{
    io::Read,
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[test]
fn a_started_app_does_not_keep_the_callers_pipe_open() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("kg-pipe-eof-{}-{nanos}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_kg"))
        .arg("guide")
        .env("KG_DATA_DIR", &dir)
        .env_remove("KG_SOCKET")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = Vec::new();
        _ = stdout.read_to_end(&mut text);
        _ = done.send(());
    });
    let reached_end = finished.recv_timeout(Duration::from_secs(30)).is_ok();
    _ = child.wait();
    if let Ok(pid) = std::fs::read_to_string(dir.join("kagan.pid")) {
        _ = Command::new("taskkill")
            .args(["/PID", pid.trim(), "/F"])
            .status();
    }
    _ = std::fs::remove_dir_all(&dir);
    assert!(
        reached_end,
        "the pipe did not reach its end: the app kept it open"
    );
}
