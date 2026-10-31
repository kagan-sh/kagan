use std::path::Path;

use gpui_kit::{AnyWindowHandle, TestAppContext};
use kg_core::Workspace;
use kg_domain::{ConfigOp, Op, OpResult, PluginId, PluginOp, UiOp};
use serde_json::{Value, json};

use super::kagan::{click, open_with_plugins_then, present, value_of, wait_until};
use super::settings::write_plugin;

const PLUGIN: &str = "dev.test.secret";
const SECRET: &str = "lin_api_ui_5e2b";
const FIELD: &str = "plugin-secret-dev.test.secret/token";
const SAVE: &str = "setting-secret-dev.test.secret/token";
const CLEAR: &str = "setting-clear-dev.test.secret/token";

fn install(data: &Path) {
    write_plugin(
        data,
        "secret-plugin",
        &json!({
            "id": PLUGIN,
            "name": "Linear",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "instructions": "A test plugin with a secret.",
            "powers": {},
            "settings": {
                "token": {"type": "secret", "title": "Linear API key"}
            }
        }),
        "// @ts-check\nexport default class Plugin {}\n",
    );
}

fn token(cx: &mut TestAppContext) -> Value {
    let get = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Config {
                id: PluginId(PLUGIN.into()),
                op: ConfigOp::Get {
                    key: "token".into(),
                },
                thread_id: None,
            }),
            cx,
        )
    });
    match cx.foreground_executor().block_test(get).unwrap() {
        OpResult::Setting(value) => value,
        other => panic!("{other:?}"),
    }
}

fn snapshot(cx: &mut TestAppContext) -> String {
    let task = cx.update(|cx| Workspace::dispatch(Op::Ui(UiOp::Snapshot { window: 0 }), cx));
    cx.run_until_parked();
    let snapshot = cx.foreground_executor().block_test(task).unwrap();
    serde_json::to_string(&snapshot).unwrap()
}

fn kept(data: &Path) -> String {
    std::fs::read_to_string(data.join("secrets").join(format!("{PLUGIN}.json"))).unwrap_or_default()
}

fn type_into_field(cx: &mut TestAppContext, window: AnyWindowHandle, text: &str) {
    click(cx, window, FIELD);
    cx.simulate_input(window, text);
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_secret_field_masks_what_is_typed_and_never_shows_it_back(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert!(present(cx, window, FIELD));
    assert!(!present(cx, window, CLEAR), "nothing to clear yet");

    type_into_field(cx, window, SECRET);
    assert_eq!(value_of(cx, window, FIELD).as_deref(), Some("••••••••"));
    cx.simulate_input(window, "_more");
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, FIELD).as_deref(),
        Some("••••••••"),
        "the length stays hidden"
    );
    assert!(!snapshot(cx).contains(SECRET));

    click(cx, window, SAVE);
    wait_until(cx, |cx| token(cx) == json!("set"));
    assert!(kept(data.path()).contains(&format!("{SECRET}_more")));
    assert_eq!(
        value_of(cx, window, FIELD).as_deref(),
        Some(""),
        "the field empties"
    );
    assert!(present(cx, window, CLEAR));
    assert!(!snapshot(cx).contains(SECRET));

    type_into_field(cx, window, "lin_api_replaced");
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |_| kept(data.path()).contains("lin_api_replaced"));
    assert_eq!(value_of(cx, window, FIELD).as_deref(), Some(""));

    click(cx, window, CLEAR);
    wait_until(cx, |cx| token(cx) == Value::Null);
    assert_eq!(kept(data.path()), "");
    assert!(!present(cx, window, CLEAR));
}
