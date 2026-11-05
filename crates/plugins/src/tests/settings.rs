use std::path::Path;

use gpui_kit::TestAppContext;
use kg_domain::{ConfigOp, Op, OpResult, PluginId, PluginOp};
use serde_json::{Value, json};

use super::{ID, installed, open, read_manifest, run, write_plugin};
use crate::manifest::Setting;

fn manifest(settings: &Value) -> Value {
    json!({
        "id": ID,
        "name": "Echo",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Echo things back.",
        "powers": {},
        "settings": settings,
    })
}

fn plugin(settings: &Value) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_plugin(dir.path(), &manifest(settings));
    dir
}

fn refused(root: &Path) -> String {
    read_manifest(root).unwrap_err().to_string()
}

#[test]
fn a_string_setting_may_be_multiline_and_a_number_may_be_schema_checked() {
    let dir = plugin(&json!({
        "notes": {
            "type": "string",
            "title": "Notes",
            "multiline": true,
            "default": "one\ntwo",
            "schema": { "type": "string", "minLength": 3 }
        },
        "retries": {
            "type": "number",
            "title": "Retries",
            "default": 3,
            "schema": { "type": "number", "minimum": 1, "maximum": 5 }
        }
    }));
    let manifest = read_manifest(dir.path()).unwrap();
    let notes = manifest.settings.get("notes").unwrap();
    assert!(matches!(
        notes,
        Setting::String {
            multiline: true,
            ..
        }
    ));
    assert!(notes.schema().is_some());
    let retries = manifest.settings.get("retries").unwrap();
    assert!(retries.schema().is_some());
}

#[test]
fn multiline_and_schema_are_refused_on_the_wrong_setting_and_a_bad_schema_names_its_key() {
    let cases = [
        (
            json!({ "token": { "type": "secret", "title": "Token", "multiline": true } }),
            "setting token cannot be multiline",
        ),
        (
            json!({ "sounds": { "type": "boolean", "title": "Sounds", "schema": {} } }),
            "setting sounds: schema works on a string or number setting only",
        ),
        (
            json!({ "notes": { "type": "folder", "title": "Notes", "schema": {} } }),
            "setting notes: schema works on a string or number setting only",
        ),
        (
            json!({ "notes": { "type": "string", "title": "Notes", "schema": [] } }),
            "setting notes schema must be a JSON object",
        ),
        (
            json!({ "notes": { "type": "string", "title": "Notes", "schema": { "type": "nonsense" } } }),
            "setting notes schema does not compile",
        ),
    ];
    for (settings, expected) in cases {
        let dir = plugin(&settings);
        let error = refused(dir.path());
        assert!(error.contains(expected), "{settings}: {error}");
    }
}

#[test]
fn a_default_that_fails_its_schema_is_refused_with_the_key() {
    let dir = plugin(&json!({
        "retries": {
            "type": "number",
            "title": "Retries",
            "default": 9,
            "schema": { "type": "number", "maximum": 5 }
        }
    }));
    let error = refused(dir.path());
    assert!(error.contains("setting retries (Retries)"), "{error}");
    assert!(error.contains("schema"), "{error}");
}

fn config(op: ConfigOp) -> Op {
    Op::Plugin(PluginOp::Config {
        id: PluginId(ID.into()),
        op,
        thread_id: None,
    })
}

#[gpui_kit::test]
fn config_set_checks_strings_and_numbers_against_the_schema(cx: &mut TestAppContext) {
    let dir = plugin(&json!({
        "notes": {
            "type": "string",
            "title": "Notes",
            "multiline": true,
            "schema": { "type": "string", "maxLength": 10 }
        },
        "tag": {
            "type": "string",
            "title": "Tag",
            "schema": { "enum": ["alpha", "beta"] }
        },
        "retries": {
            "type": "number",
            "title": "Retries",
            "schema": { "type": "number", "minimum": 1, "maximum": 5 }
        },
        "name": {
            "type": "string",
            "title": "Name",
            "schema": { "type": "string", "minLength": 1 }
        },
        "workers": {
            "type": "number",
            "title": "Workers",
            "schema": { "type": "integer" }
        }
    }));
    open(cx, &[installed(dir.path())]);

    for (key, value) in [
        ("notes", json!("one\ntwo")),
        ("tag", json!("alpha")),
        ("retries", json!(3)),
        ("retries", json!(2.5)),
        ("name", json!("kagan")),
        ("workers", json!(2)),
    ] {
        run(
            cx,
            config(ConfigOp::Set {
                key: key.into(),
                value: Some(value.clone()),
            }),
        )
        .unwrap();
        assert_eq!(
            run(cx, config(ConfigOp::Get { key: key.into() })).unwrap(),
            OpResult::Setting(value)
        );
    }

    for (key, value) in [
        ("notes", json!("far too many characters")),
        ("tag", json!("gamma")),
        ("retries", json!(9)),
        ("name", json!("")),
        ("workers", json!(2.5)),
    ] {
        let error = run(
            cx,
            config(ConfigOp::Set {
                key: key.into(),
                value: Some(value),
            }),
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains(&format!("setting {key}")), "{message}");
        assert!(message.contains("does not match its schema"), "{message}");
    }
}
