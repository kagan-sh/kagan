use crate::kagan::screen_read;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ElementId, Modifiers, TestAppContext, point, px, size,
};
use kg_core::Workspace;
use kg_domain::{DropSide, InteractionOption, Op, OpError, OpResult, PreferenceOp, ThreadOp, UiOp};
use serde_json::{Value, json};

use crate::kagan::changes::POLL;
use crate::tests::kagan::{
    add_project, chord, click, drag_to, focused_of, hover_to, label, open, present, press,
    press_key, release, repo_thread, run_child, run_shell, selected_of, set_preference, shown,
    shows, value_of, wait_until,
};

fn saved(cx: &mut TestAppContext, key: &str) -> Value {
    let get = cx.update(|cx| {
        Workspace::dispatch(Op::Preference(PreferenceOp::Get { key: key.into() }), cx)
    });
    let OpResult::Setting(value) = cx.foreground_executor().block_test(get).unwrap() else {
        panic!("setting");
    };
    value
}

fn left_shown(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    shows(cx, window, "project-prj_kagan")
}

#[gpui_kit::test]
fn empty_panels_do_not_open_from_the_button_or_shortcut(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    for width in [800., 1492.] {
        cx.simulate_window_resize(window, size(px(width), px(720.)));
        cx.run_until_parked();
        let center = span(cx, window, "center").expect("center");
        assert_eq!(
            crate::tests::plugin::panels::disabled_of(cx, window, "right-sidebar-toggle"),
            Some(true)
        );
        click(cx, window, "right-sidebar-toggle");
        assert!(!shows(cx, window, "right-overlay-panel"));
        assert_eq!(span(cx, window, "center"), Some(center));
        cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
        cx.run_until_parked();
        assert!(!shows(cx, window, "right-overlay-panel"));
        assert_eq!(span(cx, window, "center"), Some(center));
        assert_eq!(
            value_of(cx, window, "right-sidebar-toggle").as_deref(),
            Some("hidden")
        );
        assert_eq!(saved(cx, "main.right-sidebar"), Value::Null);
    }
}

#[gpui_kit::test]
fn questions_appear_before_changes_and_empty_windows_have_no_column(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    assert!(!shows(cx, window, "right-sidebar"));
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| shows(cx, window, "changes-not-git"));
    assert!(!shows(cx, window, "needs-you"));
    assert!(!shows(cx, window, "changes-scope-uncommitted"));
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    wait_until(cx, |cx| shows(cx, window, "needs-you"));
    assert!(
        col_x(cx, window, "needs-you-header") < col_x(cx, window, "changes-header"),
        "Questions tab sits left of Changes"
    );
}

#[gpui_kit::test]
fn the_waiting_toggle_opens_questions(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(800.), px(720.)));
    cx.run_until_parked();
    let _asked = cx.update(|cx| {
        Workspace::ask_for_person(None, None, "Install the plugin?".into(), vec![], cx)
    });
    cx.run_until_parked();
    click(cx, window, "right-sidebar-toggle");
    click(cx, window, "right-sidebar-toggle");
    assert!(
        shows(cx, window, "needs-you-body"),
        "the toggle that shows the waiting count opens Questions"
    );
}

#[gpui_kit::test]
fn hidden_panels_say_the_shown_thread_has_changes(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    cx.simulate_window_resize(window, size(px(480.), px(720.)));
    cx.run_until_parked();
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    wait_until(cx, |cx| {
        value_of(cx, window, "right-sidebar-toggle").as_deref() == Some("hidden, 1 changed")
    });
    assert!(shows(cx, window, "right-sidebar-changes"));
    click(cx, window, "right-sidebar-toggle");
    assert_eq!(
        value_of(cx, window, "right-sidebar-toggle").as_deref(),
        Some("shown")
    );
    assert!(!shows(cx, window, "right-sidebar-changes"));
}

#[gpui_kit::test]
fn the_left_sidebar_hides_and_shows_from_its_button(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    assert!(left_shown(cx, window));
    click(cx, window, "left-sidebar-toggle");
    assert!(!left_shown(cx, window));
    assert_eq!(saved(cx, "main.left-sidebar"), json!(false));
    click(cx, window, "left-sidebar-toggle");
    assert!(left_shown(cx, window));
}

#[gpui_kit::test]
fn the_left_sidebar_hides_and_shows_from_its_shortcut(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_keystrokes(window, &chord("ToggleSidebar"));
    cx.run_until_parked();
    assert!(!left_shown(cx, window));
    cx.simulate_keystrokes(window, &chord("ToggleSidebar"));
    cx.run_until_parked();
    assert!(left_shown(cx, window));
}

#[gpui_kit::test]
fn the_right_sidebar_hides_and_shows_from_its_button(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    assert!(shows(cx, window, "needs-you"));
    click(cx, window, "right-sidebar-toggle");
    assert!(!shows(cx, window, "needs-you"));
    click(cx, window, "right-sidebar-toggle");
    assert!(shows(cx, window, "needs-you"));
}

#[gpui_kit::test]
fn a_preference_set_by_kg_moves_the_sidebar(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    set_preference(cx, "main.left-sidebar", json!(false));
    assert!(!left_shown(cx, window));
    set_preference(cx, "main.left-sidebar", json!(true));
    assert!(left_shown(cx, window));
}

