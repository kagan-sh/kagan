use kg_domain::{Plugin, PluginId, prefs};
use kg_store::Store;
use serde_json::json;

use super::seed_preview;

#[test]
fn a_seeded_folder_holds_its_approved_plugin_and_the_preview_preference() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let plugin = Plugin {
        id: PluginId("dev.kagan.panel".into()),
        root: dir.path().join("panel"),
        source: dir.path().join("panel").display().to_string(),
        commit: None,
        enabled: true,
        approved: json!({ "grant": "panel" }),
        skills: Vec::new(),
        error: None,
    };

    seed_preview(&data, &plugin).unwrap();

    let store = Store::open(data.join("kagan.sqlite3")).unwrap();
    assert_eq!(store.plugins().unwrap(), vec![plugin]);
    assert_eq!(
        store.setting(prefs::PREVIEW).unwrap(),
        Some(json!(true)),
        "a preview folder must say it is one"
    );
}
