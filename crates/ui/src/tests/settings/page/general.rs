use std::path::Path;

use gpui_kit::{AppContext as _, ElementId, TestAppContext};
use kg_core::Workspace;
use kg_domain::{Color, Op, OpResult, PluginId, PluginOp};
use serde_json::json;

use super::{TIDE, aria_label, install_plugins, plugin_config, run_op, tab_to, write_plugin};
use crate::tests::kagan::{
    click, focused_of, open, open_with_plugins_then, present, run_shell, shows, value_of,
};

const PREFS: &str = "dev.test.prefs";

pub(in crate::tests) fn painted_bg(cx: &mut TestAppContext) -> Color {
    cx.update(|cx| crate::theme::ThemeState::global(cx).read(cx).palette.bg)
}

pub(in crate::tests) fn try_op(
    cx: &mut TestAppContext,
    op: Op,
) -> Result<OpResult, kg_domain::OpError> {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    let result = cx.foreground_executor().block_test(task);
    cx.run_until_parked();
    result
}

pub(in crate::tests) const THEMES: &str = "dev.test.themes";

pub(in crate::tests) fn install_theme_plugin(data: &Path) {
    write_plugin(
        data,
        "themes-plugin",
        &json!({
            "id": THEMES,
            "name": "Themes",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "powers": {},
            "themes": [{"id": "midnight", "file": "themes/midnight.json"}]
        }),
        "// @ts-check\nexport default class Plugin {}\n",
    );
    let themes = data.join("themes-plugin/themes");
    std::fs::create_dir(&themes).unwrap();
    let midnight = TIDE
        .replace("Tide", "Midnight")
        .replace("#f2f2f2", "#101010");
    std::fs::write(themes.join("midnight.json"), midnight).unwrap();
}

#[gpui_kit::test]
fn general_describes_what_integration_still_sets(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "setting-row-agent-integration-note");
    let note = aria_label(cx, window, "setting-row-agent-integration-note").unwrap_or_default();
    assert!(
        note.contains("claude") && note.contains("codex") && note.contains("cursor-agent"),
        "{note}"
    );
    assert!(note.contains("--new-session-id or --resume"), "{note}");
    assert!(
        note.contains("--mcp-config")
            && note.contains("--session-id or --resume")
            && note.contains("-c mcp_servers.kagan.*"),
        "{note}"
    );
    assert!(note.contains("Off drops kagan’s tools"), "{note}");
    assert!(
        note.contains("--disallowed-tools")
            && note.contains("--disable multi_agent")
            && note.contains("--append-system-prompt"),
        "{note}"
    );
    assert!(
        note.contains("KG_ variables") && note.contains("PATH"),
        "{note}"
    );
    assert!(!note.contains("unchanged"), "{note}");
}

#[gpui_kit::test]
fn close_leaves_settings_and_enter_on_the_page_does_not(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    click(cx, window, "settings-gear");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-page"));
    tab_to(cx, window, "settings-close");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert!(!shows(cx, window, "settings-page"));
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(id));
}

#[gpui_kit::test]
fn a_text_draft_waits_for_enter_and_revert_puts_the_saved_value_back(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let field = format!("plugin-text-{PREFS}/label");
    click(cx, window, &field);
    cx.simulate_input(window, "!");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "label"), json!("hi"));
    assert_eq!(
        aria_label(cx, window, "setting-row-dev.test.prefs/label-note").as_deref(),
        Some("Enter saves.")
    );
    assert!(shows(cx, window, "setting-revert-dev.test.prefs/label"));

    cx.dispatch_keystroke(window, kg_core::parse_key("enter").unwrap());
    assert_eq!(
        aria_label(cx, window, "setting-row-dev.test.prefs/label-note").as_deref(),
        Some("Saving…")
    );
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "label"), json!("hi!"));
    assert!(!present(cx, window, "setting-revert-dev.test.prefs/label"));

    click(cx, window, &field);
    cx.simulate_input(window, "?");
    cx.run_until_parked();
    click(cx, window, "setting-revert-dev.test.prefs/label");
    assert_eq!(value_of(cx, window, &field), Some("hi!".into()));
    assert_eq!(plugin_config(cx, "label"), json!("hi!"));
}

#[gpui_kit::test]
fn a_newer_draft_survives_the_reply(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let field = format!("plugin-text-{PREFS}/label");
    click(cx, window, &field);
    cx.simulate_input(window, "!");
    cx.run_until_parked();
    cx.dispatch_keystroke(window, kg_core::parse_key("enter").unwrap());
    cx.dispatch_keystroke(window, kg_core::parse_key("x").unwrap());
    cx.simulate_keystrokes(window, "tab");
    cx.run_until_parked();
    assert_eq!(value_of(cx, window, &field), Some("hi!x".into()));
    assert_eq!(plugin_config(cx, "label"), json!("hi!"));
    assert_eq!(focused_of(cx, window, &field), Some(false));
}

#[gpui_kit::test]
fn a_failed_text_save_keeps_the_draft_until_revert(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    let field = format!("plugin-text-{PREFS}/label");
    click(cx, window, &field);
    cx.simulate_input(window, "!");
    cx.run_until_parked();
    cx.dispatch_keystroke(window, kg_core::parse_key("enter").unwrap());
    cx.update(|cx| {
        kg_plugins::PluginHost::global(cx).update(cx, |host, cx| {
            host.disable_loaded(&PluginId(PREFS.into()), cx);
        });
    });
    cx.run_until_parked();
    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(PREFS.into()),
            thread_id: None,
        }),
    );
    assert_eq!(value_of(cx, window, &field), Some("hi!".into()));
    assert_eq!(plugin_config(cx, "label"), json!("hi"));
    assert_eq!(
        aria_label(cx, window, "setting-row-dev.test.prefs/label-note").as_deref(),
        Some("Enter saves.")
    );
    click(cx, window, "setting-revert-dev.test.prefs/label");
    assert_eq!(value_of(cx, window, &field), Some("hi".into()));
}

#[gpui_kit::test]
fn a_failed_switch_keeps_the_saved_value_and_shows_the_error(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click(
            window,
            &ElementId::Name(format!("plugin-{PREFS}/sounds").into()),
            cx,
        )
        .unwrap();
        kg_plugins::PluginHost::global(cx).update(cx, |host, cx| {
            host.disable_loaded(&PluginId(PREFS.into()), cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(PREFS.into()),
            thread_id: None,
        }),
    );
    assert_eq!(
        value_of(cx, window, &format!("plugin-{PREFS}/sounds")).as_deref(),
        Some("on")
    );
    assert!(
        !present(cx, window, "setting-row-dev.test.prefs/sounds-note"),
        "a disabled plugin's failed toggle does not stay after it returns"
    );
    click(cx, window, &format!("plugin-{PREFS}/sounds"));
    assert_eq!(plugin_config(cx, "sounds"), json!(false));
    assert!(!present(
        cx,
        window,
        "setting-row-dev.test.prefs/sounds-note"
    ));
}
