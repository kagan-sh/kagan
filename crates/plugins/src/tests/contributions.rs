use gpui_kit::TestAppContext;
use kg_domain::PluginOp;
use serde_json::{Value, json};

use super::{
    check_plugin, open, plugin_dir, plugin_op, read_manifest, sample_manifest, spare_data,
    write_plugin,
};

#[test]
fn a_plain_button_in_any_plugin_script_is_a_warning_and_a_kg_button_is_not() {
    let dir = plugin_dir();
    std::fs::create_dir_all(dir.path().join("ui")).unwrap();
    std::fs::write(
        dir.path().join("ui/panel.js"),
        "export const n = new Button ( \"go\" ).label(\"Go\");\n",
    )
    .unwrap();
    assert_eq!(
        check_plugin(dir.path()).unwrap(),
        ["ui/panel.js uses Button, which kg ui cannot find or click; use KgButton"]
    );

    // A comment or a string that names Button constructs nothing.
    std::fs::write(
        dir.path().join("ui/panel.js"),
        "// new Button(\"old\")\nexport const n = \"new Button(x)\";\nexport const k = new KgButton(\"go\");\n",
    )
    .unwrap();
    assert_eq!(check_plugin(dir.path()).unwrap(), Vec::<String>::new());
}

#[test]
fn check_parses_exports_without_running_plugin_code() {
    let plugin = plugin_dir();
    let mut changed = sample_manifest();
    changed["ui"] = json!({
        "panels": [{"id": "panel", "title": "Panel", "view": "Panel", "placement": "left"}]
    });
    std::fs::write(
        plugin.path().join(crate::manifest::FILE),
        changed.to_string(),
    )
    .unwrap();
    std::fs::write(
        plugin.path().join("main.js"),
        "throw new Error('must not run'); export default class Plugin {} export class Panel {} ",
    )
    .unwrap();
    check_plugin(plugin.path()).unwrap();
    std::fs::write(plugin.path().join("main.js"), "export default class Plugin {} // export class Panel {}\nconst text = 'export class Panel {}';").unwrap();
    assert!(
        check_plugin(plugin.path())
            .unwrap_err()
            .to_string()
            .contains("Panel")
    );
    std::fs::write(plugin.path().join("main.js"), "export default class {").unwrap();
    assert!(check_plugin(plugin.path()).is_err());
}

#[test]
fn string_views_require_exports() {
    let plugin = plugin_dir();
    let mut changed = sample_manifest();
    changed["ui"] = json!({"settings": "Missing"});
    std::fs::write(
        plugin.path().join(crate::manifest::FILE),
        changed.to_string(),
    )
    .unwrap();
    std::fs::write(
        plugin.path().join("main.js"),
        "export default class Plugin {}",
    )
    .unwrap();
    assert!(
        check_plugin(plugin.path())
            .unwrap_err()
            .to_string()
            .contains("Missing")
    );
    std::fs::write(
        plugin.path().join("main.js"),
        "export default class Plugin {} export class Missing {}",
    )
    .unwrap();
    check_plugin(plugin.path()).unwrap();
}

