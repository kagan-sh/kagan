use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext, px, size};
use serde_json::json;

use super::{close_surface, install_pair, open_surface, origin_x, saved, tab_to, wide, width_of};
use crate::tests::kagan::{
    chord, click, focused_of, open_with_plugins_then, press_key, run_shell, shows, wait_until,
};
use crate::tests::sidebar::layout::focus_row;

const BETA: &str = "dev.test.beta";

#[gpui_kit::test]
fn leaving_settings_returns_to_the_row_that_opened_them(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    click(cx, window, "plugins-manage");
    assert!(shows(cx, window, "setting-row-install"));
    press_key(cx, window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "settings-page"));
    assert_eq!(focused_of(cx, window, "plugins-manage"), Some(true));
}

#[gpui_kit::test]
fn closing_the_second_tool_returns_to_its_row(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, BETA, "tool").unwrap();
    wait_until(cx, |cx| shows(cx, window, "beta-tool-count"));
    tab_to(cx, window, "beta-tool-count");
    close_surface(cx, BETA, "tool").unwrap();
    let beta = format!("surface-item-{BETA}-tool");
    assert_eq!(focused_of(cx, window, &beta), Some(true));
    assert!(!shows(cx, window, "beta-tool"));
    press_key(cx, window, "enter");
    wait_until(cx, |cx| shows(cx, window, "beta-tool"));
    assert!(!shows(cx, window, "alpha-tool"));
}

pub(in crate::tests) fn narrow(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.simulate_window_resize(window, size(px(528.), px(800.)));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn cmd_b_and_focus_sidebar_open_the_narrow_overlay_and_escape_closes_it(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    narrow(cx, window);
    let before = saved(cx, "main.left-sidebar");
    cx.simulate_keystrokes(window, &chord("ToggleSidebar"));
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    assert_eq!(focused_of(cx, window, "left-overlay-panel"), Some(true));
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert_eq!(saved(cx, "main.left-sidebar"), before);

    cx.simulate_keystrokes(window, &chord("FocusSidebar"));
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    assert_eq!(focused_of(cx, window, "project-prj_kagan"), Some(true));
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert_eq!(saved(cx, "main.left-sidebar"), before);
}

#[gpui_kit::test]
fn escape_closes_the_overlay_before_the_form_under_it(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    narrow(cx, window);
    cx.simulate_keystrokes(window, &chord("NewThread"));
    wait_until(cx, |cx| shows(cx, window, "start-prompt"));
    cx.simulate_keystrokes(window, &chord("ToggleSidebar"));
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert!(shows(cx, window, "start-prompt"));
}

#[gpui_kit::test]
fn clicking_the_dimmed_center_closes_the_narrow_overlay(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    narrow(cx, window);
    let before = saved(cx, "main.left-sidebar");
    click(cx, window, "left-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    let alpha = cx.update(|cx| crate::kagan::layout_read::left_scrim_alpha(kagan.read(cx), cx));
    assert!(alpha > 0. && alpha < 1., "scrim alpha {alpha}");
    // The panel covers the scrim's middle, so click the dimmed part to its right.
    let panel_right =
        origin_x(cx, window, "left-overlay-panel") + width_of(cx, window, "left-overlay-panel");
    let point = gpui_kit::point(px(panel_right + 60.), px(300.));
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click_at(window, point, cx);
    })
    .unwrap();
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert_eq!(saved(cx, "main.left-sidebar"), before);
}

#[gpui_kit::test]
fn picking_a_thread_in_the_narrow_overlay_closes_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    let id = run_shell(cx);
    wide(cx, window);
    narrow(cx, window);
    let before = saved(cx, "main.left-sidebar");
    click(cx, window, "left-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(id));
    assert_eq!(saved(cx, "main.left-sidebar"), before);
}

#[gpui_kit::test]
fn picking_a_project_in_the_narrow_overlay_shows_its_new_thread_form(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    narrow(cx, window);
    click(cx, window, "left-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    click(cx, window, "project-prj_kagan");
    wait_until(cx, |cx| shows(cx, window, "start-prompt"));
    assert!(!shows(cx, window, "left-overlay-panel"));
}

#[gpui_kit::test]
fn the_project_fold_button_ends_before_the_new_thread_button(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    assert_chevron_clear(cx, window, "a wide window");
}

#[gpui_kit::test]
fn the_project_fold_button_ends_before_the_plus_in_a_narrow_list(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    crate::tests::kagan::set_preference(cx, "main.left-width", json!(200.));
    wide(cx, window);
    cx.run_until_parked();
    assert_chevron_clear(cx, window, "a 200 point list");
}

#[gpui_kit::test]
fn the_project_fold_button_ends_before_the_plus_in_the_narrow_overlay(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    narrow(cx, window);
    assert_chevron_clear(cx, window, "the narrow overlay");
}

pub(in crate::tests) fn assert_chevron_clear(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    when: &str,
) {
    focus_row(cx, window, "project-prj_kagan");
    let chevron_end = origin_x(cx, window, "project-disclose-prj_kagan")
        + width_of(cx, window, "project-disclose-prj_kagan");
    let plus_start = origin_x(cx, window, "project-new-prj_kagan");
    assert!(
        chevron_end <= plus_start + 0.5,
        "in {when}: chevron ends at {chevron_end}, + starts at {plus_start}"
    );
}
