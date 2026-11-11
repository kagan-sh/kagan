//! The built-in Browser tab belongs to the thread that drives it, and its page leaves the screen
//! in the same step that takes its tab or panel away.

use gpui_kit::{AnyWindowHandle, AppContext as _, Bounds, Entity, Pixels, TestAppContext, px};

use crate::kagan::Kagan;
use crate::tests::kagan::{click, open, run_shell, shows};

fn driven_browser(cx: &mut TestAppContext, window: AnyWindowHandle, kagan: &Entity<Kagan>) {
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.update(|cx| {
        kagan.update(cx, |kagan, _| {
            kagan.drive_browser_for_test(id.clone(), "http://localhost:1/");
        });
    });
    click(cx, window, "right-tab-add");
    click(cx, window, "right-add-browser");
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::render_frame(window, cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn another_threads_browser_is_not_shown_in_this_one(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let driver = run_shell(cx);
    let here = run_shell(cx);
    click(cx, window, &format!("thread-{}", here.0));
    cx.update(|cx| {
        kagan.update(cx, |kagan, _| {
            kagan.drive_browser_for_test(driver.clone(), "http://localhost:1/");
        });
    });
    click(cx, window, "right-tab-add");
    click(cx, window, "right-add-browser");
    assert!(!shows(cx, window, "browser-stop-agent"));
    assert!(shows(cx, window, "browser-held"));
}

#[gpui_kit::test]
fn taking_the_right_panel_away_hides_the_page_in_that_step(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    driven_browser(cx, window, &kagan);
    assert!(kagan.read_with(cx, |kagan, _| kagan.browser_shown_for_test()));
    let shown = cx.update(|cx| {
        kagan.update(cx, |kagan, cx| {
            kagan.close_right_panel_for_test(cx);
            kagan.browser_shown_for_test()
        })
    });
    assert!(!shown);
}

#[gpui_kit::test]
fn leaving_the_browser_tab_hides_the_page_in_that_step(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    driven_browser(cx, window, &kagan);
    let shown = cx.update(|cx| {
        kagan.update(cx, |kagan, cx| {
            kagan.select_changes_tab_for_test(cx);
            kagan.browser_shown_for_test()
        })
    });
    assert!(!shown);
}

fn bounds_of(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> Bounds<Pixels> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &gpui_kit::ElementId::Name(name.to_owned().into()))
            .map(|element| element.bounds)
    })
    .unwrap()
    .unwrap_or_else(|| panic!("{name} is not in the frame"))
}

fn left_edge(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> f32 {
    f32::from(bounds_of(cx, window, name).origin.x)
}

// The divider's band reaches 4 px into the panel it edges. The page must start past that.
#[gpui_kit::test]
fn the_page_starts_clear_of_the_divider_band(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    driven_browser(cx, window, &kagan);
    let panel = left_edge(cx, window, "right-sidebar");
    let page = left_edge(cx, window, "browser-page");
    assert!(
        page - panel >= 4.,
        "the page starts {} px into the panel",
        page - panel
    );
}

// The toolbar of a driven tab keeps its Stop button inside the narrowest right panel.
#[gpui_kit::test]
fn the_stop_button_stays_inside_the_narrowest_panel(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    driven_browser(cx, window, &kagan);
    let narrow = px(kg_domain::prefs::RIGHT_WIDTH_RANGE.0);
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.resize_right(narrow, window, cx));
    })
    .unwrap();
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::render_frame(window, cx);
    })
    .unwrap();
    let panel = bounds_of(cx, window, "right-sidebar");
    let stop = bounds_of(cx, window, "browser-stop-agent");
    let panel_right = f32::from(panel.origin.x + panel.size.width);
    let stop_right = f32::from(stop.origin.x + stop.size.width);
    assert!(
        stop_right <= panel_right,
        "Stop agent ends {} px past the panel edge",
        stop_right - panel_right
    );
}
