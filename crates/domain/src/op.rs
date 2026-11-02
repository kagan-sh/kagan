use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::{
    Args, Subcommand, ValueEnum,
    builder::{StringValueParser, TypedValueParser},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    AgentTool, Environment, EnvironmentId, Event, IndexEntry, InteractionOption, Listing,
    PendingInteraction, Plugin, PluginId, ProgramList, Project, ProjectId, SectionId, Sidebar,
    Thread, ThreadId, Timestamp, Visibility,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
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
    /// Read or save a preference: confirm-archive, agent-integration, notify, update-check, terminal-font-size, main.left-sidebar, main.right-sidebar, main.left-width, main.right-width, or main.review-wrap
    #[command(subcommand)]
    Preference(PreferenceOp),
    /// Native window for QA and agents
    #[command(subcommand)]
    Ui(UiOp),
    /// A thread environment's diff. Reads git and never writes the worktree.
    #[command(subcommand)]
    Diff(DiffOp),
    /// Write a local diagnostics file
    #[command(subcommand)]
    Diagnostics(DiagnosticsOp),
    /// Check for a newer kagan
    #[command(subcommand)]
    Update(crate::UpdateOp),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
pub enum DiagnosticsOp {
    /// Write a local diagnostics file. Refuses to replace an existing file.
    Export {
        /// File to write. The parent folder must exist. Omit to write under the data directory.
        #[arg(long)]
        path: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum GuideChapter {
    Threads,
    Projects,
    Environments,
    Plugins,
    Skills,
    Ui,
    Json,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum SkillOp {
    List {
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
    },
    Show {
        name: String,
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
    },
    Files {
        name: String,
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
    },
    New {
        name: String,
        #[arg(long)]
        project: bool,
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
    },
    Add {
        path: PathBuf,
        #[arg(long)]
        project: bool,
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
    },
    Remove {
        name: String,
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    Dark,
    /// Follow the operating system's light or dark mode
    #[default]
    System,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSettings {
    pub theme: String,
    pub mode: Appearance,
}

/// One shortcut: the saved name, the GPUI action it binds, the default chord, and the label.
#[derive(Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub name: &'static str,
    pub action: &'static str,
    pub chord: &'static str,
    pub label: &'static str,
}

/// Built-in shortcuts. A saved override uses `name`, so those names stay.
pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        name: "Bigger",
        action: "kagan::Bigger",
        chord: "secondary-=",
        label: "Bigger terminal text",
    },
    Shortcut {
        name: "Smaller",
        action: "kagan::Smaller",
        chord: "secondary--",
        label: "Smaller terminal text",
    },
    Shortcut {
        name: "ActualSize",
        action: "kagan::ActualSize",
        chord: "secondary-0",
        label: "Actual terminal text size",
    },
    Shortcut {
        name: "NewThread",
        action: "kagan::NewThread",
        chord: "secondary-n",
        label: "New thread",
    },
    Shortcut {
        name: "StopThread",
        action: "kagan::StopThread",
        chord: "secondary-.",
        label: "Stop thread",
    },
    Shortcut {
        name: "ArchiveThread",
        action: "kagan::ArchiveThread",
        chord: "secondary-shift-backspace",
        label: "Archive thread",
    },
    Shortcut {
        name: "TogglePinned",
        action: "kagan::TogglePinned",
        chord: "secondary-shift-p",
        label: "Pin or unpin thread",
    },
    Shortcut {
        name: "NextThread",
        action: "kagan::NextThread",
        chord: "secondary-shift-]",
        label: "Next thread",
    },
    Shortcut {
        name: "PreviousThread",
        action: "kagan::PreviousThread",
        chord: "secondary-shift-[",
        label: "Previous thread",
    },
    Shortcut {
        name: "NextThreadNeedingYou",
        action: "kagan::NextThreadNeedingYou",
        chord: "secondary-alt-j",
        label: "Next thread needing you",
    },
    Shortcut {
        name: "Thread1",
        action: "kagan::Thread1",
        chord: "secondary-1",
        label: "Thread 1",
    },
    Shortcut {
        name: "Thread2",
        action: "kagan::Thread2",
        chord: "secondary-2",
        label: "Thread 2",
    },
    Shortcut {
        name: "Thread3",
        action: "kagan::Thread3",
        chord: "secondary-3",
        label: "Thread 3",
    },
    Shortcut {
        name: "Thread4",
        action: "kagan::Thread4",
        chord: "secondary-4",
        label: "Thread 4",
    },
    Shortcut {
        name: "Thread5",
        action: "kagan::Thread5",
        chord: "secondary-5",
        label: "Thread 5",
    },
    Shortcut {
        name: "Thread6",
        action: "kagan::Thread6",
        chord: "secondary-6",
        label: "Thread 6",
    },
    Shortcut {
        name: "Thread7",
        action: "kagan::Thread7",
        chord: "secondary-7",
        label: "Thread 7",
    },
    Shortcut {
        name: "Thread8",
        action: "kagan::Thread8",
        chord: "secondary-8",
        label: "Thread 8",
    },
    Shortcut {
        name: "Thread9",
        action: "kagan::Thread9",
        chord: "secondary-9",
        label: "Thread 9",
    },
    Shortcut {
        name: "OpenSettings",
        action: "kagan::OpenSettings",
        chord: "secondary-,",
        label: "Settings",
    },
    Shortcut {
        name: "UseProjectFolder",
        action: "kagan::UseProjectFolder",
        chord: "secondary-alt-p",
        label: "Project folder",
    },
    Shortcut {
        name: "UseWorktreeFolder",
        action: "kagan::UseWorktreeFolder",
        chord: "secondary-alt-w",
        label: "New worktree",
    },
    Shortcut {
        name: "RetryBranches",
        action: "kagan::RetryBranches",
        chord: "secondary-alt-r",
        label: "Retry branches",
    },
    Shortcut {
        name: "RetryPrograms",
        action: "kagan::RetryPrograms",
        chord: "secondary-alt-l",
        label: "Retry programs",
    },
    Shortcut {
        name: "ToggleCustomCommand",
        action: "kagan::ToggleCustomCommand",
        chord: "secondary-alt-c",
        label: "Custom command",
    },
    Shortcut {
        name: "ThreadActions",
        action: "kagan::ThreadActions",
        chord: "secondary-shift-m",
        label: "Thread actions",
    },
    Shortcut {
        name: "ToggleSidebar",
        action: "kagan::ToggleSidebar",
        chord: "secondary-b",
        label: "Show or hide left sidebar",
    },
    Shortcut {
        name: "ToggleRightSidebar",
        action: "kagan::ToggleRightSidebar",
        chord: "secondary-\\",
        label: "Show or hide right sidebar",
    },
    Shortcut {
        name: "AddProject",
        action: "kagan::AddProject",
        chord: "secondary-shift-o",
        label: "Add project",
    },
    Shortcut {
        name: "SendReview",
        action: "review::SendReview",
        chord: "secondary-enter",
        label: "Send review",
    },
    Shortcut {
        name: "NextHunk",
        action: "review::NextChange",
        chord: "alt-]",
        label: "Next change",
    },
    Shortcut {
        name: "PrevHunk",
        action: "review::PrevChange",
        chord: "alt-[",
        label: "Previous change",
    },
    Shortcut {
        name: "FindInDiff",
        action: "review::FindInDiff",
        chord: "secondary-f",
        label: "Find in diff",
    },
    Shortcut {
        name: "ToggleWrap",
        action: "review::ToggleWrap",
        chord: "alt-z",
        label: "Wrap lines",
    },
    Shortcut {
        name: "NextMatch",
        action: "review::NextMatch",
        chord: "secondary-g",
        label: "Next match",
    },
    Shortcut {
        name: "PrevMatch",
        action: "review::PrevMatch",
        chord: "secondary-alt-g",
        label: "Previous match",
    },
    Shortcut {
        name: "StartAgain",
        action: "kagan::StartAgain",
        chord: "secondary-shift-s",
        label: "Start again",
    },
    Shortcut {
        name: "FocusSidebar",
        action: "kagan::FocusSidebar",
        chord: "secondary-shift-l",
        label: "Focus sidebar",
    },
    Shortcut {
        name: "RestoreThread",
        action: "kagan::RestoreThread",
        chord: "secondary-shift-r",
        label: "Restore thread",
    },
    Shortcut {
        name: "UseBuiltInSidebar",
        action: "kagan::UseBuiltInSidebar",
        chord: "secondary-alt-b",
        label: "Use built-in sidebar",
    },
    Shortcut {
        name: "UseBuiltInReview",
        action: "kagan::UseBuiltInReview",
        chord: "secondary-alt-v",
        label: "Use built-in review",
    },
    Shortcut {
        name: "OpenCommandList",
        action: "kagan::OpenCommandList",
        chord: "secondary-shift-a",
        label: "Command list",
    },
    Shortcut {
        name: "SearchThreads",
        action: "kagan::SearchThreads",
        chord: "secondary-alt-f",
        label: "Search threads…",
    },
    Shortcut {
        name: "NewTerminal",
        action: "kagan::NewTerminal",
        chord: "secondary-t",
        label: "New terminal",
    },
    Shortcut {
        name: "CloseTerminal",
        action: "kagan::CloseTerminal",
        chord: "secondary-shift-w",
        label: "Close terminal",
    },
    Shortcut {
        name: "NextTerminal",
        action: "kagan::NextTerminal",
        chord: "secondary-alt-]",
        label: "Next terminal",
    },
    Shortcut {
        name: "PreviousTerminal",
        action: "kagan::PreviousTerminal",
        chord: "secondary-alt-[",
        label: "Previous terminal",
    },
    Shortcut {
        name: "OpenLocalLink",
        action: "kagan::OpenLocalLink",
        chord: "secondary-alt-o",
        label: "Open local link",
    },
];

/// A shortcut as `kg keymap` names it: a built-in by its name, or a plugin's command as
/// `<plugin-id>:<command>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeymapAction {
    BuiltIn(&'static Shortcut),
    Plugin { plugin: PluginId, command: String },
}

impl KeymapAction {
    /// # Errors
    ///
    /// Returns `Invalid` for a name that is neither a built-in shortcut nor `<plugin-id>:<command>`.
    pub fn parse(action: &str) -> Result<Self, OpError> {
        if let Some((plugin, command)) = action.split_once(':') {
            if plugin.is_empty() || command.is_empty() {
                return Err(OpError::Invalid(format!(
                    "{action} is not <plugin-id>:<command>"
                )));
            }
            return Ok(Self::Plugin {
                plugin: PluginId(plugin.to_owned()),
                command: command.to_owned(),
            });
        }
        SHORTCUTS
            .iter()
            .find(|shortcut| shortcut.name == action)
            .map(Self::BuiltIn)
            .ok_or_else(|| OpError::Invalid(format!("unknown shortcut {action}")))
    }
}

impl std::fmt::Display for KeymapAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BuiltIn(shortcut) => f.write_str(shortcut.name),
            Self::Plugin { plugin, command } => write!(f, "{plugin}:{command}"),
        }
    }
}

