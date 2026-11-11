use std::path::Path;

use gpui_kit::{AppContext as _, ElementId, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{Appearance, Op, PluginId, PluginOp, UiOp};
use serde_json::{Value, json};

use super::{
    aria_label, get_theme, install_plugins, plugin_config, run_op, settings_text, setup_only,
    write_plugin,
};
use crate::tests::kagan::{
    add_project, click, label, open, open_with_plugins_then, present, selected_of, set_preference,
    shows, value_of, wait_until,
};

const PREFS: &str = "dev.test.prefs";

#[gpui_kit::test]
fn agent_integration_saved_as_off_shows_off_at_start(cx: &mut TestAppContext) {
    let _data = setup_only(cx);
    set_preference(cx, "agent-integration", json!(false));
    let kagan = cx.update(|cx| crate::kagan::open(cx).unwrap());
    cx.run_until_parked();
    assert!(!cx.update(|cx| kagan.read(cx).agent_integration(cx)));
}

#[gpui_kit::test]
fn a_theme_card_reports_whether_it_is_the_chosen_theme(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    let before = get_theme(cx).theme;
    assert_eq!(
        selected_of(cx, window, &format!("theme-{before}")),
        Some(true)
    );

    if before == "ember" {
        return;
    }
    click(cx, window, "theme-ember");
    assert_eq!(selected_of(cx, window, "theme-ember"), Some(true));
    assert_eq!(
        selected_of(cx, window, &format!("theme-{before}")),
        Some(false)
    );
}

#[gpui_kit::test]
fn the_theme_picker_saves_the_theme(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    click(cx, window, "theme-ember");
    assert_eq!(get_theme(cx).theme, "ember");
}

#[gpui_kit::test]
fn the_mode_choice_saves_the_mode(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    click(cx, window, "mode-dark");
    assert_eq!(get_theme(cx).mode, Appearance::Dark);
    click(cx, window, "mode-system");
    assert_eq!(get_theme(cx).mode, Appearance::System);
}

#[gpui_kit::test]
fn a_plugin_setting_saves_from_its_page(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.prefs");
    assert!(present(cx, window, "setting-row-dev.test.prefs/sounds"));

    click(cx, window, "plugin-dev.test.prefs/sounds");
    assert_eq!(plugin_config(cx, "sounds"), json!(false));

    let select = "setting-select-dev.test.prefs/density";
    assert_eq!(label(cx, window, select), "comfortable");
    assert_eq!(value_of(cx, window, select).as_deref(), Some("comfortable"));
    assert!(
        !shows(cx, window, "option-dev.test.prefs/density/compact"),
        "the choices stay in the menu until it opens"
    );
    click(cx, window, select);
    wait_until(cx, |cx| {
        shows(cx, window, "option-dev.test.prefs/density/compact")
    });
    click(cx, window, "option-dev.test.prefs/density/compact");
    assert_eq!(plugin_config(cx, "density"), json!("compact"));
    wait_until(cx, |cx| label(cx, window, select) == "compact");
}

#[gpui_kit::test]
fn a_select_with_no_default_or_saved_value_says_not_chosen(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.prefs");
    let select = "setting-select-dev.test.prefs/tier";
    assert_eq!(label(cx, window, select), "Not chosen");
    assert_eq!(value_of(cx, window, select).as_deref(), Some("Not chosen"));
}

#[gpui_kit::test]
fn a_project_setting_picks_from_a_dropdown(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.prefs");
    assert_eq!(
        label(cx, window, "setting-project-dev.test.prefs/board"),
        "None"
    );
    click(cx, window, "setting-project-dev.test.prefs/board");
    wait_until(cx, |cx| {
        shows(cx, window, "project-pick-dev.test.prefs/board/prj_kagan")
    });
    click(cx, window, "project-pick-dev.test.prefs/board/prj_kagan");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "board"), json!("prj_kagan"));
    assert_eq!(
        label(cx, window, "setting-project-dev.test.prefs/board"),
        "Kagan project"
    );
    click(cx, window, "setting-project-dev.test.prefs/board");
    wait_until(cx, |cx| {
        shows(cx, window, "project-none-dev.test.prefs/board")
    });
    click(cx, window, "project-none-dev.test.prefs/board");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "board"), Value::Null);
    assert_eq!(
        label(cx, window, "setting-project-dev.test.prefs/board"),
        "None"
    );
}

