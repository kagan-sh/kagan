use std::path::Path;

use gpui_kit::{AnyWindowHandle, TestAppContext};

use super::{height_of, install_pair, origin_x, origin_y, use_environment, wide, width_of};
#[cfg(unix)]
use super::{plugin_agent, wait_for_plugin};
use crate::tests::kagan::{
    click, label, open, open_with_plugins_then, run_shell, shows, thread_record, wait_until,
};
use crate::tests::sidebar::layout::focus_row;

#[cfg(unix)]
#[gpui_kit::test]
fn new_plugin_shows_the_real_discovery_error_and_can_try_again(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let empty = data.path().join("no-agents");
    std::fs::create_dir_all(&empty).unwrap();
    use_environment(cx, &empty, Path::new("/nonexistent/kagan-shell"));
    click(cx, window, "plugins-add");
    wait_until(cx, |cx| label(cx, window, "note").contains("the shell"));
    assert!(!label(cx, window, "note").contains("No agent"));

    plugin_agent(cx, data.path(), "printf 'Ready> '; read line; sleep 30");
    click(cx, window, "plugins-add");
    wait_for_plugin(cx, &kagan);
    let id = cx.update(|cx| kagan.read(cx).shown_thread(cx)).unwrap();
    assert_eq!(thread_record(cx, &id).title, "New plugin");
}

/// Checks that a menu opened from `button` sits under it, with its right edge on the button's.
pub(in crate::tests) fn assert_under_button(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    button: &str,
    menu: &str,
) {
    let (bx, by) = (origin_x(cx, window, button), origin_y(cx, window, button));
    let (bw, bh) = (width_of(cx, window, button), height_of(cx, window, button));
    let (mx, my) = (origin_x(cx, window, menu), origin_y(cx, window, menu));
    let mw = width_of(cx, window, menu);
    assert!(
        (my - (by + bh)).abs() < 8.,
        "menu top {my}, button bottom {}",
        by + bh
    );
    assert!(
        (mx + mw - (bx + bw)).abs() < 8.,
        "menu right {}, button right {}",
        mx + mw,
        bx + bw
    );
}

#[gpui_kit::test]
fn the_thread_menu_from_a_row_opens_under_the_row_button(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    let id = run_shell(cx);
    wide(cx, window);
    click(cx, window, &format!("thread-{}", id.0));
    let button = format!("thread-actions-{}", id.0);
    click(cx, window, &button);
    wait_until(cx, |cx| shows(cx, window, "popup-menu"));
    assert_under_button(cx, window, &button, "popup-menu");
}

#[gpui_kit::test]
fn the_title_bar_menu_still_opens_under_its_button(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    let id = run_shell(cx);
    wide(cx, window);
    click(cx, window, &format!("thread-{}", id.0));
    click(cx, window, "thread-actions-trigger");
    wait_until(cx, |cx| shows(cx, window, "popup-menu"));
    assert_under_button(cx, window, "thread-actions-trigger", "popup-menu");
}

#[gpui_kit::test]
fn the_project_menu_opens_under_its_button(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_pair);
    let folder = data.path().join("demo");
    std::fs::create_dir_all(&folder).unwrap();
    let project = crate::tests::kagan::add_project(cx, &folder);
    wide(cx, window);
    cx.run_until_parked();
    let button = format!("project-actions-{}", project.id.0);
    focus_row(cx, window, &format!("project-{}", project.id.0));
    click(cx, window, &button);
    // The project menu is gpui-kit's dropdown, so its first item stands in for the menu.
    wait_until(cx, |cx| shows(cx, window, "project-settings"));
    let (bx, by) = (origin_x(cx, window, &button), origin_y(cx, window, &button));
    let bh = height_of(cx, window, &button);
    let (item_x, item_y) = (
        origin_x(cx, window, "project-settings"),
        origin_y(cx, window, "project-settings"),
    );
    assert!(
        (item_y - (by + bh)).abs() < 40.,
        "menu item top {item_y}, button bottom {}",
        by + bh
    );
    assert!(
        (item_x - bx).abs() < 240.,
        "menu item left {item_x}, button left {bx}"
    );
}
