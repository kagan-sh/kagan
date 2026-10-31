use gpui_kit::{Role, TestAppContext, px, size};
use serde_json::json;

use super::kagan::{
    click, focused_of, label, open, present, role_of, set_preference, shows, wait_until,
};
use super::sidebars::{focus_row, settle};

const ITEMS: [&str; 2] = ["kagan-about", "kagan-settings"];

#[gpui_kit::test]
fn the_rail_starts_with_the_kagan_mark_and_opens_its_menu(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    assert_eq!(role_of(cx, window, "kagan-menu"), Some(Role::Button));
    assert_eq!(label(cx, window, "kagan-menu"), "kagan");
    assert!(!present(cx, window, "kagan-menu-popup"));
    click(cx, window, "kagan-menu");
    wait_until(cx, |cx| shows(cx, window, "kagan-menu-popup"));
    assert_eq!(label(cx, window, "kagan-about"), "About kagan");
    assert_eq!(label(cx, window, "kagan-settings"), "Settings…");
}

#[gpui_kit::test]
fn about_in_the_kagan_menu_opens_about(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "kagan-menu");
    wait_until(cx, |cx| shows(cx, window, "kagan-about"));
    click(cx, window, "kagan-about");
    wait_until(cx, |cx| cx.update(|cx| cx.windows().len()) == 2);
    assert!(!present(cx, window, "kagan-menu-popup"));
}

#[gpui_kit::test]
fn settings_in_the_kagan_menu_opens_settings_and_keeps_it_open(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    for _ in 0..2 {
        click(cx, window, "kagan-menu");
        wait_until(cx, |cx| shows(cx, window, "kagan-settings"));
        click(cx, window, "kagan-settings");
        wait_until(cx, |cx| shows(cx, window, "settings-page"));
        assert!(!present(cx, window, "kagan-menu-popup"));
    }
}

#[gpui_kit::test]
fn the_keyboard_opens_the_kagan_menu_and_escape_gives_focus_back(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    for key in ["enter", "space"] {
        focus_row(cx, window, "kagan-menu");
        cx.simulate_keystrokes(window, key);
        cx.run_until_parked();
        assert!(
            shows(cx, window, "kagan-menu-popup"),
            "{key} opens the menu"
        );
        assert_eq!(focused_of(cx, window, "kagan-menu"), Some(false));
        cx.simulate_keystrokes(window, "escape");
        cx.run_until_parked();
        assert!(!present(cx, window, "kagan-menu-popup"));
        assert_eq!(focused_of(cx, window, "kagan-menu"), Some(true));
    }
}

#[gpui_kit::test]
fn the_kagan_menu_works_beside_the_narrow_sidebar(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    set_preference(cx, "main.left-sidebar", json!(true));
    cx.simulate_window_resize(window, size(px(640.), px(720.)));
    settle(cx);
    focus_row(cx, window, "project-prj_kagan");
    assert!(shows(cx, window, "left-overlay-panel"));
    click(cx, window, "kagan-menu");
    wait_until(cx, |cx| shows(cx, window, "kagan-menu-popup"));
    for item in ITEMS {
        assert!(shows(cx, window, item), "{item}");
    }
    assert!(shows(cx, window, "left-overlay-panel"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!present(cx, window, "kagan-menu-popup"));
    assert!(shows(cx, window, "left-overlay-panel"));
}

#[gpui_kit::test]
fn the_start_screen_shows_a_mark_that_takes_no_focus(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(528.), px(360.)));
    settle(cx);
    assert!(shows(cx, window, "welcome-mark"));
    assert_eq!(role_of(cx, window, "welcome-mark"), None);
    assert_eq!(focused_of(cx, window, "welcome-mark"), None);
    for id in [
        "welcome-title",
        "new-personal-thread",
        "welcome-add-project",
    ] {
        assert!(
            shows(cx, window, id),
            "{id} is in view at the smallest window"
        );
    }
}

#[gpui_kit::test]
fn a_click_on_another_control_closes_the_kagan_menu_and_keeps_focus_there(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    focus_row(cx, window, "kagan-menu");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert!(shows(cx, window, "kagan-menu-popup"));
    click(cx, window, "new-personal-thread");
    wait_until(cx, |cx| shows(cx, window, "program-search"));
    assert!(!present(cx, window, "kagan-menu-popup"));
    assert_eq!(focused_of(cx, window, "program-search"), Some(true));
    assert_eq!(focused_of(cx, window, "kagan-menu"), Some(false));
}

#[gpui_kit::test]
fn closing_settings_from_the_kagan_menu_gives_focus_back_to_the_mark(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    focus_row(cx, window, "kagan-menu");
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    click(cx, window, "kagan-settings");
    wait_until(cx, |cx| shows(cx, window, "settings-page"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "settings-page"));
    assert_eq!(focused_of(cx, window, "kagan-menu"), Some(true));
}
