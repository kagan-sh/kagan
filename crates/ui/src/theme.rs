use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::highlighter::{HighlightTheme, HighlightThemeStyle};
use gpui_kit::component::{ActiveTheme as _, Theme, ThemeConfig, ThemeConfigColors, ThemeMode};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, Global, Hsla, SharedString, Styled,
    Subscription, Task, Window, px, rgb,
};
use kg_domain::{
    Appearance, Color, Op, OpError, OpResult, Palette, TerminalColors, ThemeFile, ThemeOp,
    ThemeSettings, Thread, ThreadStatus,
};
use kg_plugins::PluginHost;

use kg_core::{Workspace, WorkspaceEvent};

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
    let regular = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
    let semibold = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");
    let mono = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");
    let mono_bold = include_bytes!("../../../assets/fonts/JetBrainsMono-Bold.ttf");
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

fn palette(file: &ThemeFile, mode: ThemeMode) -> &Palette {
    match mode {
        ThemeMode::Light => &file.light,
        ThemeMode::Dark => &file.dark,
    }
}

struct Swatch {
    bg: u32,
    surface: u32,
    raised: u32,
    ink: u32,
    muted: u32,
    line: u32,
    accent: u32,
    needs: u32,
    failed: u32,
    hover: Option<u32>,
    added: Option<u32>,
    working: u32,
    terminal: Option<TerminalColors>,
}

impl Swatch {
    fn palette(self) -> Palette {
        Palette {
            bg: Color::new(self.bg),
            surface: Color::new(self.surface),
            raised: Color::new(self.raised),
            ink: Color::new(self.ink),
            muted: Color::new(self.muted),
            line: Color::new(self.line),
            accent: Color::new(self.accent),
            primary_foreground: Color::new(self.bg),
            working: Color::new(self.working),
            needs: Color::new(self.needs),
            failed: Color::new(self.failed),
            idle: Color::new(self.muted),
            hover: self.hover.map(Color::new),
            added: self.added.map(Color::new),
            terminal: self.terminal,
        }
    }
}

const FJORD: TerminalColors = TerminalColors {
    bg: Color::new(0x1a_1d24),
    fg: Color::new(0xe5_e9f0),
    cursor: Color::new(0xe5_e9f0),
    black: Color::new(0x3b_4252),
    red: Color::new(0xec_959c),
    green: Color::new(0xa8_c98e),
    yellow: Color::new(0xeb_c874),
    blue: Color::new(0x7f_a6e8),
    magenta: Color::new(0xd0_9ad6),
    cyan: Color::new(0x8f_c7d6),
    white: Color::new(0xd8_dee9),
    bright_black: Color::new(0x98_a0b0),
    bright_red: Color::new(0xf4_aab0),
    bright_green: Color::new(0xbc_d8a6),
    bright_yellow: Color::new(0xf2_d898),
    bright_blue: Color::new(0xa3_bff0),
    bright_magenta: Color::new(0xe0_b6e4),
    bright_cyan: Color::new(0xa8_d6e2),
    bright_white: Color::new(0xec_eff4),
};

fn fjord() -> ThemeFile {
    ThemeFile {
        name: "Ember Fjord".into(),
        light: Swatch {
            bg: 0xee_f1f5,
            surface: 0xe4_e8ee,
            raised: 0xd8_dee7,
            ink: 0x26_2b36,
            muted: 0x53_5c6c,
            line: 0xd3_d9e2,
            accent: 0x9a_4116,
            needs: 0x80_570a,
            failed: 0xa6_2f3c,
            hover: Some(0xdd_e3eb),
            added: Some(0x3c_6a2c),
            working: 0x28_5f73,
            terminal: Some(FJORD),
        }
        .palette(),
        dark: Swatch {
            bg: 0x22_262f,
            surface: 0x1c_1f27,
            raised: 0x2f_3440,
            ink: 0xe5_e9f0,
            muted: 0xa7_afbf,
            line: 0x35_3b48,
            accent: 0xf0_905f,
            needs: 0xeb_c874,
            failed: 0xec_959c,
            hover: Some(0x27_2c36),
            added: Some(0xa8_c98e),
            working: 0x8f_c7d6,
            terminal: Some(FJORD),
        }
        .palette(),
    }
}

