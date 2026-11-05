use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::highlighter::{HighlightTheme, HighlightThemeStyle};
use gpui_kit::component::{ActiveTheme as _, Theme, ThemeConfig, ThemeConfigColors, ThemeMode};
use gpui_kit::{App, Hsla, SharedString, rgb};
use kg_domain::{Color, ThemeFile, Thread, ThreadStatus};

use super::ThemeState;
use super::palette::palette;
use super::terminal;
use crate::theme::{MARK_ENDED, MARK_FAILED, MARK_IDLE, MARK_RING, REM_BASE, token_color};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    Busy,
    Needs,
    Failed,
    Done,
    Idle,
    Ended,
}

impl Tone {
    pub(crate) fn of(thread: &Thread, busy: bool, ended: bool, asked: bool) -> Self {
        let needs = thread.unread() || asked;
        match thread.status {
            ThreadStatus::Stopping => Self::Busy,
            ThreadStatus::Starting | ThreadStatus::Active if busy => Self::Busy,
            ThreadStatus::Error => Self::Failed,
            ThreadStatus::Starting
            | ThreadStatus::Active
            | ThreadStatus::Pending
            | ThreadStatus::Idle
                if needs =>
            {
                Self::Needs
            }
            ThreadStatus::Starting
            | ThreadStatus::Active
            | ThreadStatus::Pending
            | ThreadStatus::Idle
                if thread.mark == kg_domain::ThreadMark::Done =>
            {
                Self::Done
            }
            ThreadStatus::Idle if ended => Self::Ended,
            ThreadStatus::Starting
            | ThreadStatus::Active
            | ThreadStatus::Pending
            | ThreadStatus::Idle => Self::Idle,
        }
    }
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Needs => "needs you",
            Self::Failed => "failed",
            Self::Done => "done",
            Self::Idle => "idle",
            Self::Ended => "ended",
        }
    }
    pub(crate) fn color(self, cx: &App) -> Hsla {
        let state = ThemeState::global(cx);
        let p = &state.read(cx).palette;
        token_color(match self {
            Self::Busy | Self::Done => p.working,
            Self::Needs => p.needs,
            Self::Failed => p.failed,
            Self::Idle | Self::Ended => p.idle,
        })
    }

    pub(crate) fn mark(self, cx: &App) -> gpui_kit::AnyElement {
        use gpui_kit::component::Sizable as _;
        use gpui_kit::component::spinner::Spinner;
        use gpui_kit::{IntoElement, Styled as _, div, px};
        let color = self.color(cx);
        match self {
            Self::Busy => Spinner::new()
                .xsmall()
                .color(cx.theme().foreground)
                .into_any_element(),
            Self::Needs => div()
                .size(px(MARK_RING))
                .rounded_full()
                .border_1()
                .border_color(color)
                .into_any_element(),
            Self::Failed => div()
                .size(px(MARK_FAILED))
                .rounded_full()
                .bg(color)
                .into_any_element(),
            Self::Done => div().size(px(MARK_ENDED)).bg(color).into_any_element(),
            Self::Idle => div()
                .size(px(MARK_IDLE))
                .rounded_full()
                .bg(color)
                .into_any_element(),
            Self::Ended => div()
                .size(px(MARK_ENDED))
                .border_1()
                .border_color(color)
                .into_any_element(),
        }
    }
}

/// The `added` token: added lines and added counts. A theme without it uses `working`.
pub(crate) fn added(cx: &App) -> Hsla {
    let state = ThemeState::global(cx);
    token_color(state.read(cx).palette.added())
}

/// A program's own colors are data; its defaults follow the theme.
pub(crate) fn terminal_color(color: kg_core::Color, cx: &App) -> Hsla {
    let state = ThemeState::global(cx);
    let terminal = state.read(cx).palette.terminal();
    match color {
        kg_core::Color::Foreground => token_color(terminal.fg),
        kg_core::Color::Background => token_color(terminal.bg),
        kg_core::Color::Ansi(name) => token_color(terminal::ansi(&terminal, name)),
        kg_core::Color::Rgb(r, g, b) => rgb(u32::from_be_bytes([0, r, g, b])).into(),
    }
}

pub(crate) fn brand_color(cx: &App) -> Hsla {
    let state = ThemeState::global(cx);
    token_color(state.read(cx).palette.accent)
}

