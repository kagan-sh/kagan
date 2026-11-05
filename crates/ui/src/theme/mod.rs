use gpui_kit::{App, Hsla, Styled, px, rgb};
use kg_domain::Color;

mod terminal;

pub(crate) const MARK_RING: f32 = 8.;
pub(crate) const MARK_FAILED: f32 = 8.;
pub(crate) const MARK_ENDED: f32 = 8.;
pub(crate) const MARK_IDLE: f32 = 4.;
const REM_BASE: f32 = 16.;
pub(crate) const TYPE_TITLE: f32 = 15.;
pub(crate) const TYPE_BODY: f32 = 13.;
pub(crate) const TYPE_SMALL: f32 = 12.;
pub(crate) const TYPE_CAPTION: f32 = 11.;
pub(crate) const TYPE_MONO: f32 = 12.;
const RADIUS: usize = 8;
const RADIUS_LG: usize = 12;

pub(crate) trait TypeScale: Styled {
    fn type_title(self) -> Self {
        self.text_size(px(TYPE_TITLE))
    }

    fn type_small(self) -> Self {
        self.text_size(px(TYPE_SMALL))
    }

    fn type_caption(self) -> Self {
        self.text_size(px(TYPE_CAPTION))
    }

    fn type_body(self) -> Self {
        self.text_size(px(TYPE_BODY))
    }
}

impl<E: Styled> TypeScale for E {}

pub(crate) fn load_fonts(cx: &mut App) {
    use std::borrow::Cow;
    use std::sync::atomic::{AtomicBool, Ordering};
    static LOADED: AtomicBool = AtomicBool::new(false);
    if LOADED.swap(true, Ordering::SeqCst) {
        return;
    }
    let regular = include_bytes!("../../../../assets/fonts/Inter-Regular.ttf");
    let semibold = include_bytes!("../../../../assets/fonts/Inter-SemiBold.ttf");
    let mono = include_bytes!("../../../../assets/fonts/JetBrainsMono-Regular.ttf");
    let mono_bold = include_bytes!("../../../../assets/fonts/JetBrainsMono-Bold.ttf");
    let _ = cx.text_system().add_fonts(vec![
        Cow::Borrowed(regular.as_slice()),
        Cow::Borrowed(semibold.as_slice()),
        Cow::Borrowed(mono.as_slice()),
        Cow::Borrowed(mono_bold.as_slice()),
    ]);
}

pub(crate) fn token_color(color: Color) -> Hsla {
    rgb(color.rgb()).into()
}

mod catalog;
mod colors;
mod palette;

#[cfg(test)]
pub(crate) use catalog::follow_system;
#[cfg(test)]
use catalog::read_user;
pub(crate) use catalog::{
    CatalogEntry, Notice, Source, ThemeState, copy_id, follow_plugins, init_in, new_theme,
    swatches, watch_system,
};
pub(crate) use colors::{Tone, added, brand_color, cursor_color, terminal_color};
#[cfg(test)]
use gpui_kit::component::ThemeMode;
#[cfg(test)]
use kg_domain::ThemeFile;
#[cfg(test)]
use palette::{FJORD, builtins, fjord, palette};
#[cfg(test)]
mod tests;
