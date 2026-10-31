#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixListener;
use std::process::Command;

fn call(args: &[&str]) -> std::process::Output {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("kg.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut request)
            .unwrap();
        stream.write_all(b"\"done\"\n").unwrap();
    });
    let output = Command::new(env!("CARGO_BIN_EXE_kg"))
        .args(args)
        .env("KG_SOCKET", socket)
        .env_remove("KG_THREAD_ID")
        .env_remove("KG_PROJECT_ID")
        .output()
        .unwrap();
    server.join().unwrap();
    output
}

#[test]
fn plugin_authoring_prints_next_steps() {
    let created = call(&["plugin", "new", "hello"]);
    assert!(created.status.success());
    assert!(String::from_utf8_lossy(&created.stdout).contains("Next: kg plugin check hello"));
    let checked = call(&["plugin", "check", "hello"]);
    assert!(checked.status.success());
    assert!(String::from_utf8_lossy(&checked.stdout).contains("Next: kg plugin install ./hello"));
}

#[test]
fn approval_commands_describe_what_may_happen() {
    for (args, message) in [
        (&["plugin", "install", "hello"][..], "Installing hello"),
        (&["plugin", "remove", "hello"][..], "Removing hello"),
        (&["project", "remove", "prj_1"][..], "Removing prj_1"),
    ] {
        let output = call(args);
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            format!("{message}. kagan may ask for your OK in its window.\n")
        );

        let mut json_args = args.to_vec();
        json_args.push("--json");
        let output = call(&json_args);
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            format!("{message}. kagan may ask for your OK in its window.\n")
        );
        let stdout: serde_json::Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|error| panic!("stdout is not JSON: {error}"));
        assert_eq!(stdout, serde_json::json!("done"));
    }
}
