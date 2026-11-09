use kg_domain::{Plugin, PluginId};
use serde_json::json;

use super::store;

#[test]
fn plugin_values() {
    let store = store();
    let plugin = PluginId("dev.kagan.example".into());
    assert_eq!(store.plugin_value(&plugin, "label").unwrap(), None);
    store
        .set_plugin_value(&plugin, "label", &json!("one"))
        .unwrap();
    store
        .set_plugin_value(&plugin, "label", &json!("two"))
        .unwrap();
    assert_eq!(
        store.plugin_value(&plugin, "label").unwrap(),
        Some(json!("two"))
    );
}

#[test]
fn plugin_enabled_and_commit_update_the_same_row() {
    let store = store();
    let mut plugin = Plugin {
        id: PluginId("dev.kagan.canvas".into()),
        root: "/plugins/canvas".into(),
        source: "git:https://example.com/p.git@v1".into(),
        commit: Some("abc".into()),
        enabled: true,
        approved: json!({ "powers": { "serve": false } }),
        skills: vec!["skills/demo".into()],
        error: None,
    };
    store.insert_plugin(&plugin).unwrap();
    plugin.enabled = false;
    plugin.commit = None;
    plugin.skills = vec!["skills/next".into()];
    store
        .set_plugin_enabled(&plugin.id, plugin.enabled)
        .unwrap();
    store
        .keep_plugin_checkout(
            &plugin.id,
            &plugin.root,
            &plugin.source,
            &plugin.commit,
            &plugin.approved,
            &plugin.skills,
        )
        .unwrap();
    assert_eq!(store.plugins().unwrap(), vec![plugin.clone()]);
    plugin.skills.clear();
    store.set_plugin_skills(&plugin.id, &plugin.skills).unwrap();
    assert_eq!(store.plugins().unwrap(), vec![plugin]);
}
