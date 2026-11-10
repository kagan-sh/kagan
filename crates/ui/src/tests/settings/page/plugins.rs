use std::path::Path;

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{ConfigOp, Op, OpResult, PluginId, PluginOp};
use serde_json::json;

use super::{
    aria_label, install_plugins, plugin_config, plugin_config_of, plugin_enabled, run_op,
    set_plugin_config, settings_text, write_plugin,
};
use crate::tests::kagan::{
    click, focused_of, open_with_plugins_then, present, shows, value_of, wait_until,
};

const PREFS: &str = "dev.test.prefs";
const VIEW: &str = "dev.test.view";
const OTHER: &str = "dev.test.other";
const OLD: &str = "dev.test.old";

#[gpui_kit::test]
fn a_text_setting_shows_a_change_from_kg_without_saving(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert_eq!(settings_text(cx, "label"), "hi");

    set_plugin_config(cx, "label", json!("from kg"));
    cx.run_until_parked();
    assert_eq!(settings_text(cx, "label"), "from kg");
    assert_eq!(
        plugin_config(cx, "label"),
        json!("from kg"),
        "the change was not saved again"
    );
}

#[gpui_kit::test]
fn a_folder_approval_shows_in_settings(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");

    let folder = dunce::canonicalize(data.path())
        .unwrap()
        .display()
        .to_string();
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Config {
                id: PluginId(PREFS.into()),
                op: ConfigOp::Set {
                    key: "folder".into(),
                    value: Some(json!(folder.clone())),
                },
                thread_id: None,
            }),
            cx,
        )
    });
    set.detach();
    cx.run_until_parked();

    let id = cx
        .update(|cx| {
            Workspace::global(cx)
                .read(cx)
                .approvals()
                .next()
                .map(|approval| approval.id.clone())
        })
        .expect("the folder waits for approval");
    assert!(
        shows(cx, window, &format!("question-{}", id.0)),
        "the approval is in Settings"
    );
    assert!(
        !shows(cx, window, "needs-you"),
        "the right sidebar stays hidden"
    );

    let answer = cx.update(|cx| Workspace::answer_standalone(id, "approve".into(), cx));
    cx.foreground_executor().block_test(answer).unwrap();
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "folder"), json!(folder));
}

#[gpui_kit::test]
fn a_plugin_with_only_a_settings_view_gets_a_page(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert!(
        cx.update(|cx| kagan.read(cx).settings_view_mounted(&PluginId(VIEW.into()))),
        "its own view is mounted"
    );
}

#[gpui_kit::test]
fn the_installed_switch_disables_and_enables_a_plugin(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-installed");
    assert!(plugin_enabled(cx));

    click(cx, window, &format!("plugin-{PREFS}"));
    assert!(!plugin_enabled(cx), "the switch turns the plugin off");

    click(cx, window, &format!("plugin-{PREFS}"));
    assert!(plugin_enabled(cx), "and on again");
}

#[gpui_kit::test]
fn a_plugin_that_did_not_start_offers_update_and_remove(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_old_plugin);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-installed");
    click(cx, window, &format!("plugin-details-{OLD}"));
    assert!(
        !present(cx, window, &format!("plugin-update-{OLD}")),
        "a folder source has no Update"
    );
    assert!(present(cx, window, &format!("plugin-reload-{OLD}")));
    assert!(present(cx, window, &format!("plugin-remove-{OLD}")));

    // The author fixes the manifest and updates. A start that works clears the old reason.
    let fixed = json!({
        "id": OLD,
        "name": "Old",
        "version": "1.1.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "A command only.",
        "powers": {},
        "commands": [{"id": "hi", "title": "Say hi"}]
    });
    std::fs::write(
        data.path().join("old-plugin").join("kagan.json"),
        fixed.to_string(),
    )
    .unwrap();
    click(cx, window, &format!("plugin-reload-{OLD}"));
    cx.run_until_parked();
    assert_eq!(listed_error(cx, OLD), None, "kg plugin list shows no error");
    assert_eq!(status_warnings(cx, OLD), None, "kg status shows no warning");
}

/// The start failure `kg plugin list` shows for a plugin.
pub(in crate::tests) fn listed_error(cx: &mut TestAppContext, id: &str) -> Option<String> {
    let list = cx.update(|cx| Workspace::dispatch(Op::Plugin(PluginOp::List), cx));
    match cx.foreground_executor().block_test(list).unwrap() {
        OpResult::Plugins(plugins) => plugins
            .into_iter()
            .find(|plugin| plugin.id.0 == id)
            .and_then(|plugin| plugin.error),
        other => panic!("{other:?}"),
    }
}

/// The warning `kg status` shows for a plugin.
pub(in crate::tests) fn status_warnings(cx: &mut TestAppContext, id: &str) -> Option<String> {
    let status = cx.update(|cx| {
        Workspace::dispatch(
            Op::Status {
                thread_id: None,
                project_id: None,
                environment_id: None,
            },
            cx,
        )
    });
    match cx.foreground_executor().block_test(status).unwrap() {
        OpResult::Status { warnings, .. } => warnings
            .into_iter()
            .find(|warning| warning.starts_with(&format!("plugin {id}:"))),
        other => panic!("{other:?}"),
    }
}

/// A plugin installed before its toolbar was refused: it cannot start.
pub(in crate::tests) fn install_old_plugin(data: &Path) {
    write_plugin(
        data,
        "old-plugin",
        &json!({
            "id": OLD,
            "name": "Old",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "instructions": "A plugin with a toolbar.",
            "powers": {},
            "ui": {"toolbar": [{"id": "old-open", "view": "OldButton"}]}
        }),
        "// @ts-check\nexport default class Plugin {}\n",
    );
}