#[test]
fn a_replacement_panel_names_one_slot_in_its_own_place() {
    let plugin = plugin_dir();
    let panels = |panels: Value| {
        let mut declared = sample_manifest();
        declared["ui"]["panels"] = panels;
        write_plugin(plugin.path(), &declared);
        read_manifest(plugin.path()).map(|_| ())
    };
    assert!(
        panels(json!([
            { "id": "list", "title": "List", "view": "List", "placement": "left", "replaces": "threads" },
            { "id": "diff", "title": "Diff", "view": "Diff", "placement": "center", "replaces": "review" }
        ]))
        .is_ok()
    );
    for (declared, message) in [
        (
            json!([{ "id": "list", "title": "List", "view": "List", "placement": "center", "replaces": "threads" }]),
            "panel list replaces threads, so it must be placed left",
        ),
        (
            json!([{ "id": "diff", "title": "Diff", "view": "Diff", "placement": "left", "replaces": "review" }]),
            "panel diff replaces review, so it must be placed center",
        ),
        (
            json!([
                { "id": "one", "title": "One", "view": "One", "placement": "left", "replaces": "threads" },
                { "id": "two", "title": "Two", "view": "Two", "placement": "left", "replaces": "threads" }
            ]),
            "only one panel may replace threads",
        ),
        (
            json!([{ "id": "list", "title": "List", "view": "List", "placement": "left", "replaces": "terminal" }]),
            "unknown variant",
        ),
    ] {
        let error = panels(declared).unwrap_err().to_string();
        assert!(error.contains(message), "{error}");
    }
    let mut declared = sample_manifest();
    declared["ui"]["panels"] = json!([
        { "id": "diff", "title": "Diff", "view": "Diff", "placement": "center", "replaces": "review" }
    ]);
    declared["powers"]["run"] = json!([{ "program": "git" }]);
    write_plugin(plugin.path(), &declared);
    assert!(read_manifest(plugin.path()).is_ok());
}

/// Install and check both read the manifest first, so a manifest for a spot kagan does not draw is
/// refused before any code runs.

#[test]
fn a_manifest_for_a_spot_kagan_does_not_draw_is_refused() {
    let plugin = plugin_dir();
    let refused = |declared: Value, message: &str| {
        write_plugin(plugin.path(), &declared);
        let error = read_manifest(plugin.path()).unwrap_err().to_string();
        assert!(error.contains(message), "{error}");
    };
    for spot in ["toolbar", "status-bar", "sidebar-footer"] {
        let mut declared = sample_manifest();
        declared["ui"][spot] = json!([{ "id": "item", "view": "Item" }]);
        refused(declared, &format!("ui.{spot} is not in this version"));
    }
    let mut declared = sample_manifest();
    declared["ui"]["thread-list"] = json!("Board");
    refused(
        declared,
        "declare a left panel with \"replaces\": \"threads\"",
    );
    for placement in ["left", "right", "center"] {
        let mut declared = sample_manifest();
        declared["ui"]["panels"] =
            json!([{ "id": "notes", "title": "Notes", "view": "Notes", "placement": placement }]);
        write_plugin(plugin.path(), &declared);
        assert!(read_manifest(plugin.path()).is_ok(), "{placement}");
    }
    let mut declared = sample_manifest();
    declared["ui"]["panels"] =
        json!([{ "id": "notes", "title": "Notes", "view": "Notes", "placement": "right" }]);
    write_plugin(plugin.path(), &declared);
    let error = crate::development::check(plugin.path(), spare_data())
        .unwrap_err()
        .to_string();
    assert_eq!(error, "declared view Notes is not exported by main.js");
    let mut declared = sample_manifest();
    declared["ui"]["panels"] =
        json!([{ "id": "notes", "title": "Notes", "view": "Notes", "placement": "bottom" }]);
    refused(declared, "bottom panels are not in this version");
    let mut declared = sample_manifest();
    declared["ui"]["panels"] = json!([{
        "id": "notes",
        "title": "Notes",
        "view": "Notes",
        "placement": "right",
        "store": true
    }]);
    refused(declared, "store belongs on WebView.store(true)");
}

#[test]
fn a_header_command_needs_when_thread() {
    let plugin = plugin_dir();
    let mut declared = sample_manifest();
    declared["commands"] = json!([{ "id": "hand", "title": "Hand off", "header": true }]);
    write_plugin(plugin.path(), &declared);
    let error = read_manifest(plugin.path()).unwrap_err().to_string();
    assert!(
        error.contains("a header command needs when thread"),
        "{error}"
    );
    declared["commands"] =
        json!([{ "id": "hand", "title": "Hand off", "when": "thread", "header": true }]);
    write_plugin(plugin.path(), &declared);
    assert!(read_manifest(plugin.path()).is_ok());
}

/// A plugin's theme is a file in its folder, checked at install and at check with the schema of
/// a person's theme file.

