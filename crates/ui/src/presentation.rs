//! Whether kagan's windows are shown. An offscreen window is never shown or brought forward, so a
//! test run cannot take focus or tile beside the person's windows. `kg ui` still drives it and
//! captures its scene.

use gpui_kit::{App, Global, Window, WindowOptions};

/// How kagan's windows are presented.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Presentation {
    /// A window the person sees. Showing it brings the app forward.
    #[default]
    Window,
    /// A window that is never shown or activated. `kg ui screenshot` still captures it.
    Offscreen,
}

struct Current(Presentation);

impl Global for Current {}

/// Sets how every window this app opens is presented. Call before the first window opens.
pub fn set_presentation(cx: &mut App, presentation: Presentation) {
    cx.set_global(Current(presentation));
}

pub(crate) fn is_offscreen(cx: &App) -> bool {
    cx.try_global::<Current>()
        .is_some_and(|current| current.0 == Presentation::Offscreen)
}

/// Keeps a new window off the screen and out of focus when the windows are offscreen.
pub(crate) fn apply(options: &mut WindowOptions, cx: &App) {
    if is_offscreen(cx) {
        options.show = false;
        options.focus = false;
    }
}

pub(crate) fn activate(cx: &mut App) {
    if !is_offscreen(cx) {
        cx.activate(true);
    }
}

pub(crate) fn activate_window(window: &Window, cx: &App) {
    if !is_offscreen(cx) {
        window.activate_window();
    }
}