#[gpui_kit::test]
fn a_missing_project_setting_keeps_its_id(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    let store = kg_store::Store::open(data.path().join("kagan.sqlite3")).unwrap();
    store
        .set_plugin_value(&PluginId(PREFS.into()), "board", &json!("prj_gone"))
        .unwrap();
    drop(store);
    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(PREFS.into()),
            thread_id: None,
        }),
    );
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.prefs");
    assert_eq!(plugin_config(cx, "board"), json!("prj_gone"));
    assert_eq!(
        label(cx, window, "setting-project-dev.test.prefs/board"),
        "Missing"
    );
    let note = aria_label(cx, window, "setting-row-dev.test.prefs/board-note").unwrap_or_default();
    assert!(
        note.contains("gone") && note.contains("Pick another"),
        "{note}"
    );
}

#[gpui_kit::test]
fn a_project_menu_scrolls_to_its_last_of_25_projects(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    let mut last = None;
    for n in 0..25 {
        let folder = data.path().join(format!("project-{n:02}"));
        std::fs::create_dir(&folder).unwrap();
        last = Some(add_project(cx, &folder).id);
    }
    let last = last.unwrap();
    cx.simulate_window_resize(window, size(px(1100.), px(420.)));
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.prefs");
    click(cx, window, "setting-project-dev.test.prefs/board");
    let pick = format!("project-pick-dev.test.prefs/board/{}", last.0);
    wait_until(cx, |cx| {
        shows(cx, window, "project-none-dev.test.prefs/board")
    });
    assert!(
        !shows(cx, window, &pick),
        "the last project starts below the short window"
    );
    run_op(
        cx,
        Op::Ui(UiOp::Scroll {
            element_id: pick.clone(),
            within: None,
            dy: 0.,
            window: 0,
        }),
    );
    wait_until(cx, |cx| shows(cx, window, &pick));
    let (row_bottom, viewport) = cx
        .update_window(window, |_, window, _| {
            let row = crate::ui_registry::find(window, &ElementId::Name(pick.clone().into()))
                .unwrap()
                .bounds;
            (row.bottom(), window.viewport_size().height)
        })
        .unwrap();
    assert!(
        row_bottom <= viewport,
        "the revealed row sits inside the window: bottom {row_bottom:?}, window {viewport:?}"
    );
    click(cx, window, &pick);
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "board"), json!(last.0));
}

#[gpui_kit::test]
fn removing_the_project_while_settings_is_open_shows_missing(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    let folder = data.path().join("removed-project");
    std::fs::create_dir(&folder).unwrap();
    let project = add_project(cx, &folder);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.prefs");
    click(cx, window, "setting-project-dev.test.prefs/board");
    let pick = format!("project-pick-dev.test.prefs/board/{}", project.id.0);
    wait_until(cx, |cx| shows(cx, window, &pick));
    click(cx, window, &pick);
    cx.run_until_parked();
    let removal = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(kg_domain::ProjectOp::Remove {
                id: project.id.clone(),
            }),
            cx,
        )
    });
    wait_until(cx, |cx| {
        cx.update(|cx| Workspace::global(cx).read(cx).approvals().next().is_some())
    });
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .unwrap()
            .id
            .clone()
    });
    let answered = cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    cx.foreground_executor().block_test(removal).unwrap();
    cx.run_until_parked();
    assert_eq!(
        label(cx, window, "setting-project-dev.test.prefs/board"),
        "Missing"
    );
    let note = aria_label(cx, window, "setting-row-dev.test.prefs/board-note").unwrap_or_default();
    assert!(note.contains("gone"), "{note}");
}

#[gpui_kit::test]
fn review_number_type_after_reload(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_string_retries);
    cx.run_until_parked();
    let select_all = if cfg!(target_os = "macos") {
        "cmd-a"
    } else {
        "ctrl-a"
    };
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.prefs");
    let field = format!("plugin-text-{PREFS}/retries");
    click(cx, window, &field);
    cx.simulate_keystrokes(window, select_all);
    cx.simulate_input(window, "nope");
    cx.run_until_parked();
    set_retries_type(data.path(), "number");
    reload_prefs(cx);
    click(cx, window, &field);
    cx.simulate_keystrokes(window, select_all);
    cx.simulate_input(window, "4");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "retries"), json!(4));
    let store = kg_store::Store::open(data.path().join("kagan.sqlite3")).unwrap();
    assert_eq!(
        store
            .plugin_value(&PluginId(PREFS.into()), "retries")
            .unwrap(),
        Some(json!(4)),
        "the saved value is a JSON number, not text"
    );
}

pub(in crate::tests) fn install_string_retries(data: &Path) {
    install_plugins(data);
    set_retries_type(data, "string");
}

pub(in crate::tests) fn set_retries_type(data: &Path, kind: &str) {
    let path = data.join("prefs-plugin").join("kagan.json");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let retries = &mut manifest["settings"]["retries"];
    retries["type"] = json!(kind);
    retries["default"] = if kind == "string" {
        json!("3")
    } else {
        json!(3)
    };
    std::fs::write(&path, manifest.to_string()).unwrap();
}

