//! The workspace: projects, threads, environments, and the `Op` dispatch that drives them.

mod automation;
mod diff;
mod dispatch;
mod end;
mod environment;
mod host;
mod instructions;
mod plugin_folder;
mod preview;
mod project;
mod saved_prompt;
mod settings;
mod skill;
#[cfg(test)]
mod tests;
mod thread;
mod update;
mod usage;
mod workspace;

pub use dispatch::ui::{UiDispatch, UiJob};
pub use kg_terminal::{Ansi, Cell, Color, Cursor, Grid, Motion, Mouse, Weight};
pub use plugin_folder::{plugin_data_dir, plugin_dir, plugin_log_file, plugins_dir};
pub use preview::{human_size, image_size, preview_image, preview_note};
pub use settings::keys::{host_os, is_known_key, parse_key};
pub use update::{Target, Updater};
#[cfg(any(test, feature = "test"))]
pub use usage::{
    advance_usage_clock, use_claude_path, use_claude_timeout, use_codex_source, use_codex_timeout,
};
pub use workspace::approval::{Approval, Requester};
pub use workspace::entity::{Environment, Project, Thread};

pub use workspace::{Permit, PluginDispatch, PluginRequest, Workspace, WorkspaceEvent};

// How long the person must stop typing before held questions show, and before a child's line
// follows their Enter. A design value, not a number from the research.
pub const TYPING_QUIET: std::time::Duration = std::time::Duration::from_millis(1500);
