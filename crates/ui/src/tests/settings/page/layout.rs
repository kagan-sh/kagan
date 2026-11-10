use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ElementId, InputEvent as _, Modifiers, MouseMoveEvent,
    Pixels, Point, ScrollDelta, ScrollEvent, TestAppContext, TouchPhase, point, px, size,
};
use kg_domain::{Op, UiOp};
use serde_json::json;

use super::{run_op, saved};
use crate::tests::kagan::{
    click, drag_to, open, press as press_at, release, shows, value_of, wait_until,
};
use crate::tests::sidebar::layout::span;

#[gpui_kit::test]
fn settings_saves_terminal_size_and_notifications(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-appearance");
    click(cx, window, "terminal-size-bigger");
    wait_until(cx, |cx| saved(cx, "terminal-font-size") == json!(13));
    click(cx, window, "settings-section-general");
    click(cx, window, "notify");
    wait_until(cx, |cx| saved(cx, "notify") == json!(false));
    assert_eq!(value_of(cx, window, "notify").as_deref(), Some("off"));
}

#[gpui_kit::test]
fn clicking_a_setting_out_of_view_scrolls_to_it(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(1100.), px(560.)));
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-installed");
    assert!(
        !shows(cx, window, "slot-review-kagan"),
        "the target starts below the window"
    );
    run_op(
        cx,
        Op::Ui(UiOp::Click {
            element_id: "slot-review-kagan".into(),
            within: None,
            window: 0,
        }),
    );
    cx.run_until_parked();
    assert!(shows(cx, window, "slot-review-kagan"));
}

#[gpui_kit::test]
fn scrolling_an_element_out_of_view_first_brings_it_into_view(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(1100.), px(560.)));
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-installed");
    assert!(
        !shows(cx, window, "slot-review-kagan"),
        "the target starts below the window"
    );
    run_op(
        cx,
        Op::Ui(UiOp::Scroll {
            element_id: "slot-review-kagan".into(),
            within: None,
            dy: 0.,
            window: 0,
        }),
    );
    assert!(shows(cx, window, "slot-review-kagan"));
}

#[gpui_kit::test]
fn the_settings_scrollbar_hugs_the_window_edge(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-keyboard");
    for width in [1100., 1500.] {
        cx.simulate_window_resize(window, size(px(width), px(720.)));
        cx.run_until_parked();
        let (center_x, center_width) = span(cx, window, "center").unwrap();
        let edge = center_x + center_width;
        let (page_x, page_width) = span(cx, window, "settings-page").unwrap();
        let (column_x, column) = span(cx, window, "settings-group-shortcuts").unwrap();
        let left = column_x - page_x;
        let right = page_x + page_width - column_x - column;
        assert!(
            (left - right).abs() < 1.,
            "the column is not centered at {width}"
        );
        // gpui-kit draws the bar along the right side of the element that scrolls.
        let bar = point(px(edge) - Scrollbar::width() / 2., px(300.));
        wheel(cx, window, bar, 10_000.);
        let top = top_of(cx, window, "setting-row-NewThread");
        wheel(cx, window, bar, -50.);
        assert!(
            (top_of(cx, window, "setting-row-NewThread") - (top - 50.)).abs() < 0.5,
            "the page does not scroll under its bar at {width}"
        );
    }
    click(cx, window, "settings-section-installed");
    wheel(cx, window, point(px(700.), px(300.)), 10_000.);
    run_op(
        cx,
        Op::Ui(UiOp::Click {
            element_id: "plugin-install-open".into(),
            within: None,
            window: 0,
        }),
    );
    assert!(shows(cx, window, "plugin-source"));
}

#[gpui_kit::test]
fn the_settings_thumb_follows_the_page_down(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-keyboard");
    cx.simulate_window_resize(window, size(px(1100.), px(720.)));
    cx.run_until_parked();
    let (center_x, center_width) = span(cx, window, "center").unwrap();
    let x = px(center_x + center_width) - Scrollbar::width() / 2.;
    let bottom = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name("settings-page".into()))
                .map(|page| page.bounds.bottom())
        })
        .unwrap()
        .unwrap();
    wheel(cx, window, point(x, px(300.)), -10_000.);
    let top = top_of(cx, window, "setting-row-NewThread");
    let grab = point(x, bottom - px(10.));
    let drop = grab - point(px(0.), px(100.));
    press_at(cx, window, grab, 1, Modifiers::default());
    drag_to(cx, window, drop);
    release(cx, window, drop, 1, Modifiers::default());
    cx.run_until_parked();
    assert!(
        top_of(cx, window, "setting-row-NewThread") > top + 100.,
        "the thumb is not at the bottom of the bar once the page is scrolled down"
    );
}

pub(in crate::tests) fn top_of(cx: &mut TestAppContext, window: AnyWindowHandle, id: &str) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(id.into()))
            .map(|element| f32::from(element.bounds.origin.y))
    })
    .unwrap()
    .unwrap()
}

pub(in crate::tests) fn wheel(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    at: Point<Pixels>,
    dy: f32,
) {
    let event = ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        source: gpui_kit::ScrollSource::Pointer,
    };
    let hover = MouseMoveEvent {
        position: at,
        pressed_button: None,
        modifiers: Modifiers::default(),
    };
    cx.update_window(window, |_, window, cx| {
        window.dispatch_event(hover.to_platform_input(), cx);
        crate::ui_dispatch::render_frame(window, cx);
        window.dispatch_event(event.to_platform_input(), cx);
        crate::ui_dispatch::render_frame(window, cx);
    })
    .unwrap();
    cx.run_until_parked();
}
