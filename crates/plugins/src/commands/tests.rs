use std::collections::BTreeMap;

use kg_domain::{OpError, PluginId};
use serde_json::{Value, json};

use super::{Command, CommandContribution, CommandWhen};

fn read(commands: &Value) -> Result<crate::manifest::Manifest, OpError> {
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let manifest = json!({
        "id": "dev.test.menu",
        "name": "Menu",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "commands": commands,
    });
    std::fs::write(dir.path().join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        dir.path().join("main.js"),
        "export default class Plugin {}\n",
    )
    .unwrap();
    crate::manifest::read(dir.path(), data.path())
}

#[test]
fn a_menu_command_needs_when_thread() {
    for when in [None, Some("always"), Some("project")] {
        let mut command = json!({ "id": "notes", "title": "Notes", "menu": true });
        if let Some(when) = when {
            command["when"] = when.into();
        }
        let error = read(&json!([command])).unwrap_err().to_string();
        assert!(
            error.contains("a menu command needs when thread"),
            "{when:?}: {error}"
        );
    }
}

#[test]
fn a_menu_command_is_accepted_with_when_thread() {
    let manifest = read(&json!([
        { "id": "notes", "title": "Notes", "when": "thread", "menu": true }
    ]))
    .unwrap();
    assert!(manifest.commands[0].menu);
    assert!(!manifest.commands[0].header);
}

#[test]
fn a_command_carries_its_placement_to_its_contribution() {
    let command = |header, menu| Command {
        id: "notes".into(),
        title: "Notes".into(),
        key: None,
        when: CommandWhen::Thread,
        header,
        menu,
    };
    let plugin = PluginId("dev.test.menu".into());
    let both = CommandContribution::of(&plugin, &command(true, true), &BTreeMap::new());
    assert!(both.header && both.menu);
    let neither = CommandContribution::of(&plugin, &command(false, false), &BTreeMap::new());
    assert!(!neither.header && !neither.menu);
}
