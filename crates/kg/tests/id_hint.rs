use std::process::Command;

use serde_json::Value;

fn kg(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new(env!("CARGO_BIN_EXE_kg"))
        .args(args)
        .env_remove("KG_THREAD_ID")
        .env_remove("KG_PROJECT_ID")
        .env_remove("KG_ENVIRONMENT_ID")
        .output()
}

#[test]
fn a_missing_thread_id_prints_a_hint() {
    let output = kg(&["thread", "get", "--json"]).unwrap();
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{error}; status={:?}; stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(value["error"]["kind"], "invalid");
    assert!(
        value["error"]["message"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );
    assert_eq!(value["error"]["hint"], "set KG_THREAD_ID, or pass the id");
    assert!(value.get("ok").is_none());

    let project: Value =
        serde_json::from_slice(&kg(&["project", "get", "--json"]).unwrap().stdout).unwrap();
    assert_eq!(
        project["error"]["hint"],
        "set KG_PROJECT_ID, or pass the id"
    );
    let environment: Value =
        serde_json::from_slice(&kg(&["environment", "get", "--json"]).unwrap().stdout).unwrap();
    assert_eq!(
        environment["error"]["hint"],
        "set KG_ENVIRONMENT_ID, or pass the id"
    );

    let run: Value =
        serde_json::from_slice(&kg(&["--json", "thread", "run"]).unwrap().stdout).unwrap();
    assert_eq!(run["error"]["kind"], "invalid");
    assert!(run["error"].get("hint").is_none(), "{run}");
    assert!(run.get("ok").is_none());
}

#[test]
fn a_missing_plugin_name_gives_an_example() {
    let output = kg(&["plugin", "new"]).unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Try: kg plugin new hello"),
        "status={:?}; stderr={stderr}",
        output.status
    );
}