#[gpui_kit::test]
fn two_plugins_with_the_same_key_stay_apart(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.other");
    click(cx, window, "setting-row-dev.test.other/sounds");

    click(cx, window, "plugin-dev.test.other/sounds");
    assert_eq!(
        plugin_config_of(cx, OTHER, "sounds"),
        json!(true),
        "the other plugin changed"
    );
    assert_eq!(
        plugin_config_of(cx, PREFS, "sounds"),
        json!(true),
        "the first plugin did not"
    );
}

#[gpui_kit::test]
fn disabling_a_plugin_with_its_page_open_moves_the_page_and_drops_its_view(
    cx: &mut TestAppContext,
) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert!(cx.update(|cx| kagan.read(cx).settings_view_mounted(&PluginId(VIEW.into()))));

    run_op(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(VIEW.into()),
        }),
    );
    assert!(!cx.update(|cx| kagan.read(cx).settings_view_mounted(&PluginId(VIEW.into()))));
    assert!(shows(cx, window, "settings-page"));
}

#[gpui_kit::test]
fn reloading_a_plugin_with_its_page_open_mounts_its_view_again(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let before = cx.update(|cx| {
        kagan
            .read(cx)
            .settings_view_generation(&PluginId(VIEW.into()))
    });
    assert!(before.is_some());

    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(VIEW.into()),
            thread_id: None,
        }),
    );
    let host = cx.update(|cx| {
        kg_plugins::PluginHost::global(cx)
            .read(cx)
            .generation(&PluginId(VIEW.into()))
    });
    let after = cx.update(|cx| {
        kagan
            .read(cx)
            .settings_view_generation(&PluginId(VIEW.into()))
    });
    assert_ne!(before, after, "the view was made again");
    assert_eq!(after, host, "and it matches the plugin's new load");
    assert!(cx.update(|cx| kagan.read(cx).settings_view_mounted(&PluginId(VIEW.into()))));
}

#[gpui_kit::test]
fn an_entry_and_its_control_expose_a_label(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    assert_eq!(
        aria_label(cx, window, "theme-ember").as_deref(),
        Some("Ember theme")
    );
}

#[gpui_kit::test]
fn a_plugin_text_field_states_its_value_and_saves_on_enter(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.prefs");
    let field = format!("plugin-text-{PREFS}/label");
    assert_eq!(value_of(cx, window, &field), Some("hi".into()));

    click(cx, window, &field);
    assert_eq!(focused_of(cx, window, &field), Some(true));
    cx.simulate_input(window, "!");
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, &field),
        Some("hi!".into()),
        "the field shows what is typed"
    );
    assert_eq!(
        plugin_config(cx, "label"),
        json!("hi"),
        "typing alone saves nothing"
    );

    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "label"), json!("hi!"));
}

#[gpui_kit::test]
fn a_plugin_number_field_saves_a_finite_number_on_enter(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.prefs");
    let field = format!("plugin-text-{PREFS}/retries");
    assert_eq!(value_of(cx, window, &field), Some("3".into()));
    assert_eq!(plugin_config(cx, "retries"), json!(3));

    click(cx, window, &field);
    cx.simulate_keystrokes(window, "backspace");
    cx.simulate_input(window, "2.5");
    cx.run_until_parked();
    assert_eq!(
        plugin_config(cx, "retries"),
        json!(3),
        "typing alone saves nothing"
    );

    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "retries"), json!(2.5));
    assert_eq!(value_of(cx, window, &field), Some("2.5".into()));

    click(cx, window, &field);
    for _ in 0..3 {
        cx.simulate_keystrokes(window, "backspace");
    }
    cx.simulate_input(window, "  2.50 ");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "retries"), json!(2.5));
    assert_eq!(
        value_of(cx, window, &field),
        Some("2.5".into()),
        "Enter keeps the canonical number in the field"
    );
    assert!(!present(
        cx,
        window,
        "setting-row-dev.test.prefs/retries-note"
    ));

    click(cx, window, &field);
    for _ in 0..3 {
        cx.simulate_keystrokes(window, "backspace");
    }
    cx.simulate_input(window, "nope");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(
        plugin_config(cx, "retries"),
        json!(2.5),
        "a non-number is refused"
    );
    assert_eq!(value_of(cx, window, &field), Some("nope".into()));
    assert_eq!(
        aria_label(cx, window, "setting-row-dev.test.prefs/retries-note").as_deref(),
        Some("Type a number.")
    );
}

#[gpui_kit::test]
fn a_plugin_turns_off_and_on_from_the_installed_page(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-installed");
    let switch = format!("plugin-{PREFS}");
    wait_until(cx, |cx| present(cx, window, &switch));
    assert!(plugin_enabled(cx));

    click(cx, window, &switch);
    wait_until(cx, |cx| !plugin_enabled(cx));
    click(cx, window, &switch);
    wait_until(cx, plugin_enabled);

    click(cx, window, &format!("plugin-details-{PREFS}"));
    assert!(
        !present(cx, window, &format!("plugin-update-{PREFS}")),
        "a folder source has no Update"
    );
    assert!(present(cx, window, &format!("plugin-reload-{PREFS}")));
    assert!(present(cx, window, &format!("plugin-logs-{PREFS}")));
    assert!(present(cx, window, &format!("plugin-remove-{PREFS}")));
}
