use gpui_kit::{TestAppContext, WindowOptions};

use crate::presentation::{Presentation, apply, is_offscreen, set_presentation};

fn shown() -> WindowOptions {
    WindowOptions {
        show: true,
        focus: true,
        ..WindowOptions::default()
    }
}

#[gpui_kit::test]
fn offscreen_windows_are_never_shown_or_focused(cx: &mut TestAppContext) {
    cx.update(|cx| set_presentation(cx, Presentation::Offscreen));
    let mut options = shown();
    cx.update(|cx| apply(&mut options, cx));
    assert!(!options.show, "an offscreen window must not be shown");
    assert!(!options.focus, "an offscreen window must not take focus");
}

#[gpui_kit::test]
fn windows_keep_show_and_focus_by_default(cx: &mut TestAppContext) {
    let mut options = shown();
    cx.update(|cx| apply(&mut options, cx));
    assert!(options.show);
    assert!(options.focus);
}

#[gpui_kit::test]
fn the_window_presentation_leaves_options_alone(cx: &mut TestAppContext) {
    cx.update(|cx| set_presentation(cx, Presentation::Window));
    let mut options = shown();
    cx.update(|cx| apply(&mut options, cx));
    assert!(options.show);
    assert!(options.focus);
    assert!(!cx.update(|cx| is_offscreen(cx)));
}

#[gpui_kit::test]
fn no_presentation_set_is_not_offscreen(cx: &mut TestAppContext) {
    assert!(!cx.update(|cx| is_offscreen(cx)));
}
