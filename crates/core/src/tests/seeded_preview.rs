use gpui_kit::TestAppContext;
use kg_domain::{Op, Plugin, PluginId, prefs};
use serde_json::json;

use super::open_at;
use crate::{Workspace, seed_preview};

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
    let preview = cx.read(|cx| workspace.read(cx).is_preview());

    assert_eq!(plugins.len(), 1, "{plugins:?}");
    assert!(plugins[0].enabled, "the seeded plugin is not enabled");
    assert_eq!(plugins[0].approved, approved);
    assert!(preview, "the folder does not say it is a preview");
}

// The preview marker is not a preference: a person's kagan cannot read or write it through `kg`.
#[gpui_kit::test]
async fn kg_preference_refuses_the_preview_marker(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let plugin = Plugin {
        id: PluginId("dev.test.panel".into()),
        root: dir.path().join("panel"),
        source: dir.path().join("panel").display().to_string(),
        commit: None,
        enabled: true,
        approved: json!({}),
        skills: Vec::new(),
        error: None,
    };
    seed_preview(dir.path(), &plugin).unwrap();
    let _workspace = open_at(&dir, cx, |_| {}).await.unwrap();

    let read = Op::Preference(kg_domain::PreferenceOp::Get {
        key: prefs::PREVIEW.into(),
    });
    let write = Op::Preference(kg_domain::PreferenceOp::Set {
        key: prefs::PREVIEW.into(),
        value: json!(false),
    });
    for op in [read, write] {
        let answer = cx.update(|cx| Workspace::dispatch(op, cx)).await;
        assert!(
            answer.is_err(),
            "kg preference reached the preview marker: {answer:?}"
        );
    }
}