fn builtins() -> Vec<CatalogEntry> {
    vec![
        CatalogEntry {
            id: "fjord".into(),
            source: Source::Builtin,
            file: Ok(fjord()),
        },
        builtin("ember", include_bytes!("../../../docs/themes/ember.json")),
        builtin("grey", include_bytes!("../../../docs/themes/grey.json")),
    ]
}

fn builtin(id: &str, bytes: &[u8]) -> CatalogEntry {
    CatalogEntry {
        id: id.into(),
        source: Source::Builtin,
        file: ThemeFile::parse(bytes).map_err(|error| error.to_string()),
    }
}

fn reserved(id: &str) -> bool {
    matches!(id, "fjord" | "ember" | "grey")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Builtin,
    User,
    Plugin(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CatalogEntry {
    pub(crate) id: String,
    pub(crate) source: Source,
    pub(crate) file: Result<ThemeFile, String>,
}

pub(crate) struct Notice(pub(crate) String);

pub(crate) struct ThemeState {
    pub(crate) catalog: Vec<CatalogEntry>,
    pub(crate) settings: ThemeSettings,
    pub(crate) palette: Palette,
    pub(crate) error: Option<String>,
    dir: PathBuf,
    user: Vec<CatalogEntry>,
    plugins: Vec<CatalogEntry>,
    applied: ThemeFile,
    applied_id: Option<String>,
    warned: Option<String>,
    builtins: Vec<CatalogEntry>,
    /// The light or dark mode the operating system uses now
    system: ThemeMode,
    /// The mode painted now: the setting, with `system` resolved
    pub(crate) mode: ThemeMode,
    _watch: Task<()>,
    _changed: Subscription,
    plugins_changed: Option<Subscription>,
    load: Task<()>,
}

impl EventEmitter<Notice> for ThemeState {}

struct ThemeOwner(Entity<ThemeState>);
impl Global for ThemeOwner {}

impl ThemeState {
    pub(crate) fn global(cx: &App) -> Entity<Self> {
        cx.global::<ThemeOwner>().0.clone()
    }

    fn new(dir: PathBuf, cx: &mut Context<Self>) -> Self {
        let file = fjord();
        let settings = ThemeSettings {
            theme: "fjord".into(),
            mode: Appearance::System,
        };
        let system = ThemeMode::Dark;
        let mode = resolve(settings.mode, system);
        let palette = palette(&file, mode).clone();
        apply(&palette, &file.name, mode, cx);
        let folder = dir.clone();
        let watch = cx.spawn(async move |this, cx| {
            if this.update(cx, Self::load_settings).is_err() {
                return;
            }
            loop {
                let folder = folder.clone();
                let user = cx.background_spawn(async move { read_user(&folder) }).await;
                if this
                    .update(cx, |this, cx| {
                        this.user = user;
                        this.refresh(this.settings.clone(), cx);
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
            }
        });
        let changed = cx.subscribe(&Workspace::global(cx), |this, _, event, cx| {
            if matches!(event, WorkspaceEvent::Changed) {
                this.load_settings(cx);
            }
        });
        Self {
            catalog: builtins(),
            builtins: builtins(),
            settings,
            palette,
            error: None,
            dir,
            user: Vec::new(),
            plugins: Vec::new(),
            applied: file,
            applied_id: Some("fjord".into()),
            warned: None,
            system,
            mode,
            _watch: watch,
            _changed: changed,
            plugins_changed: None,
            load: Task::ready(()),
        }
    }

    fn load_settings(&mut self, cx: &mut Context<Self>) {
        let task = Workspace::dispatch(Op::Theme(ThemeOp::Get), cx);
        self.load = cx.spawn(async move |this, cx| {
            if let Ok(OpResult::Theme(settings)) = task.await {
                _ = this.update(cx, |this, cx| this.refresh(settings, cx));
            }
        });
    }

    fn take_plugins(&mut self, host: &Entity<PluginHost>, cx: &mut Context<Self>) {
        let plugins: Vec<CatalogEntry> = host
            .read(cx)
            .themes()
            .map(|theme| CatalogEntry {
                id: theme.id.to_owned(),
                source: Source::Plugin(theme.plugin.to_owned()),
                file: Ok(theme.file.clone()),
            })
            .collect();
        if plugins != self.plugins {
            self.plugins = plugins;
            self.refresh(self.settings.clone(), cx);
        }
    }

    fn refresh(&mut self, settings: ThemeSettings, cx: &mut Context<Self>) {
        let catalog: Vec<CatalogEntry> = self
            .builtins
            .iter()
            .cloned()
            .chain(self.user.iter().cloned())
            .chain(self.plugins.iter().cloned())
            .collect();
        let id = &settings.theme;
        let held = self.applied_id.as_ref() == Some(id);
        let chosen = catalog
            .iter()
            .find(|entry| &entry.id == id)
            .map(|entry| &entry.file);
        // A file that breaks keeps its last good colors; a theme that goes away leaves the default.
        let (file, applied_id, error, broken) = match chosen {
            Some(Ok(file)) => (file.clone(), Some(id.clone()), None, None),
            Some(Err(problem)) if held => (
                self.applied.clone(),
                Some(id.clone()),
                None,
                Some(problem.clone()),
            ),
            Some(Err(problem)) => (fjord(), None, None, Some(problem.clone())),
            None => (
                fjord(),
                None,
                Some(format!(
                    "Theme {id} is missing. Using Kagan until it returns."
                )),
                None,
            ),
        };
        let notice = match &broken {
            Some(problem) if self.warned.as_ref() != Some(id) => Some(format!(
                "Theme {id} has an error, so the window keeps {}: {problem}",
                file.name
            )),
            _ if held && applied_id.is_none() => Some(format!(
                "Theme {id} went away, so the window uses {}.",
                file.name
            )),
            _ => None,
        };
        let warned = broken.map(|_| id.clone());
        if self.catalog == catalog
            && self.settings == settings
            && self.applied == file
            && self.applied_id == applied_id
            && self.error == error
            && self.warned == warned
        {
            return;
        }
        self.applied = file;
        self.applied_id = applied_id;
        self.catalog = catalog;
        self.settings = settings;
        self.error = error;
        self.warned = warned;
        self.paint(cx);
        if let Some(notice) = notice {
            cx.emit(Notice(notice));
        }
    }

    /// Follows the operating system's light or dark mode. Only a `system` setting changes.
    fn set_system(&mut self, system: ThemeMode, cx: &mut Context<Self>) {
        if self.system != system {
            self.system = system;
            self.paint(cx);
        }
    }

    fn paint(&mut self, cx: &mut Context<Self>) {
        self.mode = resolve(self.settings.mode, self.system);
        self.palette = palette(&self.applied, self.mode).clone();
        apply(&self.palette, &self.applied.name, self.mode, cx);
        cx.notify();
    }

    fn copy(&self, id: &str, from: Option<&str>) -> Result<ThemeFile, OpError> {
        if reserved(id) {
            return Err(OpError::Invalid(format!(
                "theme {id} is built in; choose another id"
            )));
        }
        let from = from.unwrap_or(&self.settings.theme);
        let entry = self
            .catalog
            .iter()
            .find(|entry| entry.id == from)
            .ok_or_else(|| OpError::NotFound(format!("theme {from}")))?;
        let mut file = entry
            .file
            .clone()
            .map_err(|error| OpError::Invalid(format!("theme {from} has an error: {error}")))?;
        file.name = format!("{} copy", file.name);
        Ok(file)
    }
}

/// The light or dark mode a setting paints, given the operating system's mode.
fn resolve(mode: Appearance, system: ThemeMode) -> ThemeMode {
    match mode {
        Appearance::Light => ThemeMode::Light,
        Appearance::Dark => ThemeMode::Dark,
        Appearance::System => system,
    }
}

/// Makes a `system` setting follow the window's light or dark mode, now and when it changes.
/// The window keeps the returned subscription.
pub(crate) fn watch_system(window: &mut Window, cx: &mut App) -> Subscription {
    follow_system(ThemeMode::from(window.appearance()), cx);
    window.observe_window_appearance(|window, cx| {
        follow_system(ThemeMode::from(window.appearance()), cx);
    })
}

/// Three colors of a theme in `mode`, for its card on the settings page: the window, the accent,
/// and the text.
pub(crate) fn swatches(file: &ThemeFile, mode: ThemeMode) -> [Hsla; 3] {
    let palette = palette(file, mode);
    [palette.bg, palette.accent, palette.ink].map(token_color)
}

pub(crate) fn follow_system(system: ThemeMode, cx: &mut App) {
    ThemeState::global(cx).update(cx, |state, cx| state.set_system(system, cx));
}

pub(crate) fn follow_plugins(cx: &mut App) {
    let (Some(host), Some(owner)) = (PluginHost::try_global(cx), cx.try_global::<ThemeOwner>())
    else {
        return;
    };
    owner.0.clone().update(cx, |state, cx| {
        if state.plugins_changed.is_none() {
            state.plugins_changed = Some(cx.observe(&host, |state, host, cx| {
                state.take_plugins(&host, cx);
            }));
            state.take_plugins(&host, cx);
        }
    });
}

pub(crate) fn new_theme(
    id: &str,
    from: Option<&str>,
    cx: &mut App,
) -> Task<Result<OpResult, OpError>> {
    let state = ThemeState::global(cx).read(cx);
    let dir = state.dir.clone();
    let path = dir.join(format!("{id}.json"));
    let file = match state.copy(id, from) {
        Ok(file) => file,
        Err(error) => return Task::ready(Err(error)),
    };
    cx.background_spawn(async move {
        let mut text = serde_json::to_vec_pretty(&file)?;
        text.push(b'\n');
        std::fs::create_dir_all(&dir)?;
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::AlreadyExists => {
                    OpError::Invalid(format!("{} already exists", path.display()))
                }
                _ => error.into(),
            })?;
        std::io::Write::write_all(&mut out, &text)?;
        Ok(OpResult::Files(vec![path]))
    })
}

pub(crate) fn copy_id(cx: &App) -> String {
    let state = ThemeState::global(cx).read(cx);
    let chosen = &state.settings.theme;
    let base = chosen.rsplit('/').next().unwrap_or(chosen);
    let first = format!("{base}-copy");
    // One more candidate than the catalog has themes, so one is free.
    std::iter::once(first.clone())
        .chain((2..=state.catalog.len() + 1).map(|n| format!("{first}-{n}")))
        .find(|id| state.catalog.iter().all(|entry| &entry.id != id))
        .unwrap_or(first)
}

pub(crate) fn init_in(dir: PathBuf, cx: &mut App) {
    let state = cx.new(|cx| ThemeState::new(dir, cx));
    cx.set_global(ThemeOwner(state));
}

fn read_user(dir: &Path) -> Vec<CatalogEntry> {
    let mut user = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|id| id.to_str()) else {
                continue;
            };
            if reserved(id) || !kg_domain::valid_theme_id(id) {
                continue;
            }
            let file = ThemeFile::read(&path).map_err(|error| error.to_string());
            user.push(CatalogEntry {
                id: id.into(),
                source: Source::User,
                file,
            });
        }
    }
    user.sort_by(|a, b| a.id.cmp(&b.id));
    user
}

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