#[test]
fn an_overlay_names_one_exported_view() {
    let plugin = plugin_dir();
    let mut declared = sample_manifest();
    declared["ui"]["overlay"] = json!("Plate");
    write_plugin(plugin.path(), &declared);
    let read = read_manifest(plugin.path()).unwrap();
    assert_eq!(read.ui.overlay.as_deref(), Some("Plate"));
    for overlay in [json!(""), json!(["Plate"]), json!({ "view": "Plate" })] {
        declared["ui"]["overlay"] = overlay;
        write_plugin(plugin.path(), &declared);
        assert!(read_manifest(plugin.path()).is_err());
    }
}

#[test]
fn ui_manifest_rejects_unknown_spots_and_duplicate_panels() {
    let plugin = plugin_dir();
    for ui in [
        json!({"unknown": "View"}),
        json!({"panels": [{"id":"same","title":"A","view":"A","placement":"left"}, {"id":"same","title":"B","view":"B","placement":"right"}]}),
        json!({"overlay":"../escape"}),
    ] {
        let mut declared = sample_manifest();
        declared["ui"] = ui;
        write_plugin(plugin.path(), &declared);
        assert!(read_manifest(plugin.path()).is_err());
    }
}

#[test]
fn a_file_opener_is_refused_with_a_reason() {
    let plugin = plugin_dir();
    let mut declared = sample_manifest();
    declared["ui"] = json!({"file-opener": {"extensions": [".rs"], "job": "open"}});
    write_plugin(plugin.path(), &declared);
    let error = read_manifest(plugin.path()).unwrap_err().to_string();
    assert!(
        error.contains("ui.file-opener is not in this version of kagan"),
        "{error}"
    );
}

#[test]
fn a_status_item_is_one_titled_line() {
    let plugin = plugin_dir();
    let mut declared = sample_manifest();
    declared["ui"]["status"] = json!({ "title": "Usage" });
    write_plugin(plugin.path(), &declared);
    let read = read_manifest(plugin.path()).unwrap();
    assert_eq!(
        read.ui.status.as_ref().map(|status| status.title.as_str()),
        Some("Usage")
    );
    for status in [
        json!({}),
        json!({ "title": "" }),
        json!({ "title": "   " }),
        json!({ "title": "x".repeat(41) }),
        json!({ "title": "a\nb" }),
        json!({ "title": "a\u{202E}b" }),
        json!({ "title": "Usage", "view": "Usage" }),
    ] {
        declared["ui"]["status"] = status.clone();
        write_plugin(plugin.path(), &declared);
        assert!(read_manifest(plugin.path()).is_err(), "accepted {status}");
    }
}

#[test]
fn status_bar_points_to_the_status_item() {
    let plugin = plugin_dir();
    let mut declared = sample_manifest();
    declared["ui"] = json!({ "status-bar": [{}] });
    write_plugin(plugin.path(), &declared);
    let error = read_manifest(plugin.path()).unwrap_err().to_string();
    assert_eq!(
        error,
        format!(
            "kagan.json in {}: ui.status-bar is not in this version of kagan; declare ui.status for one line in the activity strip",
            plugin.path().display()
        )
    );
}

#[test]
fn a_status_item_names_its_capability() {
    let kinds = crate::capabilities_in(&json!({ "ui": { "status": { "title": "Usage" } } }));
    assert_eq!(kinds.len(), 1);
    assert_eq!(kinds[0].id, "status");
    assert_eq!(kinds[0].name, "Status item");
    assert_eq!(kinds[0].detail, "adds a line to the activity strip");
}

#[gpui_kit::test]
fn check_names_the_invalid_javascript_file_and_line(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    std::fs::write(
        plugin.path().join("main.js"),
        "export default class View {}\nconst = ;\n",
    )
    .unwrap();
    open(cx, &[]);
    let task = plugin_op(
        cx,
        PluginOp::Check {
            path: plugin.path().to_path_buf(),
        },
    );
    let error = cx
        .foreground_executor()
        .block_test(task)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("main.js") && error.contains("line 2"),
        "{error}"
    );
}
