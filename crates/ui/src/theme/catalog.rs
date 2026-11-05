use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::component::ThemeMode;
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, Global, Hsla, Subscription, Task, Window,
};
use kg_domain::{Appearance, Op, OpError, OpResult, Palette, ThemeFile, ThemeOp, ThemeSettings};
use kg_plugins::PluginHost;

use kg_core::{Workspace, WorkspaceEvent};

use super::colors::apply;
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
