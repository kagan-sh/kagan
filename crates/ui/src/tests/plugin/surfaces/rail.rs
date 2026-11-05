use gpui_kit::{AppContext as _, TestAppContext, px, size};
use kg_domain::{Op, SettingsOp, SurfaceOp};

use super::{
    install_many, install_pair, item, narrow, open_surface, origin_x, origin_y, run, surfaces,
    tab_to, wide, width_of,
};
use crate::tests::kagan::{
    click, focused_of, label, open_with_plugins_then, press_key, run_shell, selected_of, shows,
    wait_until,
};

const ALPHA: &str = "dev.test.alpha";

#[gpui_kit::test]
fn rail_switches_between_tool_and_threads(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let tool = format!("surface-item-{ALPHA}-tool");
    click(cx, window, &tool);
    assert!(
        item(&surfaces(cx), ALPHA, "tool").open,
        "{:?}",
        surfaces(cx)
    );
    assert!(
        item(&surfaces(cx), ALPHA, "tool").visible,
        "{:?}",
        surfaces(cx)
    );
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
    assert!(shows(cx, window, "project-prj_kagan"));
    click(cx, window, "threads-home");
    wait_until(cx, |cx| shows(cx, window, "project-prj_kagan"));
    assert!(!shows(cx, window, "alpha-tool"));
    open_surface(cx, ALPHA, "tool").unwrap();
    run(cx, Op::Surface(SurfaceOp::Threads)).unwrap();
    assert!(shows(cx, window, "project-prj_kagan"));
    click(cx, window, "settings-gear");
    assert!(shows(cx, window, "settings-page"));
}

#[gpui_kit::test]
fn rail_has_the_expected_width_and_plugin_settings_opens(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    assert!((width_of(cx, window, "sidebar-rail") - 48.).abs() < 0.5);
    assert!((width_of(cx, window, &format!("surface-item-{ALPHA}-tool")) - 24.).abs() < 0.5);
    run(cx, Op::Settings(SettingsOp::Plugins)).unwrap();
    wait_until(cx, |cx| shows(cx, window, "setting-row-install"));
}

#[gpui_kit::test]
fn the_rail_has_one_icon_per_plugin_above_the_plugin_buttons(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_many);
    wide(cx, window);
    let tool = "surface-item-dev.test.many-t0";
    assert!(shows(cx, window, tool));
    assert!(!shows(cx, window, "surface-item-dev.test.many-t1"));
    let tool_y = origin_y(cx, window, tool);
    assert!(origin_y(cx, window, "plugins-manage") > tool_y);
    assert!(origin_y(cx, window, "plugins-add") > origin_y(cx, window, "plugins-manage"));
    assert_eq!(label(cx, window, "plugins-manage"), "Manage plugins");
    assert_eq!(label(cx, window, "plugins-add"), "New plugin…");
}

#[gpui_kit::test]
fn clicking_the_open_rail_tool_returns_to_threads(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let tool = format!("surface-item-{ALPHA}-tool");
    click(cx, window, &tool);
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
    assert_eq!(selected_of(cx, window, &tool), Some(true));
    click(cx, window, &tool);
    wait_until(cx, |cx| shows(cx, window, "project-prj_kagan"));
    assert!(!shows(cx, window, "alpha-tool"));
    assert_eq!(selected_of(cx, window, &tool), Some(false));
}

