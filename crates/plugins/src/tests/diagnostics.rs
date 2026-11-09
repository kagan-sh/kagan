use gpui_kit::TestAppContext;
use kg_domain::{DiagnosticsOp, Op, OpResult, PluginId};

use crate::log::{Level, log};

use super::{ID, installed, open_kept, plugin_dir, run};

fn export(cx: &mut TestAppContext) -> String {
    let out = tempfile::tempdir().unwrap();
    let path = out.path().join("diagnostics.txt");
    let OpResult::Diagnostics { path } = run(
        cx,
        Op::Diagnostics(DiagnosticsOp::Export { path: Some(path) }),
    )
    .unwrap() else {
        panic!("diagnostics")
    };
    std::fs::read_to_string(path).unwrap()
}

#[gpui_kit::test]
fn diagnostics_counts_the_plugin_log_and_omits_its_messages(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[installed(plugin.path())]);
    let id = PluginId(ID.into());
    log(&path, &id, Level::Warn, "pw-warn-9f3a").unwrap();
    log(&path, &id, Level::Error, "pw-error-9f3a").unwrap();
    log(&path, &id, Level::Info, "pw-info-9f3a").unwrap();
    let text = export(cx);
    assert!(text.contains("log: 1 warn, 1 error"), "{text}");
    for message in ["pw-warn-9f3a", "pw-error-9f3a", "pw-info-9f3a"] {
        assert!(!text.contains(message), "{message} leaked:\n{text}");
    }
}

#[gpui_kit::test]
fn diagnostics_says_a_plugin_without_a_log_has_no_log(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[installed(plugin.path())]);
    let text = export(cx);
    assert!(text.contains("log: no log"), "{text}");
}