#[gpui_kit::test]
fn a_dragged_sidebar_width_is_saved(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.resize_left(px(300.), window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert!((saved(cx, "main.left-width").as_f64().unwrap() - 300.).abs() < 1.);
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.resize_right(px(400.), window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert!((saved(cx, "main.right-width").as_f64().unwrap() - 400.).abs() < 1.);
}

#[gpui_kit::test]
fn a_saved_width_is_shown_at_once(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    set_preference(cx, "main.right-width", json!(360.));
    let shown = cx.update(|cx| kagan.read(cx).right_width_now(cx));
    assert!((shown - 360.).abs() < 1., "shown {shown}");
}

#[gpui_kit::test]
fn a_saved_left_width_is_shown_at_once(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    set_preference(cx, "main.left-width", json!(330.));
    let width = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &gpui_kit::ElementId::Name("sidebar-pane".into()))
                .map_or(0., |element| f32::from(element.bounds.size.width))
        })
        .unwrap();
    assert!((width - 330.).abs() < 1., "shown {width}");
}

#[gpui_kit::test]
fn a_saved_left_width_is_shown_after_settings(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    set_preference(cx, "main.left-width", json!(330.));
    let width = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &gpui_kit::ElementId::Name("sidebar-pane".into()))
                .map_or(0., |element| f32::from(element.bounds.size.width))
        })
        .unwrap();
    assert!((width - 330.).abs() < 1., "shown {width}");
}

/// The x, width and right edge of an element in the window, in points.
pub(crate) fn span(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: &str,
) -> Option<(f32, f32)> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(id.into())).map(|element| {
            let bounds = element.bounds;
            (f32::from(bounds.origin.x), f32::from(bounds.size.width))
        })
    })
    .unwrap()
}

#[gpui_kit::test]
fn widening_the_window_gives_the_center_the_extra_room(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(1100.), px(720.)));
    cx.run_until_parked();
    let left = span(cx, window, "sidebar-pane").unwrap().1;
    let center = span(cx, window, "center").unwrap().1;
    cx.simulate_window_resize(window, size(px(1500.), px(720.)));
    cx.run_until_parked();
    let wider_left = span(cx, window, "sidebar-pane").unwrap().1;
    let wider_center = span(cx, window, "center").unwrap().1;
    assert!(
        (wider_left - left).abs() < 2.,
        "left {left} became {wider_left}"
    );
    assert!(
        wider_center - center > 395.,
        "center {center} became {wider_center}"
    );
}

#[gpui_kit::test]
fn the_center_keeps_its_room_at_every_window_size(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| shows(cx, window, "thread-title"));
    // The narrowest the window may be, a width with only the left sidebar, a normal width, and a
    // wide one. Each row says which sidebars show at that width.
    for (width, left, right) in [
        (528., false, false),
        (800., true, false),
        (1120., true, true),
        (1492., true, true),
    ] {
        cx.simulate_window_resize(window, size(px(width), px(720.)));
        cx.run_until_parked();
        // The right sidebar holds the changes list, so its panel shows through that element.
        let right_shown = shows(cx, window, "changes") || shows(cx, window, "changes-none");
        assert_eq!(
            (shows(cx, window, "project-prj_kagan"), right_shown),
            (left, right),
            "sidebars at {width}"
        );
        let (center_x, center_width) = span(cx, window, "center").unwrap();
        assert!(
            center_width >= 480. - 0.5,
            "center {center_width} at {width}"
        );
        assert!(
            center_x + center_width <= width + 0.5,
            "center leaves the window at {width}"
        );
        let (actions_x, actions_width) = span(cx, window, "thread-actions-trigger").unwrap();
        assert!(
            actions_x + actions_width <= width + 0.5,
            "actions leave the window at {width}"
        );
    }
}

#[gpui_kit::test]
fn a_saved_width_beyond_its_range_is_clamped(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    set_preference(cx, "main.left-width", json!(9999.));
    let (_, width) = span(cx, window, "sidebar-pane").unwrap();
    assert!((width - 420.).abs() < 1., "shown {width}");
}

/// Paints the window again, so the bounds read afterwards are the ones on screen.
fn redraw(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
}

#[gpui_kit::test]
fn the_center_keeps_its_room_while_a_sidebar_is_dragged(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| shows(cx, window, "thread-title"));
    cx.simulate_window_resize(window, size(px(1100.), px(720.)));
    cx.run_until_parked();
    let (left_x, left_width) = span(cx, window, "sidebar-pane").unwrap();
    let edge = left_x + left_width;
    // The same steps gpui-kit's own drag test takes. The pointer is still down when the center
    // is measured, so the limit has to hold during the drag, not only at release.
    press(
        cx,
        window,
        point(px(edge - 2.), px(300.)),
        1,
        Modifiers::default(),
    );
    redraw(cx, window);
    drag_to(cx, window, point(px(edge + 10.), px(300.)));
    redraw(cx, window);
    drag_to(cx, window, point(px(420.), px(300.)));
    redraw(cx, window);
    let (_, left) = span(cx, window, "sidebar-pane").unwrap();
    let (_, center) = span(cx, window, "center").unwrap();
    release(
        cx,
        window,
        point(px(420.), px(300.)),
        1,
        Modifiers::default(),
    );
    assert!(
        left > 280.,
        "the drag did not move the left sidebar: {left}"
    );
    assert!(center >= 480. - 0.5, "center {center} during the drag");
}

#[gpui_kit::test]
fn a_hidden_sidebar_keeps_its_saved_width(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| shows(cx, window, "thread-title"));
    set_preference(cx, "main.left-width", json!(280.));
    set_preference(cx, "main.right-width", json!(300.));
    cx.simulate_window_resize(window, size(px(1492.), px(720.)));
    cx.run_until_parked();
    cx.simulate_window_resize(window, size(px(480.), px(720.)));
    cx.run_until_parked();
    assert!(!left_shown(cx, window), "the left sidebar stayed at 480");
    assert!(
        !shows(cx, window, "changes"),
        "the right sidebar stayed at 480"
    );
    assert_eq!(saved(cx, "main.left-width"), json!(280.));
    assert_eq!(saved(cx, "main.right-width"), json!(300.));
}

