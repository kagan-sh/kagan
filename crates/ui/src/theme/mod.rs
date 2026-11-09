use gpui_kit::{App, Hsla, Styled, px, rgb};
use kg_domain::Color;

mod terminal;

pub(crate) const MARK_RING: f32 = 8.;
pub(crate) const MARK_FAILED: f32 = 8.;
pub(crate) const MARK_ENDED: f32 = 8.;
pub(crate) const MARK_IDLE: f32 = 4.;
const REM_BASE: f32 = 16.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpaceStep {
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
}

pub(crate) use catalog::look;

pub(crate) fn space(step: SpaceStep, cx: &App) -> f32 {
    let space = look(cx).space;
    f32::from(match step {
        SpaceStep::Xs => space.xs,
        SpaceStep::Sm => space.sm,
        SpaceStep::Md => space.md,
        SpaceStep::Lg => space.lg,
        SpaceStep::Xl => space.xl,
    })
}

pub(crate) fn radius_xs(cx: &App) -> f32 {
    f32::from(look(cx).radius.xs)
}

pub(crate) fn radius_sm(cx: &App) -> f32 {
    f32::from(look(cx).radius.sm)
}

pub(crate) fn radius_md(cx: &App) -> f32 {
    f32::from(look(cx).radius.md)
}

pub(crate) fn radius_lg(cx: &App) -> f32 {
    f32::from(look(cx).radius.lg)
}

pub(crate) fn type_title(cx: &App) -> f32 {
    f32::from(look(cx).type_sizes.title)
}

pub(crate) fn type_body(cx: &App) -> f32 {
    f32::from(look(cx).type_sizes.body)
}

pub(crate) fn type_small(cx: &App) -> f32 {
    f32::from(look(cx).type_sizes.small)
}

pub(crate) fn type_caption(cx: &App) -> f32 {
    f32::from(look(cx).type_sizes.caption)
}

pub(crate) fn type_mono(cx: &App) -> f32 {
    f32::from(look(cx).type_sizes.mono)
}

pub(crate) fn row_height(cx: &App) -> f32 {
    f32::from(look(cx).row)
}

pub(crate) fn control_height(cx: &App) -> f32 {
    f32::from(look(cx).control)
}

pub(crate) trait TypeScale: Styled {
    fn type_title(self, cx: &App) -> Self {
        self.text_size(px(type_title(cx)))
    }

    fn type_small(self, cx: &App) -> Self {
        self.text_size(px(type_small(cx)))
    }

    fn type_caption(self, cx: &App) -> Self {
        self.text_size(px(type_caption(cx)))
    }

    fn type_body(self, cx: &App) -> Self {
        self.text_size(px(type_body(cx)))
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
    if let Err(error) = cx.text_system().add_fonts(vec![
        Cow::Borrowed(regular.as_slice()),
        Cow::Borrowed(semibold.as_slice()),
        Cow::Borrowed(mono.as_slice()),
        Cow::Borrowed(mono_bold.as_slice()),
    ]) {
        eprintln!("kg: the bundled fonts could not load: {error}");
    }
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
#[cfg(test)]
pub(crate) use catalog::set_look_for_test;
pub(crate) use catalog::{
    CatalogEntry, Notice, Source, ThemeState, copy_id, follow_plugins, init_in, new_theme,
    swatches, watch_system,
};
#[cfg(test)]
use colors::config;
pub(crate) use colors::{Tone, added, brand_color, cursor_color, terminal_color};
#[cfg(test)]
use gpui_kit::component::ThemeMode;
#[cfg(test)]
use kg_domain::ThemeFile;
#[cfg(test)]
use palette::{FJORD, builtins, fjord, palette};
#[cfg(test)]
mod tests;