fn apply(p: &Palette, name: &str, mode: ThemeMode, cx: &mut App) {
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
        table_active_border, table_even, table_hover, selection, skeleton);
    project!(p.ink => foreground, secondary_foreground, accent_foreground, sidebar_foreground,
        sidebar_accent_foreground, popover_foreground, tab_active_foreground,
        group_box_foreground, group_box_title_foreground, description_list_label_foreground,
        button_foreground, table_head_foreground, table_foot_foreground);
    project!(p.muted => muted_foreground, tab_foreground, scrollbar_thumb);
    project!(p.line => border, input, secondary_active, sidebar_border, title_bar_border,
        status_bar_border, window_border, button_active, table_row_border, chart_grid,
        scrollbar_thumb_hover);
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
    let syntax = match mode {
        ThemeMode::Light => HighlightTheme::default_light(),
        ThemeMode::Dark => HighlightTheme::default_dark(),
    };
    let mut highlight = HighlightThemeStyle::default();
    highlight.status.clone_from(&syntax.style.status);
    highlight.syntax.clone_from(&syntax.style.syntax);
    let mut config = ThemeConfig::default();
    config.name = name.to_owned().into();
    config.mode = mode;
    config.font_size = Some(REM_BASE);
    config.mono_font_size = Some(TYPE_MONO);
    config.font_family = Some(SharedString::from("Inter"));
    config.mono_font_family = Some(SharedString::from("JetBrains Mono"));
    config.radius = Some(RADIUS);
    config.radius_lg = Some(RADIUS_LG);
    config.shadow = Some(false);
    config.highlight = Some(highlight);
    config.colors = colors;
    let config = Rc::new(config);
    Theme::update(cx, |theme| theme.apply_config(&config));
}

#[cfg(test)]
mod tests;