/// The chord an action answers to. A saved override wins. Off macOS, `secondary` is Ctrl+Shift,
/// because a bare Ctrl chord belongs to the program.
#[must_use]
pub fn effective_chord(name: &str, keymap: &Keymap, platform: KeyPlatform) -> String {
    if let Some(saved) = keymap.shared.get(name) {
        return saved.clone();
    }
    let Some(shortcut) = SHORTCUTS.iter().find(|shortcut| shortcut.name == name) else {
        return String::new();
    };
    expand_secondary(shortcut.chord, platform)
}

/// `secondary` is the platform key on macOS and Ctrl+Shift elsewhere. A following `shift-` is already in Ctrl+Shift.
#[must_use]
pub fn expand_secondary(chord: &str, platform: KeyPlatform) -> String {
    if platform == KeyPlatform::Mac {
        return chord.to_owned();
    }
    match chord.strip_prefix("secondary-") {
        Some(rest) => format!("ctrl-shift-{}", rest.trim_start_matches("shift-")),
        None => chord.to_owned(),
    }
}

/// Which machine the window is on. `secondary` stays the command key on macOS and becomes Ctrl+Shift elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum KeyPlatform {
    Mac,
    Windows,
    Linux,
}

impl KeyPlatform {
    #[must_use]
    pub fn host() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// Saved shortcut overrides. One data directory is one machine, so a chord is not scoped to a platform.
/// An older map that named `mac`, `windows`, or `linux` keeps only its `shared` part.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keymap {
    #[serde(default)]
    pub shared: BTreeMap<String, String>,
}

