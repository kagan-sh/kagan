use gpui_kit::TestAppContext;
use kg_domain::{Plugin, PluginId};
use serde_json::json;

use super::open_at;
use crate::seed_preview;

// The same open the app does at start, on a folder made by `seed_preview`.
#[gpui_kit::test]
async fn a_seeded_preview_folder_opens_with_its_plugin_enabled_and_approved(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let approved = json!({ "clipboard": { "write": true } });
    let plugin = Plugin {
        id: PluginId("dev.test.panel".into()),
        root: dir.path().join("panel"),
        source: dir.path().join("panel").display().to_string(),
        commit: None,
        enabled: true,
        approved: approved.clone(),
        skills: Vec::new(),
        error: None,
    };
    seed_preview(dir.path(), &plugin).unwrap();

    let workspace = open_at(&dir, cx, |_| {}).await.unwrap();
    let plugins = cx.read(|cx| workspace.read(cx).plugins()).await.unwrap();
    let preview = cx.read(|cx| workspace.read(cx).is_preview()).await.unwrap();

    assert_eq!(plugins.len(), 1, "{plugins:?}");
    assert!(plugins[0].enabled, "the seeded plugin is not enabled");
    assert_eq!(plugins[0].approved, approved);
    assert!(preview, "the folder does not say it is a preview");
}
