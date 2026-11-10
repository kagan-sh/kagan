use gpui_kit::TestAppContext;

use serde_json::json;

use super::{install_plugins, saved, tab_to};
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