fn viewport_width(cx: &mut TestAppContext, window: AnyWindowHandle) -> f32 {
    cx.update_window(window, |_, window, _| {
        f32::from(window.viewport_size().width)
    })
    .unwrap()
}

fn row_y(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .expect(name)
            .bounds
            .origin
            .y
            .as_f32()
    })
    .unwrap()
}

fn col_x(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .expect(name)
            .bounds
            .origin
            .x
            .as_f32()
    })
    .unwrap()
}

#[gpui_kit::test]
fn a_closed_right_sidebar_reopens_its_column_when_the_window_is_wide(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    let width = viewport_width(cx, window);
    assert!((width - 1120.).abs() < 1., "viewport {width}");
    click(cx, window, "right-sidebar-toggle");
    assert!(!shows(cx, window, "needs-you"));
    assert!(!shows(cx, window, "right-overlay-panel"));
    assert_eq!(saved(cx, "main.right-sidebar"), json!(false));
    assert_eq!(
        value_of(cx, window, "right-sidebar-toggle").as_deref(),
        Some("hidden, 1 waiting")
    );
    click(cx, window, "right-sidebar-toggle");
    assert!(shows(cx, window, "needs-you"));
    assert!(!shows(cx, window, "right-overlay-panel"));
    assert_eq!(saved(cx, "main.right-sidebar"), json!(true));
    assert_eq!(
        value_of(cx, window, "right-sidebar-toggle").as_deref(),
        Some("shown")
    );
}

#[gpui_kit::test]
fn a_saved_closed_overlay_opens_the_column_on_one_toggle_when_wide(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    set_preference(cx, "main.right-sidebar", json!(false));
    cx.simulate_window_resize(window, size(px(800.), px(720.)));
    cx.run_until_parked();
    let narrow = viewport_width(cx, window);
    assert!((narrow - 800.).abs() < 1., "viewport {narrow}");
    click(cx, window, "right-sidebar-toggle");
    assert!(shows(cx, window, "right-overlay-panel"));
    assert_eq!(saved(cx, "main.right-sidebar"), json!(false));

    cx.simulate_window_resize(window, size(px(1492.), px(720.)));
    cx.run_until_parked();
    let wide = viewport_width(cx, window);
    assert!((wide - 1492.).abs() < 1., "viewport {wide}");
    assert!(shows(cx, window, "right-overlay-panel"));
    click(cx, window, "right-sidebar-toggle");
    assert!(shows(cx, window, "needs-you"));
    assert!(!shows(cx, window, "right-overlay-panel"));
    assert_eq!(saved(cx, "main.right-sidebar"), json!(true));
    assert_eq!(
        value_of(cx, window, "right-sidebar-toggle").as_deref(),
        Some("shown")
    );
}

#[gpui_kit::test]
fn a_narrow_window_draws_the_panels_without_changing_the_saved_sidebar(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    set_preference(cx, "main.right-sidebar", json!(true));
    cx.simulate_window_resize(window, size(px(800.), px(720.)));
    cx.run_until_parked();
    let width = viewport_width(cx, window);
    assert!((width - 800.).abs() < 1., "viewport {width}");
    assert!(!shows(cx, window, "needs-you"));
    assert_eq!(saved(cx, "main.right-sidebar"), json!(true));
    assert_eq!(
        value_of(cx, window, "right-sidebar-toggle").as_deref(),
        Some("hidden, 1 waiting")
    );
    let (center_x, center_width) = span(cx, window, "center").unwrap();

    click(cx, window, "right-sidebar-toggle");
    assert!(shows(cx, window, "needs-you"));
    assert!(shows(cx, window, "right-overlay-panel"));
    assert!(!shows(cx, window, "right-sidebar-waiting"));
    assert_eq!(
        focused_of(cx, window, "needs-you-header"),
        Some(true),
        "focus starts on Needs you"
    );
    assert_eq!(saved(cx, "main.right-sidebar"), json!(true));
    assert_eq!(
        value_of(cx, window, "right-sidebar-toggle").as_deref(),
        Some("shown")
    );
    let (panel_x, panel_width) = span(cx, window, "right-overlay-panel").unwrap();
    assert!((panel_width - 320.).abs() < 1., "drawn width {panel_width}");
    assert!(
        (panel_x + panel_width - width).abs() < 2.,
        "panel {panel_x}+{panel_width} window {width}"
    );
    let (after_x, after_width) = span(cx, window, "center").unwrap();
    assert!((after_x - center_x).abs() < 1.);
    assert!((after_width - center_width).abs() < 1.);
}

#[gpui_kit::test]
fn widening_the_left_overlay_keeps_the_focused_row(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    set_preference(cx, "main.left-sidebar", json!(true));
    cx.simulate_window_resize(window, size(px(640.), px(720.)));
    cx.run_until_parked();
    focus_row(cx, window, "project-prj_kagan");
    assert!(shows(cx, window, "left-overlay-panel"));
    assert_eq!(focused_of(cx, window, "project-prj_kagan"), Some(true));
    cx.simulate_window_resize(window, size(px(1492.), px(720.)));
    cx.run_until_parked();
    assert!(shows(cx, window, "sidebar-pane"));
    assert!(!shows(cx, window, "left-overlay-panel"));
    assert_eq!(focused_of(cx, window, "project-prj_kagan"), Some(true));
}

