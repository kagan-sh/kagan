//! IDs, records, statuses, the `Op` the CLI and window share, and the theme file format. No GPUI.

mod diff;
mod event;
mod id;
mod op;
mod program;
mod record;
mod review;
#[cfg(test)]
mod tests;
mod theme;
mod update;

pub use diff::{
    BaseBranch, DiffCommit, DiffFile, DiffFileView, DiffHunk, DiffLine, DiffLineKind, DiffList,
    DiffNote, DiffSide, DiffStatus, GitDetails, ThreadDetails, format_notes, parse_notes,
};
pub use event::{Attachment, Event, EventKind, NoticeSeverity, Role, ToolStatus, TurnOutcome};
pub use id::{
    Conversation, EnvironmentId, PendingInteractionId, PluginId, ProjectId, SectionId, ThreadId,
    Timestamp, plain_identifier,
};
pub use op::{
    Appearance, BranchList, ConfigOp, DiagnosticsOp, DiffOp, DiffQuery, DiffScope, DropSide,
    EnvironmentOp, EnvironmentSpec, ExtraTerminal, GuideChapter, IndexOp, KeyPlatform, Keymap,
    KeymapAction, KeymapOp, Metadata, Op, OpError, OpResult, Order, PluginOp, PreferenceOp,
    ProgramOp, ProjectOp, Run, SEARCH_DEFAULT, SEARCH_LIMIT, SEARCH_TEXT_MAX, SHORTCUTS, SectionOp,
    SelectMode, SettingsOp, Shortcut, Skill, SkillOp, SkillSource, Slot, SlotChoice, SurfaceItem,
    SurfaceOp, SurfacePlacement, TERMINAL_COLS, TERMINAL_ROWS, TerminalOp, ThemeOp, ThemeSettings,
    ThreadOp, UI_PERSON_ONLY, UI_SCREENSHOT_UNAVAILABLE, UI_UNAVAILABLE, UiBounds, UiElement, UiOp,
    UiSnapshot, WaitOutcome, WindowOp, branch_slug, check, default_chord, expand_secondary,
    invalid_json, valid_theme_choice, valid_theme_id,
};
pub use program::{
    KNOWN_PROGRAMS, LISTED_WHEN_MISSING, Program, ProgramList, ProgramSource, split_program,
};
pub use record::{
    AgentTool, Environment, EnvironmentProvider, EnvironmentStatus, Found, IndexEntry,
    InteractionOption, Listing, PendingInteraction, Plugin, Project, ProjectKind, Section, Sidebar,
    Snippet, Thread, ThreadMark, ThreadStatus, Visibility, archive_span, newest_first,
    order_sidebar_projects, owned_thread_ids,
};
pub use review::{ReviewRow, review_rows};
pub use theme::{
    Color, Highlight, MAX_THEME_BYTES, Palette, TerminalColors, ThemeFile, ThemeFileError,
};
pub use update::{InstallKind, UpdateOp, UpdateState, UpdateStatus};
