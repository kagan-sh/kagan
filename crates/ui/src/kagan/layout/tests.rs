use gpui_kit::App;

use super::left_scrim;
use crate::kagan::Kagan;

pub(crate) fn left_scrim_alpha(kagan: &Kagan, cx: &App) -> f32 {
    if kagan.screen.drawn(super::super::screen::Side::Left) {
        left_scrim(cx).a
    } else {
        0.
    }
}
