use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::SharedString;
use gpui_kit::component::ThemeMode;
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, Global, Hsla, Subscription, Task, Window,
    px,
};
use kg_domain::{
    Appearance, Look, Op, OpError, OpResult, Palette, PreferenceOp, Space, ThemeFile, ThemeOp,
    ThemeSettings, prefs,
};
use kg_plugins::PluginHost;

use kg_core::{Workspace, WorkspaceEvent};

use super::colors::{Families, apply};
use super::palette::{builtins, fjord, palette, reserved};
use crate::theme::token_color;
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
    pub(crate) density: Density,
    /// Families that can set Latin text: symbol and emoji faces are left out.
    pub(crate) installed_fonts: Vec<String>,
    /// The text families whose letters all share one width.
    pub(crate) monospace_fonts: Vec<String>,
    font_prefs: FontPrefs,
    // The card's id and theme while a card previews; the saved theme is `applied`.
    preview: Option<(String, ThemeFile)>,
    focus_preview: bool,
    pub(crate) palette: Palette,
    pub(crate) error: Option<String>,
    dir: PathBuf,
    user: Vec<CatalogEntry>,
    plugins: Vec<CatalogEntry>,
    applied: ThemeFile,
    applied_id: Option<String>,
    warned: Option<String>,
    builtins: Vec<CatalogEntry>,
    system: ThemeMode,
    pub(crate) mode: ThemeMode,
    _watch: Task<()>,
    _changed: Subscription,
    plugins_changed: Option<Subscription>,
    load: Task<()>,
    fonts_load: Task<()>,
}

impl EventEmitter<Notice> for ThemeState {}

struct ThemeOwner(Entity<ThemeState>);
impl Global for ThemeOwner {}

/// How close the rows and the spacing sit. The theme file sets the default; this scales it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Density {
    Compact,
    #[default]
    Default,
    Roomy,
}

impl Density {
    pub(crate) fn parse(text: &str) -> Self {
        match text {
            "compact" => Self::Compact,
            "roomy" => Self::Roomy,
            _ => Self::Default,
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Default => "default",
            Self::Roomy => "roomy",
        }
    }

    fn scale(self, mut look: Look) -> Look {
        let percent = match self {
            Self::Compact => 75,
            Self::Default => return look,
            Self::Roomy => 125,
        };
        let by = |value: u16| u16::try_from(u32::from(value) * percent / 100).unwrap_or(u16::MAX);
        look.space = Space {
            xs: by(look.space.xs),
            sm: by(look.space.sm),
            md: by(look.space.md),
            lg: by(look.space.lg),
            xl: by(look.space.xl),
            xxl: by(look.space.xxl),
        };
        look.row = by(look.row);
        look.control = by(look.control);
        look
    }
}

pub(crate) fn look(cx: &App) -> Look {
    cx.try_global::<ThemeOwner>()
        .map_or_else(Look::default, |owner| owner.0.read(cx).look())
}

#[cfg(test)]
pub(crate) fn set_look_for_test(look: Look, cx: &mut App) {
    ThemeState::global(cx).update(cx, |state, cx| {
        state.applied.look = look;
        state.paint(cx);
    });
}

impl ThemeState {
    pub(crate) fn global(cx: &App) -> Entity<Self> {
        cx.global::<ThemeOwner>().0.clone()
    }

