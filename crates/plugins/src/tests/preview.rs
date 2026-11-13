use std::path::Path;

use gpui_kit::TestAppContext;
use kg_core::{previews_dir, seed_preview};
use kg_domain::{OpError, Plugin, PluginId, PluginOp, PluginPreviewOp, prefs};
use kg_store::Store;
use serde_json::json;

use crate::host::Hold;
use crate::manifest::Grant;

use super::{
    ID, answer, hold_at, open_kept, plugin_dir, plugin_op, read_manifest, records, thread,
};

fn start(path: &Path) -> PluginOp {
    PluginOp::Preview {
        op: PluginPreviewOp::Start {
            path: path.to_path_buf(),
            thread_id: Some(thread()),
        },
    }
}

fn preview_folders(data: &Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(previews_dir(data))
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[gpui_kit::test]
fn declining_a_preview_writes_nothing_and_starts_nothing(cx: &mut TestAppContext) {
    {
        let plugin = plugin_dir();
        let data = tempfile::tempdir().unwrap();
        let path = data.path().to_path_buf();
        open_kept(cx, &path, data, &[]);

        let started = plugin_op(cx, start(plugin.path()));
        answer(cx, "decline");

        assert!(cx.foreground_executor().block_test(started).is_err());
        assert!(
            preview_folders(&path).is_empty(),
            "a declined preview left a folder"
        );
    }
}

#[gpui_kit::test]
fn approving_a_preview_seeds_its_folder_with_the_approved_grant(cx: &mut TestAppContext) {
    {
        let plugin = plugin_dir();
        let root = dunce::canonicalize(plugin.path()).unwrap();
        let data = tempfile::tempdir().unwrap();
        let path = data.path().to_path_buf();
        open_kept(cx, &path, data, &[]);
        let release = hold_at(cx, Hold::PreviewStart);

        let _started = plugin_op(cx, start(&root));
        answer(cx, "approve");

        let folders = preview_folders(&path);
        let [folder] = folders.as_slice() else {
            panic!("one preview folder: {folders:?}");
        };
        let store = Store::open(folder.join("data").join("kagan.sqlite3")).unwrap();
        let grant = Grant::of(&read_manifest(&root).unwrap(), &root);
        let plugins = store.plugins().unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].id, PluginId(ID.into()));
        assert_eq!(plugins[0].approved, serde_json::to_value(&grant).unwrap());
        assert_eq!(store.setting(prefs::PREVIEW).unwrap(), Some(json!(true)));
        drop(release);
    }
}

#[gpui_kit::test]
fn a_preview_refuses_a_reload_whose_powers_grew_and_keeps_the_approved_code(
    cx: &mut TestAppContext,
) {
    {
        let plugin = plugin_dir();
        let root = dunce::canonicalize(plugin.path()).unwrap();
        let data = tempfile::tempdir().unwrap();
        let path = data.path().to_path_buf();
        let record = Plugin {
            id: PluginId(ID.into()),
            source: root.display().to_string(),
            root: root.clone(),
            commit: None,
            enabled: true,
            approved: serde_json::to_value(Grant::default()).unwrap(),
            skills: Vec::new(),
            error: None,
        };
        seed_preview(&path, &record).unwrap();
        open_kept(cx, &path, data, &[]);

        let reloaded = plugin_op(
            cx,
            PluginOp::Reload {
                id: PluginId(ID.into()),
                thread_id: None,
            },
        );
        cx.run_until_parked();
        let Err(OpError::Invalid(message)) = cx.foreground_executor().block_test(reloaded) else {
            panic!("a preview reload with grown powers did not fail");
        };
        assert!(
            message.contains("run kg plugin preview start again so the person can approve them"),
            "{message}"
        );
        let kept = records(cx);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].approved, record.approved, "the old grant changed");
    }
}
