//! The `Op` the CLI and the window share, its result, and its error.

mod browser;
mod check;
mod config;
mod diagnostics;
mod diff;
mod environment;
mod guide;
mod index;
mod keymap;
mod plugin;
mod preference;
mod program;
mod project;
mod section;
mod settings;
mod skill;
mod surface;
mod terminal;
mod theme;
mod thread;
mod ui;
mod window;

use std::path::PathBuf;

use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    AgentTool, Environment, EnvironmentId, Event, IndexEntry, Listing, PendingInteraction, Plugin,
    PreviewOp, ProgramList, Project, ProjectId, Sidebar, Thread, ThreadId,
};

pub use browser::{BrowserControl, BrowserOp, BrowserSnapshot, BrowserView};
pub use check::check;
pub use config::ConfigOp;
pub use diagnostics::DiagnosticsOp;
pub use diff::{DiffOp, DiffQuery, DiffScope};
pub use environment::{BranchList, EnvironmentOp, EnvironmentSpec, FOLDER_IN_USE, ProjectBranch};
pub use guide::GuideChapter;
pub use index::IndexOp;
pub use keymap::{
    CommandGroup, KeyPlatform, Keymap, KeymapAction, KeymapOp, SHORTCUTS, Shortcut, ShortcutGroup,
    ShortcutScope, ShortcutWhen, default_chord, expand_secondary,
};
pub use plugin::PluginOp;
pub use preference::PreferenceOp;
pub use program::ProgramOp;
pub use project::ProjectOp;
pub use section::SectionOp;
pub use settings::SettingsOp;
pub use skill::{Skill, SkillOp, SkillSource};
pub use surface::{Slot, SlotChoice, SlotChoiceError, SurfaceItem, SurfaceOp, SurfacePlacement};
pub use terminal::{
    ExtraTerminal, SCREEN_MAX_BYTES, SelectMode, TERMINAL_COLS, TERMINAL_ROWS, TerminalOp,
};
pub use theme::{Appearance, ThemeOp, ThemeSettings, valid_theme_choice, valid_theme_id};
pub use thread::{
    Metadata, Order, Run, SEARCH_DEFAULT, SEARCH_LIMIT, SEARCH_TEXT_MAX, ThreadOp, WaitOutcome,
    branch_slug,
};
pub use ui::{
    DropSide, UI_PERSON_ONLY, UI_SCREENSHOT_UNAVAILABLE, UI_UNAVAILABLE, UiBounds, UiElement, UiOp,
    UiSnapshot,
};
pub use window::WindowOp;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Op {
    /// Current thread context and plugin warnings
    Status {
        #[arg(long, env = "KG_THREAD_ID")]
        thread_id: Option<ThreadId>,
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
        #[arg(long, env = "KG_ENVIRONMENT_ID")]
        environment_id: Option<EnvironmentId>,
    },
    /// Concepts and command chapters
    Guide { chapter: Option<GuideChapter> },
    /// Find, read and create skills
    #[command(subcommand)]
    Skill(SkillOp),
    /// Open a window
    Open,
    /// Threads
    #[command(subcommand)]
    Thread(ThreadOp),
    /// Projects
    #[command(subcommand)]
    Project(ProjectOp),
    /// Programs a new thread can run, as the new-thread picker lists them
    #[command(subcommand)]
    Program(ProgramOp),
    /// Environments
    #[command(subcommand)]
    Environment(EnvironmentOp),
    /// Plugins
    #[command(subcommand)]
    Plugin(PluginOp),
    /// Projects, sections and threads as the sidebar shows them
    Sidebar,
    /// Named groups threads are filed under
    #[command(subcommand)]
    Section(SectionOp),
    /// Read or change appearance
    #[command(subcommand)]
    Theme(ThemeOp),
    /// The settings page in the window
    #[command(subcommand)]
    Settings(SettingsOp),
    /// The application menu and the window
    #[command(subcommand)]
    Window(WindowOp),
    /// Plugin surfaces in the window
    #[command(subcommand)]
    Surface(SurfaceOp),
    /// Keyboard shortcuts: built-in ones and plugin commands
    #[command(subcommand)]
    Keymap(KeymapOp),
    /// Read or save a preference: confirm-archive, agent-integration, notify, plugin-status, update-check, terminal-font-size, review-viewed, slot.threads and slot.review (set from the window or with kg plugin use), or one window's `<window>.left-sidebar`, `<window>.left-width`, `<window>.right-sidebar`, `<window>.right-width` and `<window>.review-wrap`, where `<window>` is a window id such as main
    #[command(subcommand)]
    Preference(PreferenceOp),
    /// Native window for QA and agents
    #[command(subcommand)]
    Ui(UiOp),
    /// The Browser tab's page, driven by this thread
    #[command(subcommand)]
    Browser(BrowserOp),
    /// A thread environment's diff. Reads git and never writes the worktree.
    #[command(subcommand)]
    Diff(DiffOp),
    /// One file in the Preview tab, read-only
    #[command(subcommand)]
    Preview(PreviewOp),
    /// Write a local diagnostics file
    #[command(subcommand)]
    Diagnostics(DiagnosticsOp),
    /// Check for a newer kagan
    #[command(subcommand)]
    Update(crate::UpdateOp),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum OpResult {
    Text(String),
    /// Serialized as `text` so `kg thread wait --json` keeps the key it printed before.
    #[serde(rename(serialize = "text", deserialize = "wait"))]
    Wait(WaitOutcome),
    Diagnostics {
        path: PathBuf,
    },
    /// A saved screenshot. `current` is false when the window had not drawn a new frame, so the
    /// image may show an earlier state.
    Capture {
        path: String,
        current: bool,
    },
    Warnings(Vec<String>),
    Skills {
        skills: Vec<Skill>,
        errors: Vec<String>,
    },
    Skill {
        skill: Skill,
        body: String,
        files: Vec<PathBuf>,
    },
    Files(Vec<PathBuf>),
    Plugins(Vec<Plugin>),
    Indexes(Vec<String>),
    /// An index entry for a shared plugin, and the line a teammate runs
    Shared {
        entry: IndexEntry,
        install: String,
    },
    /// Index entries, and one line for each index that could not be read
    Listings {
        listings: Vec<Listing>,
        warnings: Vec<String>,
    },
    Logs {
        text: String,
        after: u64,
        reset: bool,
    },
    Status {
        thread_id: Option<ThreadId>,
        project_id: Option<ProjectId>,
        environment_id: Option<EnvironmentId>,
        cli: PathBuf,
        data_dir: PathBuf,
        warnings: Vec<String>,
    },
    Done,
    Cli {
        exit_code: i32,
        stdout: String,
        stderr: String,
    },
    Thread(Thread),
    Threads(Vec<Thread>),
    Found(Vec<crate::Found>),
    Output(Option<String>),
    Screen(String),
    Metadata(Value),
    Events(Vec<Event>),
    Event(Option<Event>),
    Sidebar(Sidebar),
    Project(Project),
    Projects(Vec<Project>),
    Programs(ProgramList),
    Environment(Environment),
    Branches(BranchList),
    Setting(Value),
    Tools {
        instructions: Option<String>,
        tools: Vec<AgentTool>,
    },
    ToolReply {
        text: String,
        is_error: bool,
    },
    Interactions(Vec<PendingInteraction>),
    Theme(ThemeSettings),
    Ui(UiSnapshot),
    Browser(BrowserView),
    BrowserSnapshot(BrowserSnapshot),
    Diff(crate::DiffList),
    DiffFile(crate::DiffFileView),
    Preview(crate::PreviewFile),
    Commits(Vec<crate::DiffCommit>),
    Surfaces(Vec<SurfaceItem>),
    Terminals(Vec<ExtraTerminal>),
    Details(crate::ThreadDetails),
    Links(Vec<String>),
    Update(crate::UpdateStatus),
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(into = "Wire", from = "Wire")]
pub enum OpError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Failed(String),
}