pub(crate) fn cursor_color(cx: &App) -> Hsla {
    let state = ThemeState::global(cx);
    token_color(state.read(cx).palette.terminal().cursor)
}

pub(super) fn highlight(file: &ThemeFile, mode: ThemeMode) -> Arc<HighlightTheme> {
    let chosen = file.highlight.as_ref().and_then(|highlight| match mode {
        ThemeMode::Light => highlight.light.as_ref(),
        ThemeMode::Dark => highlight.dark.as_ref(),
    });
    chosen
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .map_or_else(
            || match mode {
                ThemeMode::Light => HighlightTheme::default_light(),
                ThemeMode::Dark => HighlightTheme::default_dark(),
            },
            Arc::new,
        )
}

pub(super) fn apply(file: &ThemeFile, mode: ThemeMode, cx: &mut App) {
    let config = Rc::new(config(file, mode));
    Theme::update(cx, |theme| theme.apply_config(&config));
}

pub(super) fn config(file: &ThemeFile, mode: ThemeMode) -> ThemeConfig {
    let p = palette(file, mode);
    let hex = |c: Color| Some(SharedString::from(c.hex()));
    let mut colors = ThemeConfigColors::default();
    macro_rules! project {
        ($color:expr => $($field:ident),+ $(,)?) => { $(colors.$field = hex($color);)+ };
    }
    project!(p.bg => background, list, table);
    project!(p.surface => muted, sidebar, popover, tab, tab_active, tab_bar, title_bar,
        status_bar, accordion, group_box, description_list_label, list_head, table_head,
        table_foot, scrollbar, tab_bar_segmented);
    project!(p.raised => secondary, secondary_hover, accent, list_active, list_active_border,
        list_even, sidebar_accent, button, button_hover, table_active,
        table_active_border, table_even, table_hover, skeleton);
    project!(p.selection() => selection);
    project!(p.ink => foreground, secondary_foreground, accent_foreground, sidebar_foreground,
        sidebar_accent_foreground, popover_foreground, tab_active_foreground,
        group_box_foreground, group_box_title_foreground, description_list_label_foreground,
        button_foreground, table_head_foreground, table_foot_foreground);
    project!(p.muted => muted_foreground, tab_foreground, scrollbar_thumb);
    project!(p.line => border, secondary_active, sidebar_border, title_bar_border,
        status_bar_border, window_border, button_active, table_row_border, chart_grid,
        scrollbar_thumb_hover);
    project!(p.input() => input);
    project!(p.hover.unwrap_or(p.raised) => list_hover);
    project!(p.muted => switch);
    project!(p.bg => switch_thumb);
    project!(p.ink => primary, primary_hover, primary_active);
    project!(p.bg => primary_foreground);
    project!(p.ink => ring, caret, link, link_hover, link_active, drag_border, drop_target,
        progress_bar, sidebar_primary, slider_bar, slider_thumb, info, info_hover, info_active);
    project!(p.primary_foreground => sidebar_primary_foreground, info_foreground);
    project!(p.needs => warning, warning_hover, warning_active);
    project!(p.failed => danger, danger_hover, danger_active, chart_bearish);
    project!(p.working => success, success_hover, success_active, chart_bullish);
    project!(p.bg => warning_foreground, danger_foreground, success_foreground);
    colors.overlay = Some(format!("{}cc", p.bg.hex()).into());
    let syntax = highlight(file, mode);
    let mut highlight = HighlightThemeStyle::default();
    highlight.status.clone_from(&syntax.style.status);
    highlight.syntax.clone_from(&syntax.style.syntax);
    let mut config = ThemeConfig::default();
    config.name = file.name.clone().into();
    config.mode = mode;
    config.font_size = Some(REM_BASE);
    config.mono_font_size = Some(f32::from(file.look.type_sizes.mono));
    config.font_family = Some(SharedString::from("Inter"));
    config.mono_font_family = Some(SharedString::from("JetBrains Mono"));
    config.radius = Some(usize::from(file.look.radius.md));
    config.radius_lg = Some(usize::from(file.look.radius.lg));
    config.shadow = Some(false);
    config.highlight = Some(highlight);
    config.colors = colors;
    config
}