#[gpui_kit::test]
fn widening_the_right_overlay_keeps_the_focused_header(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    set_preference(cx, "main.right-sidebar", json!(true));
    cx.simulate_window_resize(window, size(px(800.), px(720.)));
    cx.run_until_parked();
    click(cx, window, "right-sidebar-toggle");
    assert!(shows(cx, window, "right-overlay-panel"));
    assert_eq!(focused_of(cx, window, "needs-you-header"), Some(true));
    cx.simulate_window_resize(window, size(px(1492.), px(720.)));
    cx.run_until_parked();
    assert!(shows(cx, window, "needs-you-header"));
    assert!(!shows(cx, window, "right-overlay-panel"));
    assert_eq!(focused_of(cx, window, "needs-you-header"), Some(true));
}

#[gpui_kit::test]
fn opening_a_side_overlay_replaces_the_other(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(640.), px(720.)));
    cx.run_until_parked();
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &chord("ToggleSidebar"));
    cx.run_until_parked();
    assert!(shows(cx, window, "left-overlay-panel"));
    assert!(!shows(cx, window, "right-overlay-panel"));
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    cx.run_until_parked();
    assert!(!shows(cx, window, "left-overlay-panel"));
    assert!(shows(cx, window, "right-overlay-panel"));
    cx.simulate_keystrokes(window, &chord("ToggleSidebar"));
    cx.run_until_parked();
    assert!(shows(cx, window, "left-overlay-panel"));
    assert!(!shows(cx, window, "right-overlay-panel"));
}

#[gpui_kit::test]
fn closing_the_panels_returns_to_the_focused_sidebar_row(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(800.), px(720.)));
    cx.run_until_parked();
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    if shows(cx, window, "right-overlay-panel") {
        press_key(cx, window, "escape");
    }
    focus_row(cx, window, "project-prj_kagan");
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    wait_until(cx, |cx| shows(cx, window, "right-overlay-panel"));
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "right-overlay-panel"));
    assert_eq!(focused_of(cx, window, "project-prj_kagan"), Some(true));
}

#[gpui_kit::test]
fn the_drawn_panels_close_without_the_saved_sidebar_and_return_when_wide(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    set_preference(cx, "main.right-sidebar", json!(true));
    cx.simulate_window_resize(window, size(px(800.), px(720.)));
    cx.run_until_parked();
    click(cx, window, "right-sidebar-toggle");
    assert!(shows(cx, window, "right-overlay-panel"));
    press_key(cx, window, "escape");
    assert!(!shows(cx, window, "right-overlay-panel"));
    assert!(!shows(cx, window, "needs-you"));
    assert_eq!(focused_of(cx, window, "right-sidebar-toggle"), Some(true));
    assert_eq!(saved(cx, "main.right-sidebar"), json!(true));

    click(cx, window, "right-sidebar-toggle");
    click(cx, window, "right-overlay-scrim");
    assert!(!shows(cx, window, "right-overlay-panel"));
    assert_eq!(focused_of(cx, window, "right-sidebar-toggle"), Some(true));
    assert_eq!(saved(cx, "main.right-sidebar"), json!(true));

    cx.simulate_window_resize(window, size(px(1492.), px(720.)));
    cx.run_until_parked();
    let wide = viewport_width(cx, window);
    assert!((wide - 1492.).abs() < 1., "viewport {wide}");
    assert!(shows(cx, window, "needs-you"));
    assert!(!shows(cx, window, "right-overlay-panel"));
    assert_eq!(saved(cx, "main.right-sidebar"), json!(true));
    assert_eq!(
        value_of(cx, window, "right-sidebar-toggle").as_deref(),
        Some("shown")
    );
}

#[gpui_kit::test]
fn a_hidden_sidebar_counts_what_is_waiting_and_refuses_a_synthetic_answer(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    set_preference(cx, "main.right-sidebar", json!(true));
    cx.simulate_window_resize(window, size(px(800.), px(720.)));
    cx.run_until_parked();
    let options = vec![InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
        role: None,
    }];
    let first = cx.update(|cx| {
        Workspace::ask_for_person(
            None,
            None,
            "Install the plugin?".into(),
            options.clone(),
            cx,
        )
    });
    let second = cx
        .update(|cx| Workspace::ask_for_person(None, None, "Keep the plugin?".into(), options, cx));
    cx.run_until_parked();
    // A person-only question opens the panels over the center. Close them to see the count.
    assert!(shows(cx, window, "needs-you-header"));
    click(cx, window, "right-sidebar-toggle");
    assert_eq!(
        value_of(cx, window, "right-sidebar-toggle").as_deref(),
        Some("hidden, 2 waiting")
    );
    assert!(shows(cx, window, "right-sidebar-waiting"));
    let (button, dot) = cx
        .update_window(window, |_, window, _| {
            let place = |name: &str| {
                let bounds = crate::ui_registry::find(window, &ElementId::Name(name.into()))
                    .unwrap_or_else(|| panic!("{name}"))
                    .bounds;
                (
                    f32::from(bounds.origin.x),
                    f32::from(bounds.origin.y),
                    f32::from(bounds.origin.x + bounds.size.width),
                    f32::from(bounds.origin.y + bounds.size.height),
                )
            };
            (
                place("right-sidebar-toggle"),
                place("right-sidebar-waiting"),
            )
        })
        .unwrap();
    let count_height = dot.3 - dot.1;
    assert!(
        count_height > 8. && dot.0 >= button.0 && dot.2 <= button.2,
        "count {dot:?} sits beside the toggle {button:?}"
    );
    assert!(!shows(cx, window, "needs-you"));
    click(cx, window, "right-sidebar-toggle");
    assert!(shows(cx, window, "needs-you"));
    assert!(!shows(cx, window, "right-sidebar-waiting"));
    assert_eq!(
        value_of(cx, window, "right-sidebar-toggle").as_deref(),
        Some("shown")
    );
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("approval")
            .id
            .0
            .clone()
    });
    {
        let _armed = crate::synthetic::arm();
        click(cx, window, &format!("answer-{approval}-yes"));
        assert!(crate::synthetic::take_person_blocked());
    }
    assert_eq!(
        cx.update(|cx| Workspace::global(cx).read(cx).approvals().count()),
        2
    );
    assert_eq!(saved(cx, "main.right-sidebar"), json!(true));
    drop((first, second));
}

