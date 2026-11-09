#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test stops at the first broken step"
)]

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(unix)]
#[test]
fn documented_first_run_commands_run() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let socket = PathBuf::from(format!("/tmp/kg-first-run-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);
    let log = dir.path().join("server.log");
    let home = data.parent().unwrap();
    let mut server = Server::start(home, Some(&data), &socket, &log);
    server.wait_until_listening();

    for command in first_run_commands() {
        let output = kg(&command, home, Some(&data), &socket);
        assert!(
            output.status.success(),
            "{command:?}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    let listed = kg(
        &["kg".into(), "project".into(), "list".into()],
        home,
        Some(&data),
        &socket,
    );
    assert!(
        String::from_utf8_lossy(&listed.stdout).contains("personal"),
        "{}",
        String::from_utf8_lossy(&listed.stdout),
    );
    let status = kg(
        &["kg".into(), "status".into(), "--json".into()],
        home,
        Some(&data),
        &socket,
    );
    let value: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(
        value["status"]["dataDir"],
        std::fs::canonicalize(&data)
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );
    assert!(data.join("kagan.sqlite3").is_file());
    drop(server);
    let _ = std::fs::remove_file(&socket);
}

#[cfg(unix)]
#[test]
fn the_app_keeps_its_data_in_home_kagan_without_kg_data_dir() {
    let home = tempfile::tempdir().unwrap();
    let socket = PathBuf::from(format!("/tmp/kg-home-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);
    let log = home.path().join("server.log");
    let status = ["kg".to_owned(), "status".into(), "--json".into()];

    let mut server = Server::start(home.path(), None, &socket, &log);
    server.wait_until_listening();
    let output = kg(&status, home.path(), None, &socket);
    assert!(
        output.status.success(),
        "{}",
        server.failure("kg status failed")
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let kagan = home.path().join(".kagan");
    assert_eq!(
        value["status"]["dataDir"],
        std::fs::canonicalize(&kagan)
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );
    assert!(kagan.join("kagan.sqlite3").is_file());
    drop(server);
}

#[cfg(unix)]
fn first_run_commands() -> Vec<Vec<String>> {
    let book = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../book/src/start.md");
    let text = std::fs::read_to_string(&book).unwrap();
    let mut commands = Vec::new();
    let mut fence = false;
    let mut take = false;
    for line in text.lines() {
        if line.starts_with("```") {
            fence = !fence;
            take = false;
            continue;
        }
        if fence && line == "# Check that kg works" {
            take = true;
            continue;
        }
        if take {
            let words: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
            assert!(
                words.first().is_some_and(|word| word == "kg"),
                "first-run line is not a kg command: {line}"
            );
            commands.push(words);
        }
    }
    assert!(
        commands.len() >= 2,
        "book/src/start.md has no first-run commands"
    );
    commands
}

#[cfg(unix)]
fn kg(args: &[String], home: &Path, data: Option<&Path>, socket: &Path) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kg"));
    command.args(args.iter().skip(1));
    environment(&mut command, home, data, socket);
    command.output().unwrap()
}

#[cfg(unix)]
fn environment(command: &mut Command, home: &Path, data: Option<&Path>, socket: &Path) {
    command
        .env("HOME", home)
        .env("KG_SOCKET", socket)
        .env_remove("KG_DATA_DIR")
        .env_remove("KG_THREAD_ID")
        .env_remove("KG_PROJECT_ID")
        .env_remove("KG_ENVIRONMENT_ID");
    if let Some(data) = data {
        command.env("KG_DATA_DIR", data);
    }
}

#[cfg(unix)]
struct Server {
    child: Child,
    socket: PathBuf,
    log: PathBuf,
}

#[cfg(unix)]
impl Server {
    fn start(home: &Path, data: Option<&Path>, socket: &Path, log: &Path) -> Self {
        let err = File::create(log).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_kg"));
        command.arg("--headless");
        environment(&mut command, home, data, socket);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(err);
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let child = command.spawn().unwrap();
        Self {
            child,
            socket: socket.to_path_buf(),
            log: log.to_path_buf(),
        }
    }

    fn wait_until_listening(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !self.socket.exists() {
            assert!(
                Instant::now() <= deadline,
                "{}",
                self.failure("headless kg did not open its socket")
            );
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "{}",
                self.failure("headless kg exited")
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn failure(&self, message: &str) -> String {
        let log = std::fs::read_to_string(&self.log).unwrap_or_default();
        format!("{message}\n{log}")
    }
}

#[cfg(unix)]
impl Drop for Server {
    fn drop(&mut self) {
        let pid = self.child.id();
        let _ = Command::new("kill")
            // Without `--`, procps reads -<pid> as an option and keeps one digit; -1 kills every process we own.
            .args(["-9", "--", &format!("-{pid}")])
            .output();
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
    }
}
