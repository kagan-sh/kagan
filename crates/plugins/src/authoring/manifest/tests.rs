use serde_json::{Value, json};

use super::fetch::{Area, Fetch};
use super::*;

fn parsed(extra: &Value) -> Manifest {
    let mut value = json!({
        "id": "dev.test",
        "name": "Test",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
    });
    let object = value.as_object_mut().expect("object");
    for (key, item) in extra.as_object().expect("extra") {
        object.insert(key.clone(), item.clone());
    }
    serde_json::from_value(value).expect("manifest")
}

#[test]
fn commands_need_an_id_a_title_and_a_unique_id() {
    for commands in [
        json!([{ "id": "", "title": "Title" }]),
        json!([{ "id": "go", "title": " " }]),
        json!([{ "id": "go", "title": "Go" }, { "id": "go", "title": "Again" }]),
    ] {
        let manifest = parsed(&json!({ "commands": commands }));
        assert!(
            matches!(manifest.check_commands(), Err(Error::Commands)),
            "{commands}"
        );
    }
    let manifest = parsed(&json!({ "commands": [{ "id": "go", "title": "Go" }] }));
    assert!(manifest.check_commands().is_ok());
}

#[test]
fn hotkey_bindings_are_reserved() {
    let manifest = parsed(&json!({
        "settings": { "$hotkeys": { "type": "boolean", "title": "Keys" } }
    }));
    assert!(matches!(
        manifest.check_hotkeys(),
        Err(Error::ReservedHotkeys)
    ));
    assert!(parsed(&json!({})).check_hotkeys().is_ok());
}

#[test]
fn tool_names_are_short_identifiers() {
    let longest = "a".repeat(64);
    let too_long = "a".repeat(65);
    for name in ["a", "a-b_c", "A1", longest.as_str()] {
        assert!(tool_name(name), "{name}");
    }
    for name in ["", too_long.as_str(), "a.b", "a b", "ä"] {
        assert!(!tool_name(name), "{name}");
    }
}

#[test]
fn plain_lines_reject_control_characters() {
    assert!(plain_line("Runs git").is_ok());
    assert!(plain_line("line\nbreak").is_err());
    assert!(plain_line("tab\there").is_err());
}

#[test]
fn native_programs_are_bare_names() {
    let bare = parsed(&json!({ "powers": { "run": [{ "program": "git" }] } }));
    assert!(bare.check_native().is_ok());
    for program in ["", "bin/git", "../git"] {
        let manifest = parsed(&json!({ "powers": { "run": [{ "program": program }] } }));
        assert!(
            matches!(manifest.check_native(), Err(Error::RunProgram)),
            "{program}"
        );
    }
}

#[test]
fn local_fetches_stop_at_the_cap() {
    let fetch = |port: usize| {
        json!({
            "host": format!("http://127.0.0.1:{port}"),
            "methods": ["GET"],
        })
    };
    let at_cap = (1..=LOCAL_FETCH_CAP).map(fetch).collect::<Vec<_>>();
    assert!(
        parsed(&json!({ "powers": { "fetch": at_cap } }))
            .check_fetch()
            .is_ok()
    );
    let over = (1..=LOCAL_FETCH_CAP + 1).map(fetch).collect::<Vec<_>>();
    assert!(matches!(
        parsed(&json!({ "powers": { "fetch": over } })).check_fetch(),
        Err(Error::LocalFetch(_))
    ));
}

#[test]
fn select_options_need_a_default_that_exists() {
    let empty = parsed(&json!({
        "settings": { "pick": { "type": "select", "title": "Pick", "options": [] } }
    }));
    assert!(matches!(empty.check_options(), Err(Error::Options(key)) if key == "pick"));
    let missing = parsed(&json!({
        "settings": { "pick": {
            "type": "select", "title": "Pick", "options": ["a"], "default": "b"
        } }
    }));
    assert!(matches!(missing.check_options(), Err(Error::Options(_))));
    let good = parsed(&json!({
        "settings": { "pick": {
            "type": "select", "title": "Pick", "options": ["a"], "default": "a"
        } }
    }));
    assert!(good.check_options().is_ok());
}

#[test]
fn a_secret_key_may_be_exactly_the_maximum() {
    let build = |key: &str| {
        let mut settings = serde_json::Map::new();
        settings.insert(
            key.to_owned(),
            json!({ "type": "secret", "title": "Secret" }),
        );
        parsed(&json!({ "settings": settings }))
    };
    let longest = "k".repeat(crate::secret::MAX_KEY);
    let too_long = "k".repeat(crate::secret::MAX_KEY + 1);
    assert!(build(&longest).check_options().is_ok());
    assert!(matches!(
        build(&too_long).check_options(),
        Err(Error::SecretKey { .. })
    ));
}

#[cfg(unix)]
#[test]
fn skills_must_stay_inside_the_plugin() {
    let dir = tempfile::tempdir().expect("temp");
    let root = dir.path().join("plugin");
    std::fs::create_dir_all(root.join("skills")).expect("skills");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).expect("outside");
    std::fs::write(
        outside.join("SKILL.md"),
        "---\nname: outside\ndescription: Outside\n---\nBody",
    )
    .expect("skill");
    std::os::unix::fs::symlink(&outside, root.join("skills/escaped")).expect("link");
    let manifest = parsed(&json!({ "skills": ["skills/escaped"] }));
    assert!(matches!(
        manifest.check_skills(&root),
        Err(Error::SkillMissing(_))
    ));
}

#[test]
fn the_approval_text_lists_what_the_plugin_uses() {
    let manifest = parsed(&json!({
        "powers": {
            "run": [{ "program": "git" }],
            "fetch": [{ "host": "example.com", "methods": ["GET"] }],
            "read": ["notes"],
        },
        "settings": { "sounds": { "type": "boolean", "title": "Sounds" } },
    }));
    let shown = manifest.shown().collect::<Vec<_>>();
    for expected in ["Test", "1.0.0", "git", "example.com", "notes", "Sounds"] {
        assert!(shown.contains(&expected), "{expected} in {shown:?}");
    }
}

fn fetch(host: &str) -> Fetch {
    serde_json::from_value(serde_json::json!({ "host": host, "methods": ["GET"] })).expect("fetch")
}

#[test]
fn a_remote_host_is_shown_as_declared() {
    assert_eq!(fetch("example.com").view_host().unwrap(), "example.com");
}

#[test]
fn a_loopback_host_is_shown_without_its_scheme_and_port() {
    assert_eq!(
        fetch("http://127.0.0.1:8080").view_host().unwrap(),
        "127.0.0.1"
    );
    assert_eq!(
        fetch("http://LOCALHOST:8080").view_host().unwrap(),
        "localhost"
    );
}

#[test]
fn each_area_has_a_name_and_a_phrase() {
    let names = Area::ALL.iter().map(|area| area.name()).collect::<Vec<_>>();
    assert_eq!(names, ["Threads", "Database", "Process", "Browser"]);
    for area in Area::ALL {
        assert!(!area.id().is_empty());
        assert!(!area.phrase().is_empty());
    }
}