#[gpui_kit::test]
fn settings_hides_the_drawn_panels_and_does_not_bring_them_back(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(800.), px(720.)));
    cx.run_until_parked();
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    // The person-only question has already opened the panels.
    assert!(shows(cx, window, "right-overlay-panel"));
    assert!(shows(cx, window, "needs-you"));
    click(cx, window, "settings-gear");
    assert!(!shows(cx, window, "right-overlay-panel"));
    assert!(!shows(cx, window, "needs-you"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "right-overlay-panel"));
    assert!(!shows(cx, window, "needs-you"));
}

#[gpui_kit::test]
fn colliding_project_names_show_the_shortest_parent(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let root = data.path();
    let left = root.join("a/webapp");
    let right = root.join("b/webapp");
    let solo = root.join("solo");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    std::fs::create_dir_all(&solo).unwrap();
    let left = add_project(cx, &left);
    let right = add_project(cx, &right);
    let solo = add_project(cx, &solo);
    cx.run_until_parked();
    let left_id = format!("project-{}", left.id.0);
    let right_id = format!("project-{}", right.id.0);
    let solo_id = format!("project-{}", solo.id.0);
    wait_until(cx, |cx| label(cx, window, &left_id).contains('·'));
    assert_eq!(label(cx, window, &left_id), "webapp · a");
    assert_eq!(label(cx, window, &right_id), "webapp · b");
    assert_eq!(label(cx, window, &solo_id), "solo");
    assert_eq!(
        label(cx, window, &format!("project-disclose-{}", left.id.0)),
        "Hide threads in webapp · a"
    );
    assert_eq!(
        label(cx, window, &format!("project-new-{}", left.id.0)),
        "New thread in webapp · a"
    );
    assert_eq!(label(cx, window, "project-prj_kagan"), "Kagan project");
    assert_eq!(
        label(cx, window, "project-disclose-prj_kagan"),
        "Hide threads in Kagan project"
    );
    let names = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .sidebar(cx)
            .projects
            .iter()
            .map(|project| project.name.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        names
            .iter()
            .filter(|name| name.as_str() == "webapp")
            .count(),
        2
    );
    assert!(names.iter().any(|name| name == "solo"));
    assert!(row_y(cx, window, &left_id) < row_y(cx, window, &solo_id));
    assert!(row_y(cx, window, &solo_id) < row_y(cx, window, "project-prj_kagan"));
    click(cx, window, &left_id);
    assert_eq!(selected_of(cx, window, &left_id), Some(true));
}

fn project_names(cx: &mut TestAppContext) -> Vec<String> {
    cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .sidebar(cx)
            .projects
            .iter()
            .map(|project| project.name.clone())
            .collect()
    })
}

fn ui(cx: &mut TestAppContext, op: UiOp) -> Result<OpResult, OpError> {
    let task = cx.update(|cx| Workspace::dispatch(Op::Ui(op), cx));
    let result = cx.foreground_executor().block_test(task);
    cx.run_until_parked();
    result
}

#[gpui_kit::test]
fn dragging_a_project_header_onto_another_reorders_the_sidebar(cx: &mut TestAppContext) {
    let (_kagan, _window, data) = open(cx);
    let root = data.path();
    let alpha = root.join("alpha");
    let beta = root.join("beta");
    let gamma = root.join("gamma");
    std::fs::create_dir_all(&alpha).unwrap();
    std::fs::create_dir_all(&beta).unwrap();
    std::fs::create_dir_all(&gamma).unwrap();
    let _alpha = add_project(cx, &alpha);
    let beta = add_project(cx, &beta);
    let gamma = add_project(cx, &gamma);
    cx.run_until_parked();
    wait_until(cx, |cx| {
        project_names(cx)
            .iter()
            .filter(|name| matches!(name.as_str(), "alpha" | "beta" | "gamma"))
            .count()
            == 3
    });
    assert_eq!(
        project_names(cx)
            .into_iter()
            .filter(|name| matches!(name.as_str(), "alpha" | "beta" | "gamma"))
            .collect::<Vec<_>>(),
        ["alpha", "beta", "gamma"]
    );
    ui(
        cx,
        UiOp::Drag {
            from: format!("project-{}", gamma.id.0),
            to: format!("project-{}", beta.id.0),
            at: DropSide::Top,
            within: None,
            hold: false,
            window: 0,
        },
    )
    .unwrap();
    wait_until(cx, |cx| {
        project_names(cx)
            .into_iter()
            .filter(|name| matches!(name.as_str(), "alpha" | "beta" | "gamma"))
            .collect::<Vec<_>>()
            == ["alpha", "gamma", "beta"]
    });
    assert_eq!(
        project_names(cx).last().map(String::as_str),
        Some("Personal")
    );
}

