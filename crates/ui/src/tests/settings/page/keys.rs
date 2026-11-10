use std::path::Path;

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{KeymapOp, Op, OpResult, Plugin, PluginId, PluginOp};
use serde_json::{Value, json};

use super::{aria_label, keyboard_with_navigator, keycap, nav, nav_note, plugin_keys, run_op};
use crate::tests::kagan::{
    NAVIGATOR, chord, click, focused_of, open, open_with_plugins_then, plugin_log, present,
    press_key, shows, wait_until,
};

pub(in crate::tests) const TWIN: &str = "dev.kagan.twin";

/// A second plugin whose only command wants the key Navigator's Show already has.
pub(in crate::tests) fn install_twin(data: &Path) {
    let root = data.join("plugins").join("twin");
    std::fs::create_dir_all(&root).unwrap();
    let manifest = json!({
        "id": TWIN, "name": "Twin", "version": "1.0.0", "kagan": ">=0.1.0", "entry": "main.js",
        "commands": [{ "id": "echo", "title": "Echo", "key": "secondary-shift-y" }]
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        root.join("main.js"),
        "import { View } from 'gpui-kit';\nexport default class Plugin extends View { render() { return ''; } }\n",
    )
    .unwrap();
    kg_store::Store::open(data.join("kagan.sqlite3"))
        .unwrap()
        .insert_plugin(&Plugin {
            id: PluginId(TWIN.into()),
            source: root.display().to_string(),
            root: dunce::canonicalize(root).unwrap(),
            commit: None,
            enabled: true,
            approved: json!({ "powers": {}, "downloads": {}, "folders": {} }),
            skills: Vec::new(),
            error: None,
        })
        .unwrap();
}

#[gpui_kit::test]
fn two_plugins_that_want_one_key_both_say_so(cx: &mut TestAppContext) {
    let (window, _data) = keyboard_with_navigator(cx);
    let echo = format!("shortcut-plugin-{TWIN}-echo");
    click(cx, window, &format!("setting-row-plugin-{TWIN}-echo"));
    assert_eq!(aria_label(cx, window, &echo).as_deref(), Some("Not set"));
    assert_eq!(
        aria_label(cx, window, &format!("setting-row-plugin-{TWIN}-echo-note")),
        Some(format!(
            "{} is already used by Show in Navigator.",
            keycap("secondary-shift-y")
        ))
    );
    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-show"));
    assert_eq!(
        aria_label(cx, window, &nav("show")),
        Some(keycap("secondary-shift-y"))
    );
    assert_eq!(
        nav_note(cx, window, "show").as_deref(),
        Some("Echo in Twin also wants this key.")
    );
}

#[gpui_kit::test]
fn a_key_the_program_needs_is_not_saved(cx: &mut TestAppContext) {
    let (window, _data) = keyboard_with_navigator(cx);
    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-probe"));
    for key in ["a", "ctrl-c"] {
        click(cx, window, &format!("{}-key", nav("probe")));
        cx.simulate_keystrokes(window, key);
        cx.run_until_parked();
        assert_eq!(
            aria_label(cx, window, &nav("probe")).as_deref(),
            Some("Not set")
        );
        assert!(
            nav_note(cx, window, "probe")
                .is_some_and(|note| note.starts_with("That key belongs to the program.")),
            "{key}"
        );
        assert_eq!(plugin_keys(cx)[format!("{NAVIGATOR}:probe")], Value::Null);
    }
}

#[gpui_kit::test]
fn every_plugin_command_has_a_keyboard_row_and_a_clash_shows_on_both(cx: &mut TestAppContext) {
    let (window, _data) = keyboard_with_navigator(cx);
    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-probe"));
    assert_eq!(
        aria_label(
            cx,
            window,
            &format!("settings-heading-shortcuts-plugin-{NAVIGATOR}")
        )
        .as_deref(),
        Some("Navigator")
    );
    assert_eq!(
        aria_label(cx, window, &nav("probe")).as_deref(),
        Some("Not set")
    );
    assert_eq!(
        aria_label(cx, window, &format!("{}-key", nav("probe"))).as_deref(),
        Some("Bind")
    );

    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-show"));
    assert_eq!(
        aria_label(cx, window, &nav("show")),
        Some(keycap("secondary-shift-y"))
    );
    assert_eq!(
        aria_label(cx, window, &format!("{}-key", nav("show"))).as_deref(),
        Some("Change")
    );
    assert!(present(cx, window, &format!("{}-remove", nav("show"))));

    // New Thread keeps its key. Clash wanted it, so its row says what took it, and so does New Thread's.
    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-clash"));
    assert_eq!(
        aria_label(cx, window, &nav("clash")).as_deref(),
        Some("Not set")
    );
    assert_eq!(
        nav_note(cx, window, "clash"),
        Some(format!(
            "{} is already used by New thread.",
            keycap(&chord("NewThread"))
        ))
    );
    click(cx, window, "setting-row-NewThread");
    let taken = aria_label(cx, window, "setting-row-NewThread-note").unwrap_or_default();
    assert!(
        taken.contains("Clash in Navigator also wants this key."),
        "{taken}"
    );
    assert!(
        taken.contains("Chord in Navigator also wants this key."),
        "{taken}"
    );
}

#[gpui_kit::test]
fn a_plugin_command_binds_in_settings_and_its_key_runs_it(cx: &mut TestAppContext) {
    let (window, _data) = keyboard_with_navigator(cx);
    let row = format!("setting-row-plugin-{NAVIGATOR}-probe");
    click(cx, window, &row);
    click(cx, window, &format!("{}-key", nav("probe")));
    assert_eq!(
        aria_label(cx, window, &nav("probe")).as_deref(),
        Some("Press a key")
    );

    // Escape stops without saving, and Settings stays open.
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert_eq!(
        aria_label(cx, window, &nav("probe")).as_deref(),
        Some("Not set")
    );
    assert!(shows(cx, window, "settings-page"));

    // A key Show already has is not saved, and the row says who has it.
    click(cx, window, &format!("{}-key", nav("probe")));
    cx.simulate_keystrokes(window, "secondary-shift-y");
    wait_until(cx, |cx| nav_note(cx, window, "probe").is_some());
    assert_eq!(
        nav_note(cx, window, "probe"),
        Some(format!(
            "{} is already used by Show in Navigator.",
            keycap("secondary-shift-y")
        ))
    );
    assert_eq!(plugin_keys(cx)[format!("{NAVIGATOR}:probe")], Value::Null);

    click(cx, window, &format!("{}-key", nav("probe")));
    cx.simulate_keystrokes(window, "f8");
    wait_until(cx, |cx| {
        aria_label(cx, window, &nav("probe")) == Some(keycap("f8"))
    });
    assert_eq!(plugin_keys(cx)[format!("{NAVIGATOR}:probe")], "f8");
    assert_eq!(nav_note(cx, window, "probe"), None);

    click(cx, window, "settings-close");
    cx.simulate_keystrokes(window, "f8");
    wait_until(cx, |cx| plugin_log(cx, NAVIGATOR).contains("probe refused"));
}

#[gpui_kit::test]
fn a_plugin_key_is_removed_and_its_rows_go_with_the_plugin(cx: &mut TestAppContext) {
    let (window, _data) = keyboard_with_navigator(cx);
    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-show"));
    click(cx, window, &format!("{}-remove", nav("show")));
    wait_until(cx, |cx| {
        aria_label(cx, window, &nav("show")).as_deref() == Some("Not set")
    });
    assert_eq!(plugin_keys(cx)[format!("{NAVIGATOR}:show")], Value::Null);

    // The removed key stays removed when the plugin comes back.
    let id = PluginId(NAVIGATOR.into());
    run_op(cx, Op::Plugin(PluginOp::Disable { id: id.clone() }));
    assert!(!present(
        cx,
        window,
        &format!("settings-group-shortcuts-plugin-{NAVIGATOR}")
    ));
    assert!(!present(cx, window, &nav("show")));
    assert!(plugin_keys(cx).get(format!("{NAVIGATOR}:show")).is_none());
    run_op(cx, Op::Plugin(PluginOp::Enable { id }));
    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-show"));
    assert_eq!(
        aria_label(cx, window, &nav("show")).as_deref(),
        Some("Not set")
    );
    assert_eq!(plugin_keys(cx)[format!("{NAVIGATOR}:show")], Value::Null);
}

#[gpui_kit::test]
fn change_on_a_review_shortcut_saves_it_under_its_keymap_name(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-keyboard");
    click(cx, window, "setting-row-NextChange");
    click(cx, window, "shortcut-NextChange-key");
    cx.simulate_keystrokes(window, "f9");
    wait_until(cx, |cx| {
        aria_label(cx, window, "shortcut-NextChange") == Some(keycap("f9"))
    });
    let get = cx.update(|cx| Workspace::dispatch(Op::Keymap(KeymapOp::Get), cx));
    let OpResult::Setting(keys) = cx.foreground_executor().block_test(get).unwrap() else {
        panic!("the keymap")
    };
    assert_eq!(keys["shared"]["NextHunk"], "f9");
    assert_eq!(aria_label(cx, window, "setting-row-NextChange-note"), None);
}

#[gpui_kit::test]
fn the_key_button_keeps_focus_while_it_waits_and_after(cx: &mut TestAppContext) {
    let (window, _data) = keyboard_with_navigator(cx);
    let button = format!("{}-key", nav("probe"));
    click(cx, window, &format!("setting-row-plugin-{NAVIGATOR}-probe"));
    let mut reached = false;
    for _ in 0..200 {
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
        if focused_of(cx, window, &button) == Some(true) {
            reached = true;
            break;
        }
    }
    assert!(reached, "Tab reaches Bind");
    press_key(cx, window, "enter");
    assert_eq!(aria_label(cx, window, &button).as_deref(), Some("Cancel"));
    assert_eq!(focused_of(cx, window, &button), Some(true));
    cx.simulate_keystrokes(window, "f8");
    wait_until(cx, |cx| {
        aria_label(cx, window, &button).as_deref() == Some("Change")
    });
    assert_eq!(focused_of(cx, window, &button), Some(true));
}

pub(in crate::tests) const ZZ: &str = "dev.zz.tool";
pub(in crate::tests) const AA: &str = "dev.aa.tool";

pub(in crate::tests) fn install_tool(data: &Path, id: &str, key: Option<&str>, enabled: bool) {
    let root = data.join("plugins").join(id);
    std::fs::create_dir_all(&root).unwrap();
    let mut command = json!({ "id": "run", "title": "Run" });
    if let Some(key) = key {
        command["key"] = json!(key);
    }
    let manifest = json!({
        "id": id, "name": id, "version": "1.0.0", "kagan": ">=0.1.0", "entry": "main.js",
        "commands": [command]
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        root.join("main.js"),
        "import { View } from 'gpui-kit';\nexport default class Plugin extends View { render() { return ''; } }\n",
    )
    .unwrap();
    kg_store::Store::open(data.join("kagan.sqlite3"))
        .unwrap()
        .insert_plugin(&Plugin {
            id: PluginId(id.into()),
            source: root.display().to_string(),
            root: dunce::canonicalize(root).unwrap(),
            commit: None,
            enabled,
            approved: json!({ "powers": {}, "downloads": {}, "folders": {} }),
            skills: Vec::new(),
            error: None,
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_saved_key_wins_over_a_plugin_installed_later(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install_tool(data, ZZ, None, true);
        install_tool(data, AA, Some("f8"), false);
    });
    run_op(
        cx,
        Op::Keymap(KeymapOp::Set {
            action: format!("{ZZ}:run"),
            key: "f8".into(),
            reset: false,
        }),
    );
    // aa sorts first, and its manifest asks for the same key.
    run_op(
        cx,
        Op::Plugin(PluginOp::Enable {
            id: PluginId(AA.into()),
        }),
    );
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-keyboard");
    let zz = format!("shortcut-plugin-{ZZ}-run");
    let aa = format!("shortcut-plugin-{AA}-run");
    click(cx, window, &format!("setting-row-plugin-{ZZ}-run"));
    assert_eq!(aria_label(cx, window, &zz), Some(keycap("f8")));
    click(cx, window, &format!("setting-row-plugin-{AA}-run"));
    assert_eq!(aria_label(cx, window, &aa).as_deref(), Some("Not set"));
    assert_eq!(
        aria_label(cx, window, &format!("setting-row-plugin-{AA}-run-note")),
        Some(format!("{} is already used by Run in {ZZ}.", keycap("f8")))
    );
}
