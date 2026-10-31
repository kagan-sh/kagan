#![cfg(unix)]

use std::process::Command;

#[test]
fn mcp_without_a_running_instance_fails_instead_of_starting_one() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_kg"))
        .args(["mcp", "--thread", "thr_1"])
        .env_clear()
        .env("HOME", dir.path())
        .env("XDG_RUNTIME_DIR", dir.path().join("run"))
        .env("KG_DATA_DIR", dir.path().join("data"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!dir.path().join("data").exists());
}