impl Keymap {
    /// Reads `shared`, or an older flat map of action to chord.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is JSON this map cannot read.
    pub fn from_setting(value: Option<Value>) -> Result<Self, serde_json::Error> {
        let Some(value) = value else {
            return Ok(Self::default());
        };
        let scoped = value.as_object().is_some_and(|map| {
            map.keys()
                .any(|key| matches!(key.as_str(), "shared" | "mac" | "windows" | "linux"))
        });
        if scoped {
            Ok(Self {
                shared: serde_json::from_value(
                    value
                        .get("shared")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
                )?,
            })
        } else {
            Ok(Self {
                shared: serde_json::from_value(value)?,
            })
        }
    }

    #[must_use]
    pub fn binding(&self, action: &str) -> Option<&str> {
        self.shared.get(action).map(String::as_str)
    }

    pub fn set(&mut self, action: String, key: String) {
        self.shared.insert(action, key);
    }

    pub fn reset(&mut self, action: &str) {
        self.shared.remove(action);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum KeymapOp {
    /// Saved shortcut overrides, and the key of each enabled plugin's commands
    Get,
    /// Rebind a shortcut, or reset it
    Set {
        /// Action name, such as `ToggleSidebar`, or `<plugin-id>:<command>` for a plugin's command
        action: String,
        /// Chord to record. Empty with `--reset`.
        key: String,
        /// Drop the saved chord and use the default key
        #[arg(long)]
        #[serde(default)]
        reset: bool,
    },
    /// Leave a plugin's command with no key
    Remove {
        /// `<plugin-id>:<command>`
        action: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsOp {
    /// Show the settings page, opening the window if it is closed
    Open,
    /// Show the plugin management section
    Plugins,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
pub enum WindowOp {
    /// Show the application name and version
    About,
    /// Hide kagan
    Hide,
    /// Hide the other applications
    HideOthers,
    /// Show the applications hidden by Hide Others
    ShowAll,
    /// Quit. Programs still running end
    Quit,
    /// Minimize the window
    Minimize,
    /// Zoom the window
    Zoom,
    /// Close the window. Programs keep running
    Close,
    /// Open the next visible thread and focus its terminal
    NextThread,
    /// Open the previous visible thread and focus its terminal
    PreviousThread,
    /// Open the Nth visible thread and focus its terminal
    Thread {
        #[arg(value_parser = clap::value_parser!(u8).range(1..=9))]
        index: u8,
    },
    /// Open the oldest unread thread or question and focus its terminal
    NextThreadNeedingYou,
    /// Toggle wrapping in the open diff review
    ToggleReviewWrap,
    /// Open the command list
    CommandList,
    /// Open the command list on thread search
    SearchThreads,
    /// Show the shown thread's next terminal tab
    NextTerminal,
    /// Show the shown thread's previous terminal tab
    PreviousTerminal,
    /// Open the newest local link the shown thread printed, such as `localhost:3000`, in the browser
    OpenLocalLink,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SurfacePlacement {
    Left,
    Right,
    Center,
}

/// A part of the window a plugin may replace: the thread list or the file review.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum Slot {
    Threads,
    Review,
}

impl Slot {
    pub const ALL: [Self; 2] = [Self::Threads, Self::Review];

    /// The manifest and CLI name: `threads` or `review`.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Threads => "threads",
            Self::Review => "review",
        }
    }

    /// The preference that holds the plugin id filling this slot, or `kagan`.
    #[must_use]
    pub fn preference(self) -> String {
        format!("slot.{}", self.id())
    }

    /// What a person reads: "replaceable sidebar" or "replaceable review".
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Threads => "replaceable sidebar",
            Self::Review => "replaceable review",
        }
    }
}

/// Who fills a slot: the built-in, or one plugin. Written `kagan` or the plugin id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum SlotChoice {
    Kagan,
    Plugin(PluginId),
}

impl std::str::FromStr for SlotChoice {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let id = PluginId(value.to_owned());
        match value {
            "kagan" => Ok(Self::Kagan),
            _ if id.valid() => Ok(Self::Plugin(id)),
            _ => Err(format!("{value:?} must be a plugin id or kagan")),
        }
    }
}