    fn new(dir: PathBuf, cx: &mut Context<Self>) -> Self {
        let file = fjord();
        let settings = ThemeSettings::default();
        let system = ThemeMode::Dark;
        let mode = resolve(settings.mode, system);
        let palette = palette(&file, mode).clone();
        apply(&file, mode, &Families::default(), cx);
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
        let mut state = Self {
            catalog: builtins(),
            builtins: builtins(),
            density: Density::default(),
            installed_fonts: Vec::new(),
            monospace_fonts: Vec::new(),
            font_prefs: FontPrefs::default(),
            preview: None,
            focus_preview: false,
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
            fonts_load: Task::ready(()),
        };
        let text = cx.text_system().clone();
        state.fonts_load = cx.spawn(async move |this, cx| {
            let faces = cx
                .background_spawn(async move {
                    text.all_font_names()
                        .into_iter()
                        .filter(|name| !name.starts_with('.'))
                        .filter_map(|name| {
                            let id = text.resolve_font(&gpui_kit::font(name.clone()));
                            let kind = face_kind(|ch| {
                                text.advance(id, px(16.), ch)
                                    .ok()
                                    .map(|size| size.width.as_f32())
                            })?;
                            Some((name, kind))
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            _ = this.update(cx, |this, cx| {
                // A list a test put in place is kept; the platform's list fills an empty one.
                if this.installed_fonts.is_empty() {
                    this.installed_fonts =
                        sorted_families(faces.iter().map(|(name, _)| name.clone()).collect());
                    this.monospace_fonts = sorted_families(
                        faces
                            .iter()
                            .filter(|(_, monospace)| *monospace)
                            .map(|(name, _)| name.clone())
                            .collect(),
                    );
                    this.paint(cx);
                }
            });
        });
        state
    }

    // A font the person picked wins over the theme's, when the system has it; otherwise the default.
    fn families(&self) -> Families {
        let pick_mono = |chosen: &str, fallback: &str| {
            if self.monospace_fonts.iter().any(|font| font == chosen) {
                SharedString::from(chosen.to_owned())
            } else {
                SharedString::from(fallback.to_owned())
            }
        };
        let pick = |chosen: &str, fallback: &str| {
            if self.installed_fonts.iter().any(|font| font == chosen) {
                SharedString::from(chosen.to_owned())
            } else {
                SharedString::from(fallback.to_owned())
            }
        };
        let default = Families::default();
        Families {
            ui: pick(&self.font_prefs.ui, &default.ui),
            mono: pick_mono(&self.font_prefs.terminal, &default.mono),
        }
    }

    fn load_settings(&mut self, cx: &mut Context<Self>) {
        let task = Workspace::dispatch(Op::Theme(ThemeOp::Get), cx);
        let [ui, terminal] = [prefs::FONT_UI, prefs::FONT_TERMINAL].map(|key| {
            Workspace::dispatch(Op::Preference(PreferenceOp::Get { key: key.into() }), cx)
        });
        let density = Workspace::dispatch(
            Op::Preference(PreferenceOp::Get {
                key: prefs::DENSITY.into(),
            }),
            cx,
        );
        self.load = cx.spawn(async move |this, cx| {
            let density = match density.await {
                Ok(OpResult::Setting(value)) => Density::parse(value.as_str().unwrap_or_default()),
                _ => Density::default(),
            };
            let fonts = FontPrefs {
                ui: text_of(ui.await),
                terminal: text_of(terminal.await),
            };
            if let Ok(OpResult::Theme(settings)) = task.await {
                _ = this.update(cx, |this, cx| {
                    let fonts_changed = this.font_prefs != fonts;
                    if this.density != density || fonts_changed {
                        this.density = density;
                        this.font_prefs = fonts;
                        this.paint(cx);
                    }
                    this.refresh(settings, cx);
                });
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

    fn set_system(&mut self, system: ThemeMode, cx: &mut Context<Self>) {
        if self.system != system {
            self.system = system;
            self.paint(cx);
        }
    }

    fn paint(&mut self, cx: &mut Context<Self>) {
        self.mode = resolve(self.settings.mode, self.system);
        // A preview is what the window shows; the store refresh only changes `applied`.
        let shown = self
            .preview
            .as_ref()
            .map_or_else(|| self.applied.clone(), |(_, file)| file.clone());
        self.palette = palette(&shown, self.mode).clone();
        apply(&shown, self.mode, &self.families(), cx);
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn set_installed_fonts_for_test(&mut self, fonts: Vec<String>) {
        self.monospace_fonts = fonts.clone();
        self.installed_fonts = fonts;
    }

    // Shows a card's theme without saving it.
    pub(crate) fn preview(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.preview.as_ref().is_some_and(|(shown, _)| shown == id) {
            return;
        }
        let Some(Ok(file)) = self
            .catalog
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.file.clone())
        else {
            return;
        };
        self.preview = Some((id.to_owned(), file));
        self.paint(cx);
    }

    // Puts the saved theme back on screen.
    pub(crate) fn end_preview(&mut self, cx: &mut Context<Self>) {
        if self.preview.take().is_some() {
            self.paint(cx);
        }
    }

    // A click keeps the previewed theme on screen; the save that follows makes it the saved one.
    pub(crate) fn commit_preview(&mut self, cx: &mut Context<Self>) {
        if let Some((id, file)) = self.preview.take() {
            self.applied = file;
            self.applied_id = Some(id);
            self.paint(cx);
        }
    }

    // A card with keyboard focus previews its theme; losing focus puts the saved one back.
    pub(crate) fn follow_focus(&mut self, focused: Option<&str>, cx: &mut Context<Self>) {
        match focused {
            Some(id) => self.preview(id, cx),
            None if self.focus_preview => self.end_preview(cx),
            None => {}
        }
        self.focus_preview = focused.is_some();
    }

    pub(crate) fn look(&self) -> Look {
        self.density.scale(self.applied.look)
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
/// The font names the person saved. An empty name is the theme's own font.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FontPrefs {
    pub(crate) ui: String,
    pub(crate) terminal: String,
}

fn text_of(result: Result<OpResult, OpError>) -> String {
    match result {
        Ok(OpResult::Setting(value)) => value.as_str().unwrap_or_default().to_owned(),
        _ => String::new(),
    }
}

// The families the system has, without the hidden system faces (their names start with a dot).
// Read off the main thread: the platform walks every font, which takes tens of milliseconds.
/// What a face can show, from its glyph advances: `None` when it lacks a Latin letter (a symbol,
/// braille or emoji face), otherwise whether its letters share one width.
fn face_kind(advance: impl Fn(char) -> Option<f32>) -> Option<bool> {
    for ch in "aeinostAE".chars() {
        advance(ch)?;
    }
    let widths = "ilmwW"
        .chars()
        .map(&advance)
        .collect::<Option<Vec<f32>>>()?;
    Some(widths.iter().all(|width| (width - widths[0]).abs() < 0.5))
}

fn sorted_families(all: Vec<String>) -> Vec<String> {
    let mut names: Vec<String> = all
        .into_iter()
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    names.dedup();
    names
}

fn resolve(mode: Appearance, system: ThemeMode) -> ThemeMode {
    match mode {
        Appearance::Light => ThemeMode::Light,
        Appearance::Dark => ThemeMode::Dark,
        Appearance::System => system,
    }
}

pub(crate) fn watch_system(window: &mut Window, cx: &mut App) -> Subscription {
    follow_system(ThemeMode::from(window.appearance()), cx);
    window.observe_window_appearance(|window, cx| {
        follow_system(ThemeMode::from(window.appearance()), cx);
    })
}

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

pub(super) fn read_user(dir: &Path) -> Vec<CatalogEntry> {
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

#[cfg(test)]
mod face_kind_tests {
    use super::face_kind;

    fn letters(width: impl Fn(char) -> f32 + 'static) -> impl Fn(char) -> Option<f32> {
        move |ch| Some(width(ch))
    }

    #[test]
    fn a_face_without_latin_letters_is_not_a_text_face() {
        let braille = |ch: char| {
            if ch.is_ascii_alphabetic() {
                None
            } else {
                Some(8.0)
            }
        };
        assert_eq!(face_kind(braille), None);
    }

    #[test]
    fn equal_letter_widths_are_monospace_and_mixed_ones_are_not() {
        assert_eq!(face_kind(letters(|_| 9.0)), Some(true));
        assert_eq!(
            face_kind(letters(|ch| if ch == 'i' { 3.0 } else { 9.0 })),
            Some(false)
        );
    }
}
