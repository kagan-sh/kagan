use gpui_kit::TestAppContext;

use serde_json::json;

use kg_domain::{Op, PluginId, PluginOp};
use kg_plugins::PluginHost;

use super::{install_plugins, run_op, saved, tab_to};
use crate::tests::kagan::{click, open, open_with_plugins_then, present, selected_of};

#[gpui_kit::test]
fn picking_a_section_shows_only_its_rows(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    assert_eq!(
        selected_of(cx, window, "settings-section-appearance"),
        Some(true)
    );
    assert!(present(cx, window, "setting-row-theme"));
    assert!(present(cx, window, "setting-row-mode"));
    assert!(!present(cx, window, "setting-row-notify"));

    click(cx, window, "settings-section-general");
    assert!(present(cx, window, "setting-row-notify"));
    assert!(!present(cx, window, "setting-row-theme"));
}

#[gpui_kit::test]
fn search_finds_a_row_in_another_section_and_jumps_there(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    tab_to(cx, window, "settings-search");
    cx.simulate_input(window, "text size");
    cx.run_until_parked();
    assert!(present(cx, window, "settings-result-appearance-0"));
    assert!(!present(cx, window, "settings-section-general"));
    assert!(
        present(cx, window, "setting-row-notify"),
        "the chosen section stays on screen while searching"
    );

    click(cx, window, "settings-result-appearance-0");
    cx.run_until_parked();
    assert_eq!(
        selected_of(cx, window, "settings-section-appearance"),
        Some(true)
    );
    assert!(present(cx, window, "setting-row-terminal-font-size"));
    assert!(present(cx, window, "settings-section-general"));
}

#[gpui_kit::test]
fn enter_in_the_search_box_opens_the_first_result(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    tab_to(cx, window, "settings-search");
    cx.simulate_input(window, "text size");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(
        selected_of(cx, window, "settings-section-appearance"),
        Some(true)
    );
    assert!(present(cx, window, "setting-row-terminal-font-size"));
}

#[gpui_kit::test]
fn arrows_move_the_section_list_and_enter_opens_a_section(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    tab_to(cx, window, "settings-search");
    cx.simulate_keystrokes(window, "tab");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "down");
    cx.simulate_keystrokes(window, "down");
    cx.simulate_keystrokes(window, "up");
    cx.run_until_parked();
    assert!(!present(cx, window, "setting-row-theme"));

    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(
        selected_of(cx, window, "settings-section-appearance"),
        Some(true)
    );
    assert!(present(cx, window, "setting-row-theme"));
}

#[gpui_kit::test]
fn a_plugin_with_settings_is_listed_under_plugins(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    assert!(present(
        cx,
        window,
        "settings-section-plugin-dev.test.prefs"
    ));

    click(cx, window, "settings-section-plugin-dev.test.prefs");
    assert!(present(cx, window, "setting-row-dev.test.prefs/sounds"));
    assert!(!present(cx, window, "setting-row-notify"));
}

#[gpui_kit::test]
fn the_ask_before_archiving_switch_saves_the_preference(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-general");
    assert!(present(cx, window, "setting-row-confirm-archive"));
    click(cx, window, "confirm-archive");
    assert_eq!(saved(cx, "confirm-archive"), json!(true));
    click(cx, window, "confirm-archive");
    assert_eq!(saved(cx, "confirm-archive"), json!(false));
}

#[gpui_kit::test]
fn enter_after_typing_opens_the_first_result_not_the_old_cursor(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-updates");
    tab_to(cx, window, "settings-search");
    cx.simulate_input(window, "a");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(
        selected_of(cx, window, "settings-section-general"),
        Some(true)
    );
}

#[gpui_kit::test]
fn down_in_the_search_box_moves_to_the_next_result(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    tab_to(cx, window, "settings-search");
    cx.simulate_input(window, "text size");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "down");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(
        selected_of(cx, window, "settings-section-keyboard"),
        Some(true)
    );
}

#[gpui_kit::test]
fn a_plugin_section_comes_back_after_its_plugin_reloads(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-plugin-dev.test.prefs");
    cx.update(|cx| {
        PluginHost::global(cx).update(cx, |host, cx| {
            host.disable_loaded(&PluginId("dev.test.prefs".into()), cx);
        });
    });
    cx.run_until_parked();
    assert!(present(cx, window, "setting-row-notify"));

    run_op(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId("dev.test.prefs".into()),
            thread_id: None,
        }),
    );
    cx.run_until_parked();
    assert!(present(cx, window, "setting-row-dev.test.prefs/sounds"));
}
