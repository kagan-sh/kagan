use gpui_kit::{AppContext as _, ElementId, TestAppContext};
use kg_core::Workspace;
use kg_domain::{ConfigOp, Op, PluginId, PluginOp};
use serde_json::json;

use super::{
    approval_count, approve_waiting, aria_label, canonical, choose_folder, host_setting,
    install_plugins, plugin_config, press, until_saved, waiting,
};
use crate::tests::kagan::{click, open_with_plugins_then, present, selected_of, value_of};

const PREFS: &str = "dev.test.prefs";

#[gpui_kit::test]
fn cancelling_a_folder_picker_and_a_file_path_do_not_change_the_setting(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click(
            window,
            &ElementId::Name(format!("choose-{PREFS}/folder").into()),
            cx,
        )
        .unwrap();
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "folder"), serde_json::Value::Null);
    assert!(!present(
        cx,
        window,
        "setting-row-dev.test.prefs/folder-note"
    ));

    let file = data.path().join("not-a-folder");
    std::fs::write(&file, "x").unwrap();
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click(
            window,
            &ElementId::Name(format!("choose-{PREFS}/folder").into()),
            cx,
        )
        .unwrap();
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(move |_| Some(vec![file.clone()]));
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "folder"), serde_json::Value::Null);
    let note = aria_label(cx, window, "setting-row-dev.test.prefs/folder-note").unwrap_or_default();
    assert!(note.contains("existing folder"), "{note}");
}

#[gpui_kit::test]
fn an_older_text_save_cannot_overwrite_a_newer_one(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let field = format!("plugin-text-{PREFS}/label");
    click(cx, window, &field);
    cx.simulate_input(window, "!");
    cx.run_until_parked();
    cx.dispatch_keystroke(window, kg_core::parse_key("enter").unwrap());
    cx.dispatch_keystroke(window, kg_core::parse_key("x").unwrap());
    cx.dispatch_keystroke(window, kg_core::parse_key("enter").unwrap());
    until_saved(cx, "label", &json!("hi!"));
    assert!(
        cx.background_executor.tick(),
        "the first save still has work left"
    );
    assert_eq!(
        host_setting(cx, "label"),
        json!("hi!"),
        "the newer text stays queued until the first save finishes"
    );
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "label"), json!("hi!x"));
    assert_eq!(value_of(cx, window, &field), Some("hi!x".into()));
    assert!(!present(
        cx,
        window,
        "setting-row-dev.test.prefs/label-note"
    ));
}

#[gpui_kit::test]
fn an_older_select_cannot_overwrite_a_newer_one(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    press(cx, window, "option-dev.test.prefs/density/compact");
    press(cx, window, "option-dev.test.prefs/density/comfortable");
    until_saved(cx, "density", &json!("compact"));
    assert!(
        cx.background_executor.tick(),
        "the first save still has work left"
    );
    assert_eq!(
        host_setting(cx, "density"),
        json!("compact"),
        "the second choice waits until the first save finishes"
    );
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "density"), json!("comfortable"));
    assert_eq!(
        selected_of(cx, window, "option-dev.test.prefs/density/comfortable"),
        Some(true)
    );
}

#[gpui_kit::test]
fn an_older_folder_save_cannot_overwrite_a_newer_one(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let older = data.path().join("older");
    let newer = data.path().join("newer");
    std::fs::create_dir(&older).unwrap();
    std::fs::create_dir(&newer).unwrap();
    choose_folder(cx, window, &older);
    cx.run_until_parked();
    assert_eq!(approval_count(cx), 1);
    assert_eq!(host_setting(cx, "folder"), serde_json::Value::Null);
    choose_folder(cx, window, &newer);
    cx.run_until_parked();
    assert_eq!(
        approval_count(cx),
        1,
        "the newer folder waits behind the approval already open"
    );
    approve_waiting(cx);
    let stored = plugin_config(cx, "folder");
    assert_eq!(stored, json!(canonical(&older)), "{stored}");
    assert_eq!(approval_count(cx), 1);
    approve_waiting(cx);
    let stored = plugin_config(cx, "folder");
    assert_eq!(stored, json!(canonical(&newer)), "{stored}");
    assert!(waiting(cx).is_none());
}

#[gpui_kit::test]
fn an_external_save_clears_a_stale_setting_error(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let file = data.path().join("not-a-folder");
    std::fs::write(&file, "x").unwrap();
    choose_folder(cx, window, &file);
    cx.run_until_parked();
    let note = aria_label(cx, window, "setting-row-dev.test.prefs/folder-note").unwrap_or_default();
    assert!(note.contains("existing folder"), "{note}");

    let folder = data.path().join("chosen");
    std::fs::create_dir(&folder).unwrap();
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Config {
                id: PluginId(PREFS.into()),
                op: ConfigOp::Set {
                    key: "folder".into(),
                    value: Some(json!(folder)),
                },
                thread_id: None,
            }),
            cx,
        )
    });
    cx.run_until_parked();
    approve_waiting(cx);
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();
    assert!(!present(
        cx,
        window,
        "setting-row-dev.test.prefs/folder-note"
    ));
    assert_eq!(
        plugin_config(cx, "folder"),
        json!(canonical(&data.path().join("chosen")))
    );
}
