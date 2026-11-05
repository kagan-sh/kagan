use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext};
use kg_core::Workspace;
use kg_domain::{Appearance, ConfigOp, Op, OpResult, Plugin, PluginId, PluginOp, ThemeOp};
use serde_json::{Value, json};

const PREFS: &str = "dev.test.prefs";
const VIEW: &str = "dev.test.view";
const OTHER: &str = "dev.test.other";
const OLD: &str = "dev.test.old";

pub(in crate::tests) fn aria_label(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Option<String> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.label.as_deref().map(str::to_owned))
    })
    .unwrap()
}

pub(in crate::tests) fn run_op(cx: &mut TestAppContext, op: Op) {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
}

pub(in crate::tests) fn plugin_config_of(
    cx: &mut TestAppContext,
    plugin: &str,
    key: &str,
) -> Value {
    let get = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Config {
                id: PluginId(plugin.into()),
                op: ConfigOp::Get { key: key.into() },
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

/// Opens the workspace without a window, so a preference can be set first.
pub(in crate::tests) fn setup_only(cx: &mut TestAppContext) -> tempfile::TempDir {
    crate::tests::kagan::setup(cx)
}

pub(in crate::tests) fn pane_width(cx: &mut TestAppContext, window: AnyWindowHandle) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name("sidebar-pane".into()))
            .map_or(0., |element| f32::from(element.bounds.size.width))
    })
    .unwrap()
}

pub(in crate::tests) fn saved(cx: &mut TestAppContext, key: &str) -> Value {
    let get = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(kg_domain::PreferenceOp::Get { key: key.into() }),
            cx,
        )
    });
    match cx.foreground_executor().block_test(get).unwrap() {
        OpResult::Setting(value) => value,
        other => panic!("{other:?}"),
    }
}

pub(in crate::tests) fn focused(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> bool {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .is_some_and(|element| element.focused == Some(true))
    })
    .unwrap()
}

/// Presses Tab until the named control has the focus.
pub(in crate::tests) fn tab_to(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) {
    for _ in 0..40 {
        if focused(cx, window, name) {
            return;
        }
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
    }
    panic!("{name} never took the focus");
}

pub(in crate::tests) fn settings_text(cx: &mut TestAppContext, key: &str) -> String {
    let plugin = PluginId(PREFS.into());
    cx.update(|cx| crate::kagan::Kagan::settings_text(cx, &plugin, key))
}

pub(in crate::tests) fn set_plugin_config(cx: &mut TestAppContext, key: &str, value: Value) {
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Config {
                id: PluginId(PREFS.into()),
                op: ConfigOp::Set {
                    key: key.into(),
                    value: Some(value),
                },
                thread_id: None,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();
}

pub(in crate::tests) fn plugin_enabled(cx: &mut TestAppContext) -> bool {
    let list = cx.update(|cx| Workspace::dispatch(Op::Plugin(PluginOp::List), cx));
    match cx.foreground_executor().block_test(list).unwrap() {
        OpResult::Plugins(plugins) => plugins
            .iter()
            .find(|plugin| plugin.id.0 == PREFS)
            .is_some_and(|plugin| plugin.enabled),
        other => panic!("{other:?}"),
    }
}

pub(in crate::tests) fn set_mode(cx: &mut TestAppContext, mode: Appearance) {
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Theme(ThemeOp::Set {
                theme: "fjord".into(),
                mode,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();
}

pub(in crate::tests) fn get_theme(cx: &mut TestAppContext) -> kg_domain::ThemeSettings {
    let get = cx.update(|cx| Workspace::dispatch(Op::Theme(ThemeOp::Get), cx));
    match cx.foreground_executor().block_test(get).unwrap() {
        OpResult::Theme(settings) => settings,
        other => panic!("{other:?}"),
    }
}

/// Two plugins: one declares settings (a switch, a select, a text, a folder), and one contributes
/// only its own settings view.
pub(in crate::tests) fn install_plugins(data: &Path) {
    write_plugin(
        data,
        "prefs-plugin",
        &json!({
            "id": PREFS,
            "name": "Prefs",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "instructions": "A test plugin with settings.",
            "powers": {},
            "settings": {
                "sounds": {"type": "boolean", "title": "Sounds", "default": true},
                "density": {
                    "type": "select",
                    "title": "Density",
                    "options": ["comfortable", "compact"],
                    "default": "comfortable"
                },
                "label": {"type": "string", "title": "Label", "default": "hi"},
                "board": {"type": "project", "title": "Board"},
                "retries": {"type": "number", "title": "Retries", "default": 3},
                "folder": {"type": "folder", "title": "Folder"}
            }
        }),
        "// @ts-check\nexport default class Plugin {}\n",
    );
    write_plugin(
        data,
        "other-plugin",
        &json!({
            "id": OTHER,
            "name": "Other",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "instructions": "Another test plugin with a setting of the same key.",
            "powers": {},
            "settings": {
                "sounds": {"type": "boolean", "title": "Sounds", "default": false}
            }
        }),
        "// @ts-check\nexport default class Plugin {}\n",
    );
    write_plugin(
        data,
        "view-plugin",
        &json!({
            "id": VIEW,
            "name": "View",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "instructions": "A test plugin with its own settings view.",
            "powers": {},
            "ui": {"settings": "ViewSettings"}
        }),
        "// @ts-check\nimport { div, View } from \"gpui-kit\";\n\
         export default class Plugin extends View {}\n\
         export class ViewSettings extends View {\n  render() {\n    return div().child(\"A custom view\");\n  }\n}\n",
    );
}

pub(in crate::tests) fn write_plugin(data: &Path, folder: &str, manifest: &Value, entry: &str) {
    // A folder setting is a power the person approves at install: the grant names it.
    let folders: Value = manifest["settings"]
        .as_object()
        .map_or(json!({}), |settings| {
            settings
                .iter()
                .filter(|(_, setting)| setting["type"] == "folder")
                .map(|(key, setting)| (key.clone(), setting["title"].clone()))
                .collect()
        });
    let root = data.join(folder);
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("main.js"), entry).unwrap();
    let id = manifest["id"].as_str().unwrap().to_owned();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(id),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: json!({"powers": {}, "downloads": {}, "folders": folders}),
            error: None,
        })
        .unwrap();
    drop(store);
}

pub(in crate::tests) fn plugin_config(cx: &mut TestAppContext, key: &str) -> Value {
    let get = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Config {
                id: PluginId(PREFS.into()),
                op: ConfigOp::Get { key: key.into() },
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