#[gpui_kit::test]
fn escape_cancels_a_project_header_drag_without_reordering(cx: &mut TestAppContext) {
    let (_kagan, _window, data) = open(cx);
    let root = data.path();
    let first = root.join("first");
    let second = root.join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let first = add_project(cx, &first);
    let second = add_project(cx, &second);
    cx.run_until_parked();
    wait_until(cx, |cx| {
        project_names(cx).iter().any(|name| name == "first")
            && project_names(cx).iter().any(|name| name == "second")
    });
    let before = project_names(cx);
    ui(
        cx,
        UiOp::Drag {
            from: format!("project-{}", second.id.0),
            to: format!("project-{}", first.id.0),
            at: DropSide::Top,
            within: None,
            hold: true,
            window: 0,
        },
    )
    .unwrap();
    assert_eq!(
        ui(
            cx,
            UiOp::Key {
                keystroke: "escape".into(),
                window: 0,
            }
        ),
        Ok(OpResult::Done)
    );
    assert_eq!(project_names(cx), before);
    assert_eq!(
        ui(cx, UiOp::Drop { window: 0 }),
        Err(OpError::Invalid(
            "no drag is held; start one with kg ui drag --hold".into()
        ))
    );
}

#[gpui_kit::test]
fn a_sidebar_refresh_keeps_a_held_project_drag_so_drop_still_reorders(cx: &mut TestAppContext) {
    let (_kagan, _window, data) = open(cx);
    let root = data.path();
    let first = root.join("first");
    let second = root.join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let first = add_project(cx, &first);
    let second = add_project(cx, &second);
    let thread = run_shell(cx);
    cx.run_until_parked();
    wait_until(cx, |cx| {
        project_names(cx).iter().any(|name| name == "first")
            && project_names(cx).iter().any(|name| name == "second")
    });
    assert_eq!(
        project_names(cx)
            .into_iter()
            .filter(|name| matches!(name.as_str(), "first" | "second"))
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    ui(
        cx,
        UiOp::Drag {
            from: format!("project-{}", second.id.0),
            to: format!("project-{}", first.id.0),
            at: DropSide::Top,
            within: None,
            hold: true,
            window: 0,
        },
    )
    .unwrap();
    let pinned = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::SetPinned {
                id: thread,
                pinned: true,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(pinned).unwrap();
    cx.run_until_parked();
    assert_eq!(ui(cx, UiOp::Drop { window: 0 }), Ok(OpResult::Done));
    wait_until(cx, |cx| {
        project_names(cx)
            .into_iter()
            .filter(|name| matches!(name.as_str(), "first" | "second"))
            .collect::<Vec<_>>()
            == ["second", "first"]
    });
}

#[gpui_kit::test]
fn a_shared_parent_folder_keeps_the_folder_above_it(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let root = data.path();
    let left = root.join("team/client/webapp");
    let right = root.join("other/client/webapp");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let left = add_project(cx, &left);
    let right = add_project(cx, &right);
    cx.run_until_parked();
    let left_id = format!("project-{}", left.id.0);
    let right_id = format!("project-{}", right.id.0);
    wait_until(cx, |cx| label(cx, window, &left_id).contains('/'));
    assert_eq!(label(cx, window, &left_id), "webapp · team/client");
    assert_eq!(label(cx, window, &right_id), "webapp · other/client");
    assert_eq!(
        label(cx, window, &format!("project-new-{}", right.id.0)),
        "New thread in webapp · other/client"
    );
}

#[gpui_kit::test]
fn duplicate_names_keep_the_suffix_at_the_minimum_width(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let root = data.path();
    let alpha = root.join("thr_ab12cd34-1/example-app-service");
    let beta = root.join("projects/example-app-service");
    std::fs::create_dir_all(&alpha).unwrap();
    std::fs::create_dir_all(&beta).unwrap();
    let alpha = add_project(cx, &alpha);
    let beta = add_project(cx, &beta);
    cx.run_until_parked();
    cx.simulate_window_resize(window, size(px(1312.), px(720.)));
    cx.run_until_parked();
    let alpha_id = format!("project-suffix-{}", alpha.id.0);
    let beta_id = format!("project-suffix-{}", beta.id.0);
    for width in [200., 256.] {
        cx.update_window(window, |_, window, cx| {
            kagan.update(cx, |kagan, cx| kagan.resize_left(px(width), window, cx));
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(label(cx, window, &alpha_id), "thr_ab12cd34-1");
        assert_eq!(label(cx, window, &beta_id), "projects");
        for id in [&alpha.id, &beta.id] {
            let (_, width) = span(cx, window, &format!("project-name-{id}")).expect("name");
            assert!(width >= 40., "name width {width}");
        }
        let (alpha_x, alpha_w) = span(cx, window, &alpha_id).expect("alpha");
        let (_beta_x, beta_w) = span(cx, window, &beta_id).expect("beta");
        assert!(alpha_w > 32., "alpha suffix {alpha_w}");
        assert!(beta_w > 32., "beta suffix {beta_w}");
        let (button_x, _) =
            span(cx, window, &format!("project-new-{}", alpha.id.0)).expect("new thread");
        assert!(
            button_x - (alpha_x + alpha_w) >= 7.,
            "suffix {alpha_x}+{alpha_w} touches the button at {button_x} (width {width})"
        );
        let (row_x, row_w) = span(cx, window, &format!("project-{}", alpha.id.0)).expect("row");
        assert!(
            alpha_x + alpha_w <= row_x + row_w + 1.,
            "suffix {alpha_x}+{alpha_w} row {row_x}+{row_w}"
        );
    }
}

#[gpui_kit::test]
fn a_narrow_opening_grows_back_to_the_preferred_sidebar(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(760.), px(720.)));
    cx.run_until_parked();
    cx.simulate_window_resize(window, size(px(1312.), px(720.)));
    cx.run_until_parked();
    let width = span(cx, window, "sidebar-pane").expect("sidebar").1;
    assert!(width > 240., "sidebar {width}");
    let saved_width = saved(cx, "main.left-width");
    if let Some(value) = saved_width.as_f64() {
        assert!(
            (value - 200.).abs() > 1.,
            "a squeezed width was saved: {value}"
        );
    }
}

#[gpui_kit::test]
fn the_left_width_survives_resizes_and_settings(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    set_preference(cx, "main.left-width", json!(300.));
    for width in [848., 1492., 948., 1312.] {
        cx.simulate_window_resize(window, size(px(width), px(720.)));
        cx.run_until_parked();
        let left = span(cx, window, "sidebar-pane").expect("sidebar").1;
        assert!((left - 300.).abs() < 2., "sidebar {left} at {width}");
    }
    click(cx, window, "settings-gear");
    cx.run_until_parked();
    assert!(!shows(cx, window, "sidebar-pane"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    let left = span(cx, window, "sidebar-pane").expect("sidebar").1;
    assert!((left - 300.).abs() < 2., "sidebar after settings {left}");
}

#[gpui_kit::test]
fn a_chosen_minimum_width_stays_when_the_window_is_wide(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(1312.), px(720.)));
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.resize_left(px(200.), window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    let width = span(cx, window, "sidebar-pane").expect("sidebar").1;
    assert!((width - 200.).abs() < 8., "sidebar {width}");
    let saved_width = saved(cx, "main.left-width").as_f64().unwrap();
    assert!((saved_width - 200.).abs() < 1., "saved {saved_width}");
}

#[gpui_kit::test]
fn the_left_toggle_names_the_sidebar_that_is_drawn(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    set_preference(cx, "main.left-sidebar", json!(true));
    cx.simulate_window_resize(window, size(px(640.), px(720.)));
    cx.run_until_parked();
    assert!(!shows(cx, window, "sidebar-pane"));
    assert_eq!(label(cx, window, "left-sidebar-toggle"), "Show sidebar");
    assert_eq!(
        value_of(cx, window, "left-sidebar-toggle").as_deref(),
        Some("hidden")
    );
    assert_eq!(saved(cx, "main.left-sidebar"), json!(true));
    cx.simulate_window_resize(window, size(px(1312.), px(720.)));
    cx.run_until_parked();
    assert!(shows(cx, window, "sidebar-pane"));
    assert_eq!(label(cx, window, "left-sidebar-toggle"), "Hide sidebar");
    assert_eq!(
        value_of(cx, window, "left-sidebar-toggle").as_deref(),
        Some("shown")
    );
}

#[gpui_kit::test]
fn archived_footer_appears_only_after_archive(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    assert!(!shows(cx, window, "archived"));
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_keystrokes(window, &chord("ArchiveThread"));
    wait_until(cx, |cx| shows(cx, window, "archived"));
    click(cx, window, "archived");
    assert_eq!(selected_of(cx, window, "project-prj_kagan"), Some(false));
    assert!(shows(cx, window, "archived-search"));
}

#[gpui_kit::test]
fn the_archive_icon_archives_its_thread(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-archive-{}", id.0));
    wait_until(cx, |cx| shows(cx, window, "archived"));
    assert!(!shows(cx, window, &format!("thread-{}", id.0)));
}

pub(crate) fn focus_row(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) {
    cx.simulate_keystrokes(window, &chord("FocusSidebar"));
    cx.run_until_parked();
    for _ in 0..40 {
        if focused_of(cx, window, name) == Some(true) {
            return;
        }
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
    }
    panic!("{name} never took focus");
}

#[gpui_kit::test]
fn left_and_right_fold_and_unfold_the_focused_project(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    focus_row(cx, window, "project-prj_kagan");

    press_key(cx, window, "left");
    cx.run_until_parked();
    assert_eq!(
        label(cx, window, "project-disclose-prj_kagan"),
        "Show threads in Kagan project"
    );

    press_key(cx, window, "right");
    cx.run_until_parked();
    assert_eq!(
        label(cx, window, "project-disclose-prj_kagan"),
        "Hide threads in Kagan project"
    );
}

#[gpui_kit::test]
fn new_thread_on_a_focused_project_starts_it_there(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let path = data.path().join("other");
    std::fs::create_dir_all(&path).unwrap();
    let other = add_project(cx, &path);
    cx.run_until_parked();
    focus_row(cx, window, &format!("project-{}", other.id.0));

    cx.simulate_keystrokes(window, &chord("NewThread"));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(other.id.clone())
    );
}

#[gpui_kit::test]
fn a_hidden_zero_width_disclose_is_not_clicked(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let name = ElementId::Name("project-disclose-prj_kagan".into());
    let result = cx
        .update_window(window, |_, window, cx| {
            crate::ui_dispatch::click(window, &name, cx)
        })
        .unwrap();
    assert!(
        matches!(result, Err(kg_domain::OpError::NotFound(ref message)) if message.contains("is not visible")),
        "{result:?}"
    );
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        None
    );
}

pub(crate) fn hover_row(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) {
    let bounds = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
                .unwrap()
                .bounds
        })
        .unwrap();
    hover_to(
        cx,
        window,
        point(bounds.origin.x + px(5.), bounds.origin.y + px(5.)),
    );
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_thread_menu_opens_without_showing_its_row(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let thread = run_shell(cx);
    let before = shown(cx, &kagan);
    hover_row(cx, window, &format!("thread-{thread}"));
    click(cx, window, &format!("thread-actions-{thread}"));
    cx.run_until_parked();
    assert!(shows(cx, window, "popup-menu"));
    assert_eq!(shown(cx, &kagan), before);
}