impl TryFrom<String> for SlotChoice {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<SlotChoice> for String {
    fn from(choice: SlotChoice) -> Self {
        choice.to_string()
    }
}

impl std::fmt::Display for SlotChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Kagan => f.write_str("kagan"),
            Self::Plugin(id) => f.write_str(&id.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceItem {
    pub plugin: PluginId,
    pub id: String,
    pub title: String,
    pub placement: SurfacePlacement,
    /// Selected or expanded. A change still waiting keeps the previous selection.
    pub open: bool,
    /// The body is painted at a non-zero size. While a change waits, this stays with the
    /// frame from before that change, unless a newer mount replaced the body.
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum SurfaceOp {
    /// Declared plugin surfaces, and which are on screen
    List,
    /// Show the threads list in the left sidebar
    Threads,
    /// Show a declared surface. Succeeds only when it is on screen.
    Open { plugin: PluginId, id: String },
    /// Close a left tool, a center page, or a right tab
    Close { plugin: PluginId, id: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
pub enum PreferenceOp {
    /// Read a preference
    Get { key: String },
    /// Save a preference
    Set {
        key: String,
        #[arg(value_parser = StringValueParser::new().map(json))]
        value: Value,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum ThemeOp {
    /// Selected theme and mode
    Get,
    /// Select a theme and mode
    Set {
        #[arg(value_parser = theme_choice)]
        theme: String,
        #[arg(value_enum)]
        mode: Appearance,
    },
    /// Copy a theme to <dataDir>/themes/<id>.json and print its path
    New {
        /// The new file's id: lowercase letters, digits and dashes
        #[arg(value_parser = theme_id)]
        id: String,
        /// The theme to copy; the one in use when left out
        #[arg(long, value_parser = theme_choice)]
        from: Option<String>,
    },
    /// Print the folder of your own theme files, making it when missing
    Folder,
}

pub const UI_UNAVAILABLE: &str = "the ui driver needs a kagan window";

pub const UI_PERSON_ONLY: &str = "person-only approvals cannot be answered with kg ui";

pub const UI_SCREENSHOT_UNAVAILABLE: &str = "ui screenshot is not available on this platform";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiElement {
    pub id: String,
    pub path: Vec<String>,
    pub role: Option<String>,
    pub label: Option<String>,
    pub value: Option<String>,
    pub bounds: UiBounds,
    pub focused: Option<bool>,
    pub disabled: Option<bool>,
    pub selected: Option<bool>,
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSnapshot {
    pub window: u32,
    pub elements: Vec<UiElement>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum UiOp {
    /// Observed elements with stable ids, roles, bounds, and focus state
    Snapshot {
        #[arg(long, default_value_t = 0)]
        window: u32,
    },
    /// Click the center of an element. `--within` picks the one inside that element, when several match
    Click {
        element_id: String,
        #[arg(long)]
        within: Option<String>,
        #[arg(long, default_value_t = 0)]
        window: u32,
    },
    /// Send text to the focused control
    Type {
        text: String,
        #[arg(long, default_value_t = 0)]
        window: u32,
    },
    /// Send a keystroke such as escape or cmd-,
    Key {
        keystroke: String,
        #[arg(long, default_value_t = 0)]
        window: u32,
    },
    /// Scroll an element by dy pixels
    Scroll {
        element_id: String,
        dy: f32,
        #[arg(long)]
        within: Option<String>,
        #[arg(long, default_value_t = 0)]
        window: u32,
    },
    /// Save a PNG without focusing the window
    Screenshot {
        path: PathBuf,
        #[arg(long, default_value_t = 0)]
        window: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
pub enum DiffScope {
    Uncommitted,
    Branch,
    Commit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Args)]
#[serde(rename_all = "camelCase")]
pub struct DiffQuery {
    #[arg(env = "KG_THREAD_ID")]
    pub thread_id: ThreadId,
    #[arg(long, value_enum)]
    pub scope: DiffScope,
    #[arg(long)]
    pub base: Option<String>,
    #[arg(long)]
    pub rev: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum DiffOp {
    /// Changed files in a scope
    List {
        #[command(flatten)]
        #[serde(flatten)]
        query: DiffQuery,
    },
    /// One file's diff and its new side
    Show {
        #[command(flatten)]
        #[serde(flatten)]
        query: DiffQuery,
        path: String,
    },
    /// Commits that can be reviewed, newest first
    Commits {
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum TerminalOp {
    /// Open another terminal in the thread's folder, running the login shell
    New {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// The thread's extra terminals
    List {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Close an extra terminal and end its shell
    Close {
        #[arg(long, env = "KG_THREAD_ID")]
        id: ThreadId,
        number: u32,
    },
}

/// A terminal kagan opened beside a thread's program. Numbers are never reused within a thread.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraTerminal {
    pub number: u32,
    /// The title the shell set, if any
    pub title: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum ThreadOp {
    /// Show a thread
    Get {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// List threads
    List {
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
        #[arg(long)]
        archived: Option<bool>,
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        before: Option<Timestamp>,
    },
    /// Start a thread that runs a program in its folder
    Run(Run),
    /// Stop the program
    Stop {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Type into the program's terminal
    Input {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
        text: String,
    },
    /// Resize the program's terminal
    Resize {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
        cols: u16,
        rows: u16,
    },
    /// The program's screen as ANSI escape sequences; use `--scrollback` for plain text
    Screen {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
        /// Every kept line, as plain text
        #[arg(long)]
        #[serde(default)]
        scrollback: bool,
        /// Last rows of that scrollback, from 1 to 10000. Refused without `--scrollback`
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lines: Option<u32>,
    },
    /// Scroll the program's view back into its history, or forward with a negative count
    Scroll {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
        /// Lines up into the history; negative moves back down
        #[arg(allow_negative_numbers = true)]
        lines: i32,
    },
    /// Select text on the program's screen from a cell, counted from the top left
    Select {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
        mode: SelectMode,
        /// From the left, starting at 0
        column: u16,
        /// From the top, starting at 0
        row: u16,
        /// The right half of the cell
        #[arg(long)]
        right: bool,
    },
    /// The text selected on the program's screen
    Copy {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
    },
    /// Send mouse-report bytes to the program, as decimal byte values such as `27 91 77 32 128 33`.
    /// Unlike `input`, it keeps a scrolled view where it is.
    Report {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
        /// Decimal byte values, 0 to 255, in order
        bytes: Vec<u8>,
    },
    /// Paste text into the program, bracketed when the program asks for that
    Paste {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
        /// The text. Omit it when `--file` is set
        #[arg(required_unless_present = "file", conflicts_with = "file")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        /// Read the paste from this path, or `-` for stdin
        #[arg(long, value_name = "PATH")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file: Option<String>,
    },
    /// Extra terminals: login shells in the thread's folder
    #[command(subcommand)]
    Terminal(TerminalOp),
    /// The thread's folder, program, branch, base branch and last commits, read from git without fetching
    Details {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// http and https links printed in the thread's terminals: the program's, then each extra one.
    /// A link printed again moves to the end
    Links {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Archive a thread
    Archive {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Bring back an archived thread
    Unarchive {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Delete a thread and its history for good
    Delete {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// The thread where a person approves this delete
        #[arg(long, env = "KG_THREAD_ID")]
        #[serde(default)]
        thread_id: Option<ThreadId>,
    },
    /// Pin or unpin a thread
    SetPinned {
        #[arg(long, env = "KG_THREAD_ID")]
        id: ThreadId,
        #[arg(action = clap::ArgAction::Set)]
        pinned: bool,
    },
    /// Rename a thread
    Rename {
        #[arg(long, env = "KG_THREAD_ID")]
        id: ThreadId,
        title: String,
    },
    /// Mark a thread read
    MarkRead {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// A plugin's metadata on a thread
    Metadata {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        #[arg(long)]
        plugin_id: PluginId,
    },
    /// Set a plugin's metadata on a thread
    SetMetadata {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        #[command(flatten)]
        metadata: Metadata,
    },
    /// A thread's events
    Events {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        #[arg(long)]
        after_seq: Option<u64>,
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long, value_enum)]
        order: Option<Order>,
    },
    /// Wait for an event
    WaitEvent {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        after_seq: Option<u64>,
        #[arg(long)]
        timeout_ms: u64,
    },
    /// Wait until a reply ends, the program ends, or the time is up
    Wait {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// How long to wait, in milliseconds
        #[arg(long)]
        timeout_ms: u64,
    },
    /// Raise a question for the person. The answer arrives as `answered`
    Ask {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        title: String,
        /// An option, as id=label. Repeat for at least two
        #[arg(long = "option", value_name = "ID=LABEL", value_parser = parse_option)]
        options: Vec<InteractionOption>,
    },
    /// Run a stopped or recovered thread's program again, in its folder. Only a request does this.
    ///
    /// A claude thread, and a plain codex thread, resume their conversation; when kagan cannot find
    /// it, the agent starts a new one and the thread says so.
    StartAgain {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Agent tools offered to the thread, with plugin instructions
    Tools {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Call an agent tool as the thread
    CallTool {
        #[arg(long, env = "KG_THREAD_ID")]
        id: ThreadId,
        name: String,
        #[arg(long, value_parser = StringValueParser::new().map(json), default_value = "{}")]
        input: Value,
    },
    /// Pending interactions waiting for an answer
    Interactions {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Threads whose title or kept terminal lines hold the text, ignoring case. Hidden threads are left out
    Search {
        text: String,
        /// Most threads to return, from 1 to 200; 50 when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
        /// Also search archived threads. Each result says whether it is archived
        #[arg(long)]
        #[serde(default)]
        archived: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum SectionOp {
    /// Create a section, and file a thread under it
    Create {
        name: String,
        #[arg(long, env = "KG_THREAD_ID")]
        thread_id: Option<ThreadId>,
    },
    /// Rename a section
    Rename { id: SectionId, name: String },
    /// Remove a section. Threads in it stay, unfiled.
    Delete { id: SectionId },
    /// File a thread under a section. Omit the section to clear it.
    File {
        #[arg(long, env = "KG_THREAD_ID")]
        thread_id: ThreadId,
        #[arg(long)]
        section_id: Option<SectionId>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum ProjectOp {
    /// List projects
    List,
    /// Show a project
    Get {
        #[arg(env = "KG_PROJECT_ID")]
        id: ProjectId,
    },
    /// Add a project for a folder
    Add { path: PathBuf },
    /// Remove a project and its threads
    Remove {
        /// Project to remove
        id: ProjectId,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum ProgramOp {
    #[command(skip)]
    Discover,
    /// Programs found on the PATH, the shell, and the recent threads; the first installed one is the default
    List,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum EnvironmentOp {
    /// Show an environment
    Get {
        #[arg(env = "KG_ENVIRONMENT_ID")]
        id: EnvironmentId,
    },
    /// Recreate a thread's environment
    Restore {
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// Local branches of a project
    Branches {
        #[arg(env = "KG_PROJECT_ID")]
        id: ProjectId,
    },
    /// Which branch change counts compare against
    SetBase {
        #[arg(env = "KG_ENVIRONMENT_ID")]
        id: EnvironmentId,
        base: String,
    },
    /// Remove a worktree again after git refused because of files it would keep
    Remove {
        #[arg(env = "KG_ENVIRONMENT_ID")]
        id: EnvironmentId,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum PluginOp {
    /// Stop the plugin install that is running
    CancelInstall,
    /// Start a thread to build a plugin
    Start,
    /// Run a command a plugin declares under `cli`
    #[command(skip)]
    Cli {
        name: String,
        command: String,
        argv: Vec<String>,
        cwd: PathBuf,
        thread_id: Option<ThreadId>,
        project_id: Option<ProjectId>,
    },
    /// Run one of a plugin's declared commands
    Command {
        id: PluginId,
        command: String,
        #[arg(long, env = "KG_THREAD_ID")]
        thread_id: Option<ThreadId>,
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
    },
    /// Scaffold a plugin
    New {
        #[arg(help = "Plugin name; for example: kg plugin new hello")]
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Check a plugin without running it
    Check {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Check a committed, pushed plugin and print its index entry and install line
    Share {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Reload installed code, asking a person about new powers
    Reload {
        id: PluginId,
        #[arg(long, env = "KG_THREAD_ID")]
        thread_id: Option<ThreadId>,
    },
    /// Watch an installed local plugin and reload on changes
    Dev {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, env = "KG_THREAD_ID")]
        thread_id: Option<ThreadId>,
    },
    /// Read plugin logs
    Logs {
        id: PluginId,
        #[arg(long)]
        follow: bool,
        #[arg(long, hide = true)]
        after: Option<u64>,
    },
    /// Write declarations for this version
    Types {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Installed plugins
    List,
    /// Remove an installed plugin
    Remove {
        id: PluginId,
        /// The thread where a person approves this removal
        #[arg(long, env = "KG_THREAD_ID")]
        #[serde(default)]
        thread_id: Option<ThreadId>,
    },
    /// Install from `git:<url>@<ref>`, a local folder, or a plugin name an index lists
    Install {
        source: String,
        #[arg(long)]
        plugin: Option<String>,
        /// The thread where a person approves the plugin's powers
        #[arg(long, env = "KG_THREAD_ID")]
        thread_id: Option<ThreadId>,
    },
    /// Update to the latest commit
    Update {
        id: PluginId,
        /// The thread where a person approves new powers
        #[arg(long, env = "KG_THREAD_ID")]
        thread_id: Option<ThreadId>,
    },
    /// The plugin indexes searched by name, in order
    Index {
        #[command(subcommand)]
        op: IndexOp,
    },
    /// Plugins the indexes list whose name or description holds the text
    Search {
        #[arg(default_value = "")]
        text: String,
    },
    /// Enable a plugin
    Enable { id: PluginId },
    /// Disable a plugin
    Disable { id: PluginId },
    /// Read or change a plugin setting
    Config {
        id: PluginId,
        #[command(subcommand)]
        op: ConfigOp,
        /// The thread where a person approves a folder setting
        #[arg(long, env = "KG_THREAD_ID")]
        thread_id: Option<ThreadId>,
    },
    /// Fill a slot with a plugin that offers it. `kagan` is the built-in
    Use {
        #[arg(value_enum)]
        slot: Slot,
        /// A plugin id, or `kagan`
        id: SlotChoice,
        /// The thread where a person approves the choice
        #[arg(long, env = "KG_THREAD_ID")]
        #[serde(default)]
        thread_id: Option<ThreadId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
pub enum IndexOp {
    /// Add an index: `git:<url>@<ref>`, or a file or folder on this computer
    Add { location: String },
    /// The indexes, first searched first
    List,
    /// Remove an index
    Remove { location: String },
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigOp {
    /// Read a setting
    Get { key: String },
    /// Change a setting
    Set {
        key: String,
        /// Omit it to type it at a prompt that does not show it, or to pipe it on stdin. Do that for a secret, so it stays out of your shell history
        #[arg(value_parser = StringValueParser::new().map(json))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<Value>,
    },
    /// Forget a secret setting
    Clear { key: String },
}

// A value may be a secret, so a debug print never shows it.
impl std::fmt::Debug for ConfigOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Get { key } => f.debug_struct("Get").field("key", key).finish(),
            Self::Set { key, value } => f
                .debug_struct("Set")
                .field("key", key)
                .field("value", &value.as_ref().map(|_| "<hidden>"))
                .finish(),
            Self::Clear { key } => f.debug_struct("Clear").field("key", key).finish(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Args)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    #[arg(long, env = "KG_PROJECT_ID")]
    pub project_id: ProjectId,
    /// The program to run, such as `claude`, `codex`, or a command the person types. Words split on spaces; single or double quotes keep spaces together
    #[arg(long)]
    pub program: String,
    #[arg(long)]
    pub title: Option<String>,
    /// project-default, personal, worktree[:<base>[:<branch>]] or reuse:<environment-id>
    #[arg(long, value_parser = environment_spec, default_value = "worktree")]
    pub environment: EnvironmentSpec,
    #[arg(long, value_enum)]
    #[serde(default)]
    pub visibility: Option<Visibility>,
    #[arg(long)]
    #[serde(default)]
    pub parent_thread_id: Option<ThreadId>,
    #[arg(long)]
    #[serde(default)]
    pub lifecycle_owner_thread_id: Option<ThreadId>,
    #[command(flatten)]
    #[serde(default)]
    pub metadata: Option<Metadata>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Args)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[arg(
        long = "metadata-plugin",
        id = "metadata_plugin",
        required = false,
        requires = "metadata_value"
    )]
    pub plugin_id: PluginId,
    #[arg(
        long = "metadata",
        id = "metadata_value",
        required = false,
        requires = "metadata_plugin",
        value_parser = StringValueParser::new().map(json)
    )]
    pub value: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Order {
    Asc,
    Desc,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum EnvironmentSpec {
    ProjectDefault,
    Reuse {
        environment_id: EnvironmentId,
    },
    Worktree {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_branch: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
    Personal,
}

/// A branch name taken from a thread title: lowercase, hyphenated, at most 48 characters.
#[must_use]
pub fn branch_slug(title: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for ch in title.chars() {
        if slug.len() >= 48 {
            break;
        }
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchList {
    /// The checked-out branch; None when HEAD is detached
    pub current: Option<String>,
    /// The commit HEAD points at, the base a worktree starts from when no branch is checked out
    pub head: Option<String>,
    pub names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum OpResult {
    Text(String),
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
    Diff(crate::DiffList),
    DiffFile(crate::DiffFileView),
    Commits(Vec<crate::DiffCommit>),
    Surfaces(Vec<SurfaceItem>),
    Terminals(Vec<ExtraTerminal>),
    Details(crate::ThreadDetails),
    Links(Vec<String>),
    Update(crate::UpdateStatus),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Skill {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub hint: Option<String>,
    pub source: SkillSource,
    pub path: Option<PathBuf>,
}

impl Skill {
    /// # Errors
    /// When frontmatter is missing, invalid, or names a different folder.
    pub fn parse(
        name: &str,
        text: &str,
        source: SkillSource,
        path: Option<PathBuf>,
    ) -> Result<(Self, String), OpError> {
        #[derive(Deserialize)]
        struct Header {
            name: String,
            description: String,
            #[serde(default, rename = "argument-hint")]
            argument_hint: Option<String>,
        }
        let text = text.replace("\r\n", "\n");
        let (yaml, body) = text
            .strip_prefix("---\n")
            .and_then(|text| text.split_once("\n---\n"))
            .ok_or_else(|| OpError::Invalid("SKILL.md needs YAML frontmatter".into()))?;
        let header: Header =
            serde_yaml::from_str(yaml).map_err(|error| OpError::Invalid(error.to_string()))?;
        let valid = !name.is_empty()
            && name.split('-').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            });
        if header.name != name
            || !valid
            || header.description.trim().is_empty()
            || header.description.chars().any(char::is_control)
        {
            return Err(OpError::Invalid("skill name must match its lowercase hyphenated folder; description must be one nonempty line".into()));
        }
        let hint = header
            .argument_hint
            .map(|hint| hint.trim().to_owned())
            .filter(|hint| !hint.is_empty());
        if hint
            .as_ref()
            .is_some_and(|hint| hint.chars().any(char::is_control))
        {
            return Err(OpError::Invalid("argument-hint must be one line".into()));
        }
        Ok((
            Self {
                name: header.name,
                description: header.description,
                hint,
                source,
                path,
            },
            body.trim().to_owned(),
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillSource {
    Bundled,
    Plugin,
    User,
    Project,
}

fn json(text: String) -> Value {
    serde_json::from_str(&text).unwrap_or(Value::String(text))
}

fn environment_spec(text: &str) -> Result<EnvironmentSpec, String> {
    match text.split_once(':') {
        None if text == "project-default" => Ok(EnvironmentSpec::ProjectDefault),
        None if text == "personal" => Ok(EnvironmentSpec::Personal),
        None if text == "worktree" => Ok(EnvironmentSpec::Worktree {
            base_branch: None,
            branch: None,
        }),
        Some(("worktree", rest)) => {
            let (base, branch) = match rest.split_once(':') {
                Some((base, branch)) => (present(base), present(branch)),
                None => (present(rest), None),
            };
            Ok(EnvironmentSpec::Worktree {
                base_branch: base,
                branch,
            })
        }
        Some(("reuse", id)) => Ok(EnvironmentSpec::Reuse {
            environment_id: EnvironmentId(id.to_owned()),
        }),
        _ => Err(format!("unknown environment {text:?}")),
    }
}

fn present(text: &str) -> Option<String> {
    if text.is_empty() {
        None
    } else {
        Some(text.to_owned())
    }
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

#[must_use]
pub fn valid_theme_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// A theme id, or a plugin's theme as `<plugin id>/<theme id>`.
#[must_use]
pub fn valid_theme_choice(id: &str) -> bool {
    match id.split_once('/') {
        Some((plugin, theme)) => {
            valid_theme_id(theme) && plugin.contains('.') && plugin.split('.').all(valid_theme_id)
        }
        None => valid_theme_id(id),
    }
}

fn theme_id(id: &str) -> Result<String, String> {
    if valid_theme_id(id) {
        Ok(id.to_owned())
    } else {
        Err("theme id must contain only lowercase letters, digits and dashes".into())
    }
}

fn theme_choice(id: &str) -> Result<String, String> {
    if valid_theme_choice(id) {
        Ok(id.to_owned())
    } else {
        Err(
            "theme must be an id of lowercase letters, digits and dashes, or <plugin id>/<id>"
                .into(),
        )
    }
}

/// The columns a terminal may have.
/// How a terminal selection starts. `Extend` moves the end of the selection already there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum SelectMode {
    Simple,
    Word,
    Line,
    Block,
    Extend,
}

pub const TERMINAL_COLS: std::ops::RangeInclusive<u16> = 2..=1000;
/// The most threads `kg thread search` returns. Its help and the refusal name this number.
pub const SEARCH_LIMIT: u32 = 200;
/// How many threads `kg thread search` and the window return when no limit is given.
pub const SEARCH_DEFAULT: u32 = 50;
/// The longest search text, in characters. Its refusal names this number.
pub const SEARCH_TEXT_MAX: usize = 256;
/// The rows a terminal may have.
pub const TERMINAL_ROWS: std::ops::RangeInclusive<u16> = 1..=500;

/// Rejects an op a client must not send: a thread id that is not a plain identifier, or a
/// terminal size outside [`TERMINAL_COLS`] and [`TERMINAL_ROWS`].
///
/// # Errors
///
/// Returns an error when any thread id on `op` is empty or contains a character other than an ASCII letter, digit, `_`, or `-`, or when a resize is outside those ranges.
pub fn check(op: &Op) -> Result<(), &'static str> {
    match op {
        Op::Window(WindowOp::Thread { index }) if !(1..=9).contains(index) => {
            Err("thread index must be between 1 and 9")
        }
        Op::Status { thread_id, .. } => plain_opt(thread_id.as_ref()),
        Op::Thread(op) => thread_ids(op)
            .into_iter()
            .try_for_each(plain)
            .and_then(|()| terminal_size(op))
            .and_then(|()| search(op)),
        Op::Environment(op) => environment_thread(op),
        Op::Plugin(op) => plain_opt(plugin_thread(op)),
        Op::Guide { .. }
        | Op::Skill(_)
        | Op::Open
        | Op::Project(_)
        | Op::Program(_)
        | Op::Theme(_)
        | Op::Keymap(_)
        | Op::Preference(_)
        | Op::Settings(_)
        | Op::Window(_)
        | Op::Sidebar
        | Op::Ui(_)
        | Op::Diagnostics(_)
        | Op::Update(_) => Ok(()),
        Op::Surface(op) => match op {
            SurfaceOp::List | SurfaceOp::Threads => Ok(()),
            SurfaceOp::Open { id, .. } | SurfaceOp::Close { id, .. } => {
                if id.is_empty() || id.chars().any(char::is_control) {
                    Err("surface id must be a plain name")
                } else {
                    Ok(())
                }
            }
        },
        Op::Section(op) => match op {
            SectionOp::Create { thread_id, .. } => plain_opt(thread_id.as_ref()),
            SectionOp::File { thread_id, .. } => plain(thread_id),
            SectionOp::Rename { .. } | SectionOp::Delete { .. } => Ok(()),
        },
        Op::Diff(op) => match op {
            DiffOp::List { query } | DiffOp::Show { query, .. } => plain(&query.thread_id),
            DiffOp::Commits { thread_id } => plain(thread_id),
        },
    }
}

fn terminal_size(op: &ThreadOp) -> Result<(), &'static str> {
    match op {
        ThreadOp::Resize { cols, rows, .. }
            if !TERMINAL_COLS.contains(cols) || !TERMINAL_ROWS.contains(rows) =>
        {
            Err("terminal size must be 2 to 1000 columns and 1 to 500 rows")
        }
        _ => Ok(()),
    }
}

fn search(op: &ThreadOp) -> Result<(), &'static str> {
    match op {
        ThreadOp::Search { text, .. } if text.trim().is_empty() => Err("search needs text"),
        ThreadOp::Search { text, .. } if text.chars().count() > SEARCH_TEXT_MAX => {
            Err("search text is longer than 256 characters")
        }
        ThreadOp::Search {
            limit: Some(limit), ..
        } if !(1..=SEARCH_LIMIT).contains(limit) => Err("--limit must be from 1 to 200"),
        _ => Ok(()),
    }
}

fn parse_option(raw: &str) -> Result<InteractionOption, &'static str> {
    let (id, label) = raw.split_once('=').ok_or("an option is id=label")?;
    if id.is_empty() || label.is_empty() {
        return Err("an option is id=label");
    }
    Ok(InteractionOption {
        id: id.to_owned(),
        label: label.to_owned(),
    })
}

fn plain(id: &ThreadId) -> Result<(), &'static str> {
    if id.plain() {
        Ok(())
    } else {
        Err("thread id must be a plain identifier")
    }
}

fn plain_opt(id: Option<&ThreadId>) -> Result<(), &'static str> {
    id.map_or(Ok(()), plain)
}

fn thread_ids(op: &ThreadOp) -> Vec<&ThreadId> {
    match op {
        ThreadOp::List { .. } | ThreadOp::Search { .. } => Vec::new(),
        ThreadOp::Run(run) => run
            .parent_thread_id
            .as_ref()
            .into_iter()
            .chain(run.lifecycle_owner_thread_id.as_ref())
            .collect(),
        ThreadOp::Delete { id, thread_id } => thread_id.iter().chain(std::iter::once(id)).collect(),
        ThreadOp::Get { id, .. }
        | ThreadOp::Stop { id, .. }
        | ThreadOp::Input { id, .. }
        | ThreadOp::Resize { id, .. }
        | ThreadOp::Screen { id, .. }
        | ThreadOp::Scroll { id, .. }
        | ThreadOp::Select { id, .. }
        | ThreadOp::Copy { id, .. }
        | ThreadOp::Paste { id, .. }
        | ThreadOp::Report { id, .. }
        | ThreadOp::Terminal(
            TerminalOp::New { id } | TerminalOp::List { id } | TerminalOp::Close { id, .. },
        )
        | ThreadOp::Details { id }
        | ThreadOp::Links { id }
        | ThreadOp::Archive { id, .. }
        | ThreadOp::Unarchive { id, .. }
        | ThreadOp::SetPinned { id, .. }
        | ThreadOp::Rename { id, .. }
        | ThreadOp::MarkRead { id, .. }
        | ThreadOp::Metadata { id, .. }
        | ThreadOp::SetMetadata { id, .. }
        | ThreadOp::Events { id, .. }
        | ThreadOp::WaitEvent { id, .. }
        | ThreadOp::Wait { id, .. }
        | ThreadOp::Ask { id, .. }
        | ThreadOp::StartAgain { id, .. }
        | ThreadOp::Tools { id, .. }
        | ThreadOp::CallTool { id, .. }
        | ThreadOp::Interactions { id, .. } => vec![id],
    }
}

fn environment_thread(op: &EnvironmentOp) -> Result<(), &'static str> {
    match op {
        EnvironmentOp::Restore { thread_id } => plain(thread_id),
        EnvironmentOp::Get { .. }
        | EnvironmentOp::Branches { .. }
        | EnvironmentOp::SetBase { .. }
        | EnvironmentOp::Remove { .. } => Ok(()),
    }
}

fn plugin_thread(op: &PluginOp) -> Option<&ThreadId> {
    match op {
        PluginOp::Cli { thread_id, .. }
        | PluginOp::Command { thread_id, .. }
        | PluginOp::Reload { thread_id, .. }
        | PluginOp::Dev { thread_id, .. }
        | PluginOp::Install { thread_id, .. }
        | PluginOp::Update { thread_id, .. }
        | PluginOp::Config { thread_id, .. }
        | PluginOp::Use { thread_id, .. }
        | PluginOp::Remove { thread_id, .. } => thread_id.as_ref(),
        PluginOp::Start
        | PluginOp::CancelInstall
        | PluginOp::New { .. }
        | PluginOp::Check { .. }
        | PluginOp::Share { .. }
        | PluginOp::Logs { .. }
        | PluginOp::Types { .. }
        | PluginOp::List
        | PluginOp::Index { .. }
        | PluginOp::Search { .. }
        | PluginOp::Enable { .. }
        | PluginOp::Disable { .. } => None,
    }
}
