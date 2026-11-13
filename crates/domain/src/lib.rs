//! IDs, records, statuses, the `Op` the CLI and window share, and the theme file format. No GPUI.

mod diff;
mod event;
mod id;
mod notify;
mod op;
pub mod prefs;
mod preview;
mod program;
mod record;
#[cfg(test)]
mod tests;
mod text;
mod theme;
mod update;
mod usage;

pub use diff::{
    BaseBranch, DiffCommit, DiffFile, DiffFileView, DiffHunk, DiffLine, DiffLineKind, DiffList,
    DiffNote, DiffSide, DiffStatus, GitDetails, ThreadDetails, format_notes,
};
pub use event::{
    Attachment, Event, EventKind, NoticeSeverity, Role, ToolOutcome, ToolStatus, TurnOutcome,
};
pub use id::{
    AutomationId, Conversation, EnvironmentId, PendingInteractionId, PluginId, PreviewId,
    ProjectId, SavedPromptId, SectionId, ThreadId, Timestamp,
};
pub use notify::{NotifyLevel, NotifyReason, ThreadNotify};
#[cfg(unix)]
pub use op::SOCKET_PATH_MAX;
pub use op::{
    Appearance, Automation, AutomationOp, BranchList, BrowserControl, BrowserOp, BrowserSnapshot,
    BrowserView, CommandGroup, ConfigOp, DiagnosticsOp, DiffOp, DiffQuery, DiffScope, DropSide,
    EnvOp, EnvironmentOp, EnvironmentSpec, ExtraTerminal, FAILED_RUNS_LIMIT, FOLDER_IN_USE,
    GuideChapter, INSTRUCTIONS_MAX, IndexOp, InstructionsOp, KeyPlatform, Keymap, KeymapAction,
    KeymapOp, Metadata, NotifyOp, Op, OpError, OpResult, Order, OutdatedPlugin, PinnedSetup,
    PluginOp, PluginPreview, PluginPreviewOp, PreferenceOp, ProgramOp, ProjectBranch, ProjectOp,
    ProjectSection, RailAction, Run, SCREEN_MAX_BYTES, SEARCH_DEFAULT, SEARCH_LIMIT,
    SEARCH_TEXT_MAX, SHORTCUTS, SavedPromptOp, SectionOp, SelectMode, SettingsOp, SetupFile,
    Shortcut, ShortcutGroup, ShortcutScope, ShortcutWhen, Skill, SkillOp, SkillSource,
    SkillTemplate, Slot, SlotChoice, SlotChoiceError, SurfaceItem, SurfaceOp, SurfacePlacement,
    TERMINAL_COLS, TERMINAL_ROWS, TEXT_MAX, TerminalOp, ThemeOp, ThemeSettings, ThreadOp, TurnOp,
    UI_PERSON_ONLY, UI_SCREENSHOT_UNAVAILABLE, UI_UNAVAILABLE, UiBounds, UiElement, UiOp,
    UiSnapshot, UpdateTarget, WaitOutcome, WindowOp, branch_slug, check, default_chord,
    expand_secondary, invalid_json, valid_branch, valid_theme_choice, valid_theme_id,
};
pub use preview::{PreviewCut, PreviewFile, PreviewImageFormat, PreviewKind, PreviewOp};
pub use program::{
    AGENT_MARKS, KNOWN_PROGRAMS, LISTED_WHEN_MISSING, PROGRAM_INSTALL_FOLDERS, Program,
    ProgramList, ProgramSource, SAVED_NAME_MAX, SavedProgram, SplitProgramError, agent_mark,
    saved_name, split_program,
};
pub use record::{
    ASK_TEXT_MAX_CHARS, AgentTool, AskField, Environment, EnvironmentProvider, EnvironmentStatus,
    Found, GIT_SOURCE, IndexEntry, InteractionOption, Listing, LogCounts, OptionRole,
    PendingInteraction, Plugin, PluginCommand, PluginLog, Project, ProjectKind, SavedPrompt,
    Section, Sidebar, Snippet, Thread, ThreadMark, ThreadStatus, Visibility, archive_span,
    is_git_source, newest_first, order_sidebar_projects, owned_thread_ids,
};
pub use text::unsafe_char;
pub use theme::{
    Color, Highlight, Look, MAX_THEME_BYTES, Palette, Radius, Space, TerminalColors, ThemeFile,
    ThemeFileError, TypeSizes,
};
pub use update::{InstallKind, UpdateOp, UpdateState, UpdateStatus};
pub use usage::{
    UsageAccount, UsageOp, UsageProvider, UsageStatus, UsageView, UsageWindow, resets_in, unix_now,
};