impl From<std::io::Error> for OpError {
    fn from(error: std::io::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

impl From<serde_json::Error> for OpError {
    fn from(error: serde_json::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

#[derive(Serialize, Deserialize)]
struct Wire {
    error: Body,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "message", rename_all = "kebab-case")]
enum Body {
    NotFound(String),
    Invalid(String),
    Failed(String),
}

impl From<OpError> for Wire {
    fn from(error: OpError) -> Self {
        let error = match error {
            OpError::NotFound(message) => Body::NotFound(message),
            OpError::Invalid(message) => Body::Invalid(message),
            OpError::Failed(message) => Body::Failed(message),
        };
        Self { error }
    }
}

impl From<Wire> for OpError {
    fn from(wire: Wire) -> Self {
        match wire.error {
            Body::NotFound(message) => Self::NotFound(message),
            Body::Invalid(message) => Self::Invalid(message),
            Body::Failed(message) => Self::Failed(message),
        }
    }
}

#[derive(Serialize)]
struct InvalidWire<'a> {
    error: InvalidBody<'a>,
}

#[derive(Serialize)]
struct InvalidBody<'a> {
    kind: &'static str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<&'a str>,
}

#[must_use]
pub fn invalid_json(message: &str, hint: &str) -> String {
    let hint = (!hint.is_empty()).then_some(hint);
    serde_json::to_string(&InvalidWire {
        error: InvalidBody {
            kind: "invalid",
            message,
            hint,
        },
    })
    .unwrap_or_else(|_| {
        "{\"error\":{\"kind\":\"invalid\",\"message\":\"could not format the error\"}}".to_owned()
    })
}

fn json(text: String) -> Value {
    serde_json::from_str(&text).unwrap_or(Value::String(text))
}