#[gpui_kit::test]
fn a_nested_row_keeps_its_actions_inside_the_narrow_sidebar(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    set_preference(cx, "main.left-width", json!(200));
    let mut chain = vec![run_shell(cx)];
    for _ in 0..5 {
        let child = run_child(cx, chain.last().expect("parent"));
        chain.push(child);
    }
    cx.run_until_parked();
    let (sidebar_x, sidebar_w) = span(cx, window, "sidebar-pane").expect("sidebar-pane");
    let right = sidebar_x + sidebar_w;
    for thread in chain.iter().skip(1) {
        hover_row(cx, window, &format!("thread-{thread}"));
        cx.run_until_parked();
        for control in [
            format!("thread-archive-{thread}"),
            format!("thread-actions-{thread}"),
        ] {
            let (x, w) = span(cx, window, &control).expect("row control");
            assert!(
                x + w <= right + 0.5,
                "{control} ends at {} past the sidebar edge {right}",
                x + w
            );
        }
    }
}

#[gpui_kit::test]
fn a_row_menu_acts_on_its_own_row_while_another_thread_is_shown(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let shown_thread = run_shell(cx);
    let row = run_shell(cx);
    click(cx, window, &format!("thread-{shown_thread}"));
    assert_eq!(shown(cx, &kagan), Some(shown_thread.clone()));
    hover_row(cx, window, &format!("thread-{row}"));
    click(cx, window, &format!("thread-actions-{row}"));
    cx.run_until_parked();
    click(cx, window, "thread-pin");
    cx.run_until_parked();
    assert!(crate::tests::kagan::thread_record(cx, &row).pinned);
    assert!(!crate::tests::kagan::thread_record(cx, &shown_thread).pinned);
    assert_eq!(shown(cx, &kagan), Some(shown_thread));
}