#[gpui_kit::test]
fn a_narrow_window_shows_the_sidebar_over_the_center(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    cx.simulate_window_resize(window, size(px(528.), px(800.)));
    cx.run_until_parked();
    assert!(!shows(cx, window, "project-prj_kagan"));
    click(cx, window, "left-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    assert!(shows(cx, window, "project-prj_kagan"));
    assert_eq!(label(cx, window, "left-sidebar-toggle"), "Hide sidebar");
    assert!(shows(cx, window, "threads-home"));
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert!(shows(cx, window, "threads-home"));
}

#[gpui_kit::test]
fn at_528_the_rail_stays_and_show_sidebar_draws_the_list_over_the_center(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    cx.simulate_window_resize(window, size(px(528.), px(800.)));
    cx.run_until_parked();
    let viewport = cx
        .update_window(window, |_, window, _| {
            f32::from(window.viewport_size().width)
        })
        .unwrap();
    assert!((viewport - 528.).abs() < 0.5, "viewport {viewport}");
    assert!(!shows(cx, window, "project-prj_kagan"));
    assert!(shows(cx, window, "sidebar-rail"));
    assert!((origin_x(cx, window, "sidebar-rail")).abs() < 0.5);
    assert!((width_of(cx, window, "sidebar-rail") - 48.).abs() < 0.5);

    click(cx, window, "left-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    assert_eq!(label(cx, window, "left-sidebar-toggle"), "Hide sidebar");
    assert!(shows(cx, window, "project-prj_kagan"));
    assert!(shows(cx, window, "sidebar-rail"));
    assert!((origin_x(cx, window, "sidebar-rail")).abs() < 0.5);
    let panel_x = origin_x(cx, window, "left-overlay-panel");
    let panel_width = width_of(cx, window, "left-overlay-panel");
    assert!((panel_x - 48.).abs() < 0.5, "panel at {panel_x}");
    assert!(
        panel_x + panel_width <= 528.5,
        "panel {panel_x}+{panel_width}"
    );
}

#[gpui_kit::test]
fn the_rail_has_a_page_icon_that_opens_and_closes_the_page(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_many);
    wide(cx, window);
    let page = "surface-item-dev.test.many-page";
    assert!(shows(cx, window, page));
    assert_eq!(label(cx, window, page), "Board");
    let tool_y = origin_y(cx, window, "surface-item-dev.test.many-t0");
    let page_y = origin_y(cx, window, page);
    assert!(page_y > tool_y);
    assert!(origin_y(cx, window, "plugins-manage") > page_y);

    click(cx, window, page);
    wait_until(cx, |cx| shows(cx, window, "surface-title"));
    assert_eq!(selected_of(cx, window, page), Some(true));
    click(cx, window, page);
    wait_until(cx, |cx| !shows(cx, window, "surface-title"));
    assert_eq!(selected_of(cx, window, page), Some(false));
}

#[gpui_kit::test]
fn a_rail_tool_at_a_narrow_width_opens_over_the_center(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    cx.simulate_window_resize(window, size(px(528.), px(800.)));
    cx.run_until_parked();
    click(cx, window, &format!("surface-item-{ALPHA}-tool"));
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
    assert!(shows(cx, window, "left-overlay-panel"));
    assert!(item(&surfaces(cx), ALPHA, "tool").visible);
    assert_eq!(focused_of(cx, window, "left-overlay-panel"), Some(true));

    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert!(width_of(cx, window, "center") >= 480. - 0.5);
    assert!(!shows(cx, window, "surface-title"));
}

#[gpui_kit::test]
fn a_row_menu_at_a_narrow_width_opens_in_the_overlay(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    let id = run_shell(cx);
    wide(cx, window);
    narrow(cx, window);
    click(cx, window, "left-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    click(cx, window, &format!("thread-actions-{}", id.0));
    wait_until(cx, |cx| shows(cx, window, "popup-menu"));
    assert!(shows(cx, window, "left-overlay-panel"));
    assert!(shows(cx, window, "popup-menu"));
}

#[gpui_kit::test]
fn rail_tool_is_reachable_by_keyboard(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let tool = format!("surface-item-{ALPHA}-tool");
    tab_to(cx, window, &tool);
    press_key(cx, window, "enter");
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
}

pub(in crate::tests) const TINT: &str = "dev.test.tint";

pub(in crate::tests) const TINT_OVERLAY: &str = r##"import { View, div } from "gpui-kit";
import { log } from "kagan/host";
export default class Plugin extends View {
  render() { return ""; }
}
export class Cover extends View {
  init() { log("info", "mounted"); }
  render() { return div().size_full().bg("#ff00ff"); }
}
"##;

pub(in crate::tests) const THROWING_OVERLAY: &str = r#"import { View } from "gpui-kit";
export default class Plugin extends View {
  render() { return ""; }
}
export class Cover extends View {
  render() { throw new Error("boom in the overlay"); }
}
"#;
