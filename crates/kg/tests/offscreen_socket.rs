#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test stops at the first broken step"
)]

use std::os::unix::net::UnixListener;
use std::process::Command;

#[test]
fn offscreen_refuses_a_socket_another_app_listens_on() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("kg.sock");
    let _other_app = UnixListener::bind(&socket).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kg"))
        .arg("--offscreen")
        .env("KG_SOCKET", &socket)
        .env("KG_DATA_DIR", dir.path().join("data"))
        .env_remove("KG_CLI")
        .env_remove("KG_ENVIRONMENT_ID")
        .env_remove("KG_PROJECT_ID")
        .env_remove("KG_THREAD_ID")
        .output()
        .unwrap();

    assert!(!output.status.success(), "an app is already there");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already listening"), "{stderr}");
}