#[gpui_kit::test]
fn the_child_chevron_folds_without_selecting_its_parent(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let parent = run_shell(cx);
    let child = run_child(cx, &parent);
    let other = run_shell(cx);
    click(cx, window, &format!("thread-{other}"));
    assert_eq!(shown(cx, &kagan), Some(other.clone()));
    hover_row(cx, window, &format!("thread-{parent}"));
    click(cx, window, &format!("thread-disclose-{parent}"));
    cx.run_until_parked();
    assert!(!present(cx, window, &format!("thread-{child}")));
    assert_eq!(shown(cx, &kagan), Some(other));
}

#[gpui_kit::test]
fn left_and_right_fold_a_thread_with_children(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let parent = run_shell(cx);
    let child = run_child(cx, &parent);
    focus_row(cx, window, &format!("thread-{parent}"));
    press_key(cx, window, "left");
    cx.run_until_parked();
    assert!(!present(cx, window, &format!("thread-{child}")));
    press_key(cx, window, "right");
    cx.run_until_parked();
    assert!(shows(cx, window, &format!("thread-{child}")));
}

fn center_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> gpui_kit::Point<gpui_kit::Pixels> {
    cx.update_window(window, |_, window, _| {
        let bounds = crate::ui_registry::find(window, &ElementId::Name(name.into()))
            .unwrap_or_else(|| panic!("{name}"))
            .bounds;
        point(
            bounds.origin.x + bounds.size.width / 2.,
            bounds.origin.y + bounds.size.height / 2.,
        )
    })
    .unwrap()
}

fn hint_of(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> Option<String> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.into()))
            .and_then(|element| element.hint.as_deref().map(str::to_owned))
    })
    .unwrap()
}

#[gpui_kit::test]
fn the_personal_header_says_what_it_holds_on_hover(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    wait_until(cx, |cx| shows(cx, window, "project-personal"));
    let at = center_of(cx, window, "project-personal");
    hover_to(cx, window, at);
    cx.run_until_parked();
    assert_eq!(
        hint_of(cx, window, "project-personal").as_deref(),
        Some("Threads that are not in a project. Each one gets its own folder.")
    );
}

#[gpui_kit::test]
fn a_theme_row_height_and_body_size_reach_the_sidebar(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let thread = run_shell(cx);
    let row = format!("thread-{}", thread.0);
    wait_until(cx, |cx| present(cx, window, &row));
    let height = |cx: &mut TestAppContext| {
        cx.update_window(window, |_, window, _| {
            f32::from(
                crate::ui_registry::find(window, &ElementId::Name(row.clone().into()))
                    .expect("thread row")
                    .bounds
                    .size
                    .height,
            )
        })
        .unwrap()
    };
    assert!((height(cx) - 28.).abs() < 0.5, "default row {}", height(cx));

    cx.update(|cx| {
        crate::theme::set_look_for_test(
            kg_domain::Look {
                row: 22,
                type_sizes: kg_domain::TypeSizes {
                    body: 12,
                    ..Default::default()
                },
                ..Default::default()
            },
            cx,
        );
    });
    cx.run_until_parked();
    assert!((height(cx) - 22.).abs() < 0.5, "themed row {}", height(cx));
    let body = cx.update(|cx| crate::theme::type_body(cx));
    assert!((body - 12.).abs() < 0.5);
}

#[gpui_kit::test]
fn focus_sidebar_leaves_settings_before_it_takes_focus(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.simulate_keystrokes(window, &chord("OpenSettings"));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), None);
    cx.simulate_keystrokes(window, &chord("FocusSidebar"));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(id));
}
