use gpui_kit::{App, Window};
use kg_domain::Slot;

use crate::kagan::Kagan;

pub(crate) fn filled(kagan: &Kagan, slot: Slot) -> bool {
    kagan.replaced.mounted.contains_key(&slot)
}

pub(crate) fn holds_focus(kagan: &Kagan, slot: Slot, window: &Window, cx: &App) -> bool {
    kagan
        .replaced
        .mounted
        .get(&slot)
        .is_some_and(|mounted| mounted.focus.contains_focused(window, cx))
}

/// Whether the keyboard is on something the window draws, so typing reaches it.
pub(crate) fn window_holds_keys(kagan: &Kagan, window: &Window, cx: &App) -> bool {
    kagan.focus.contains_focused(window, cx)
}
