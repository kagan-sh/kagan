use gpui_kit::{AnyWindowHandle, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{KeymapOp, Op, OpResult, PluginOp};
use serde_json::{Value, json};

use super::{aria_label, install_twin};
use crate::tests::kagan::{
    NAVIGATOR, click, install_navigator, open_with_plugins_then, present, shows, value_of,
    wait_until,
};

#[gpui_kit::test]
fn cancel_install_with_nothing_installing_says_so(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open_with_plugins_then(cx, |_| {});
    let cancel = cx.update(|cx| Workspace::dispatch(Op::Plugin(PluginOp::CancelInstall), cx));
    let error = cx.foreground_executor().block_test(cancel).unwrap_err();
    assert_eq!(
        error,
        kg_domain::OpError::Invalid("no git install is running".into())
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_plugin_name_the_disk_cannot_hold_says_so_plainly(cx: &mut TestAppContext) {
    let (_kagan, _window, data) = open_with_plugins_then(cx, |_| {});
    let name = "a".repeat(300);
    let dir = data.path().join(&name);
    let created = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::New {
                name,
                dir: Some(dir),
            }),
            cx,
        )
    });
    let error = cx.foreground_executor().block_test(created).unwrap_err();
    assert_eq!(
        error,
        kg_domain::OpError::Invalid("the plugin name is too long".into())
    );
}

#[gpui_kit::test]
fn an_index_added_in_settings_finds_a_plugin_and_install_asks_for_approval(
    cx: &mut TestAppContext,
) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, |_| {});
    let team = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(team.path().join(".kagan")).unwrap();
    std::fs::write(
        team.path().join(".kagan/index.json"),
        json!({
            "schemaVersion": 1,
            "name": "team",
            "plugins": [{ "name": "echo", "description": "Echoes text", "source": "echo" }]
        })
        .to_string(),
    )
    .unwrap();
    let echo = team.path().join("echo");
    std::fs::create_dir(&echo).unwrap();
    std::fs::write(
        echo.join("kagan.json"),
        json!({
            "id": "dev.test.echo", "name": "Echo", "version": "1.0.0", "kagan": ">=0.1.0",
            "entry": "main.js", "instructions": "Test.",
            "powers": { "clipboard": { "write": true } }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(echo.join("main.js"), "export default class Plugin {}\n").unwrap();
    let location = dunce::canonicalize(team.path())
        .unwrap()
        .display()
        .to_string();

    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-installed");
    assert!(!shows(cx, window, "plugin-search"), "no index, no search");
    click(cx, window, "plugin-index");
    cx.simulate_input(window, &location);
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| shows(cx, window, "setting-row-index-0"));
    assert_eq!(value_of(cx, window, "plugin-index"), Some(String::new()));

    click(cx, window, "plugin-search");
    cx.simulate_input(window, "echo");
    click(cx, window, "plugin-search-run");
    wait_until(cx, |cx| {
        present(cx, window, "plugin-listing-install-team/echo")
    });
    click(cx, window, "plugin-listing-install-team/echo");
    wait_until(cx, |cx| {
        cx.read(|cx| Workspace::global(cx).read(cx).approvals().next().is_some())
    });
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .cloned()
            .unwrap()
    });
    assert!(
        approval
            .title
            .starts_with("Install Echo 1.0.0 (dev.test.echo)?\nFrom the folder ")
            && approval.title.contains(", listed by index team at "),
        "{}",
        approval.title
    );
    let accepted =
        cx.update(|cx| Workspace::answer_standalone(approval.id.clone(), "approve".into(), cx));
    cx.foreground_executor().block_test(accepted).unwrap();
    wait_until(cx, |cx| present(cx, window, "plugin-row-dev.test.echo"));

    click(cx, window, "plugin-index-remove-0");
    wait_until(cx, |cx| !present(cx, window, "setting-row-index-0"));
    assert!(
        !present(cx, window, "plugin-listing-install-team/echo"),
        "a removed index's results go"
    );
}

pub(in crate::tests) fn nav(command: &str) -> String {
    format!("shortcut-plugin-{NAVIGATOR}-{command}")
}

pub(in crate::tests) fn nav_note(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    command: &str,
) -> Option<String> {
    aria_label(
        cx,
        window,
        &format!("setting-row-plugin-{NAVIGATOR}-{command}-note"),
    )
}

/// The keycaps a row shows for a key text, as its label reads them.
pub(in crate::tests) fn keycap(key: &str) -> String {
    key.split_whitespace()
        .map(|key| gpui_kit::component::kbd::Kbd::format(&kg_core::parse_key(key).unwrap()))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(in crate::tests) fn plugin_keys(cx: &mut TestAppContext) -> Value {
    let get = cx.update(|cx| Workspace::dispatch(Op::Keymap(KeymapOp::Get), cx));
    match cx.foreground_executor().block_test(get).unwrap() {
        OpResult::Setting(value) => value["plugins"].clone(),
        other => panic!("{other:?}"),
    }
}

pub(in crate::tests) fn keyboard_with_navigator(
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, tempfile::TempDir) {
    let (_kagan, window, data) = open_with_plugins_then(cx, |data| {
        install_navigator(data);
        install_twin(data);
    });
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-keyboard");
    (window, data)
}
