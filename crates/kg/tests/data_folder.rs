#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test stops at the first broken step"
)]

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

// Stops the app it wraps, so a failed test leaves no app behind.
struct App(Child);

impl Drop for App {
    fn drop(&mut self) {
        _ = self.0.kill();
        _ = self.0.wait();
    }
}

fn kg(data: &Path, socket: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kg"));
    command
        .env("KG_DATA_DIR", data)
        .env("KG_SOCKET", socket)
        .env_remove("KG_CLI")
        .env_remove("KG_ENVIRONMENT_ID")
        .env_remove("KG_PROJECT_ID")
        .env_remove("KG_THREAD_ID")
        .env_remove("KG_NO_START")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

// The pid the first app wrote once it held the folder.
fn owner_pid(data: &Path, first: &mut Child) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(text) = std::fs::read_to_string(data.join("kagan.pid"))
            && let Ok(pid) = text.trim().parse()
        {
            return pid;
        }
        assert!(
            first.try_wait().unwrap().is_none(),
            "the first app stopped before it took the folder"
        );
        assert!(
            Instant::now() < deadline,
            "the first app never took the folder"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_exit(child: &mut Child, limit: Duration) -> ExitStatus {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() > deadline {
            _ = child.kill();
            _ = child.wait();
            panic!("the second app kept running on a folder another app holds");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn read_stderr(child: &mut Child) -> String {
    let mut text = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        _ = stderr.read_to_string(&mut text);
    }
    text
}

fn first_app(dir: &Path) -> (App, PathBuf) {
    let data = dir.join("data");
    let app = kg(&data, &dir.join("first.sock"))
        .arg("--offscreen")
        .spawn()
        .unwrap();
    (App(app), data)
}

#[test]
fn a_second_app_on_one_data_folder_is_refused_and_names_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let (mut first, data) = first_app(dir.path());
    let pid = owner_pid(&data, &mut first.0);

    let mut second = kg(&data, &dir.path().join("second.sock"))
        .arg("--offscreen")
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let status = wait_for_exit(&mut second, Duration::from_secs(60));
    let stderr = read_stderr(&mut second);

    assert!(!status.success(), "{stderr}");
    assert!(stderr.contains(&format!("in use by pid {pid}")), "{stderr}");
}

#[test]
fn a_client_that_cannot_start_the_app_names_the_folder_holder() {
    let dir = tempfile::tempdir().unwrap();
    let (mut first, data) = first_app(dir.path());
    let pid = owner_pid(&data, &mut first.0);

    let output = kg(&data, &dir.path().join("third.sock"))
        .args(["project", "list"])
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains(&format!("in use by pid {pid}")), "{stderr}");
}