pub(in crate::tests) fn reload_prefs(cx: &mut TestAppContext) {
    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(PREFS.into()),
            thread_id: None,
        }),
    );
}

#[gpui_kit::test]
fn a_multiline_setting_saves_its_lines_from_the_editor(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.prefs");
    let field = format!("plugin-text-{PREFS}/log");
    assert!(present(cx, window, &field));

    click(cx, window, &field);
    cx.simulate_input(window, "first");
    cx.simulate_keystrokes(window, "enter");
    cx.simulate_input(window, "second");
    cx.run_until_parked();
    assert_eq!(settings_text(cx, "log"), "first\nsecond");
    assert_eq!(plugin_config(cx, "log"), Value::Null, "not saved yet");

    click(cx, window, &format!("setting-save-{PREFS}/log"));
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "log"), json!("first\nsecond"));

    let select_all = if cfg!(target_os = "macos") {
        "cmd-a"
    } else {
        "ctrl-a"
    };
    click(cx, window, &field);
    cx.simulate_keystrokes(window, select_all);
    cx.simulate_input(window, "draft");
    cx.run_until_parked();
    assert_eq!(settings_text(cx, "log"), "draft");
    click(cx, window, &format!("setting-revert-{PREFS}/log"));
    cx.run_until_parked();
    assert_eq!(settings_text(cx, "log"), "first\nsecond");
    assert_eq!(plugin_config(cx, "log"), json!("first\nsecond"));
}

fn set_multiline(data: &Path, key: &str, on: bool) {
    let path = data.join("prefs-plugin").join("kagan.json");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let setting = manifest["settings"][key].as_object_mut().unwrap();
    if on {
        setting.insert("multiline".into(), json!(true));
    } else {
        setting.remove("multiline");
    }
    std::fs::write(&path, manifest.to_string()).unwrap();
}

#[gpui_kit::test]
fn a_reload_that_toggles_multiline_swaps_the_row_widget_and_saves_both_ways(
    cx: &mut TestAppContext,
) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.prefs");
    let field = format!("plugin-text-{PREFS}/label");
    let select_all = if cfg!(target_os = "macos") {
        "cmd-a"
    } else {
        "ctrl-a"
    };

    click(cx, window, &field);
    cx.simulate_keystrokes(window, select_all);
    cx.simulate_input(window, "one line");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "label"), json!("one line"));

    set_multiline(data.path(), "label", true);
    reload_prefs(cx);
    cx.run_until_parked();
    assert!(present(cx, window, &format!("setting-save-{PREFS}/label")));
    click(cx, window, &field);
    cx.simulate_keystrokes(window, select_all);
    cx.simulate_input(window, "first");
    cx.simulate_keystrokes(window, "enter");
    cx.simulate_input(window, "second");
    cx.run_until_parked();
    assert_eq!(settings_text(cx, "label"), "first\nsecond");
    click(cx, window, &format!("setting-save-{PREFS}/label"));
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "label"), json!("first\nsecond"));

    set_multiline(data.path(), "label", false);
    reload_prefs(cx);
    cx.run_until_parked();
    assert!(!present(cx, window, &format!("setting-save-{PREFS}/label")));
    click(cx, window, &field);
    cx.simulate_keystrokes(window, select_all);
    cx.simulate_input(window, "back to one line");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(plugin_config(cx, "label"), json!("back to one line"));
}

#[gpui_kit::test]
fn a_long_choice_keeps_its_title_wider_than_it_is_tall(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        write_plugin(
            data,
            "programs",
            &json!({
                "id": "dev.test.programs",
                "name": "Kanban",
                "version": "1.0.0",
                "kagan": ">=0.1.0",
                "entry": "main.js",
                "instructions": "A test plugin with a long choice.",
                "powers": {},
                "settings": {
                    "program": {
                        "type": "select",
                        "title": "Program for new threads",
                        "options": [
                            "claude-program",
                            "codex-program",
                            "cursor-agent-program",
                            "opencode-program",
                            "gemini-program",
                            "kimi-program",
                            "amp-program",
                            "copilot-agent-program"
                        ],
                        "default": "claude-program"
                    }
                }
            }),
            "// @ts-check\nexport default class Plugin {}\n",
        );
    });
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.programs");
    let title = "setting-row-dev.test.programs/program-title";
    assert!(present(cx, window, title));
    let (width, height) = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name(title.into())).map_or(
                (0., 0.),
                |element| {
                    (
                        f32::from(element.bounds.size.width),
                        f32::from(element.bounds.size.height),
                    )
                },
            )
        })
        .unwrap();
    assert!(
        width > height,
        "title is {width} by {height}, so the letters stacked"
    );
}
