//! The workspace: projects, threads, environments, and the `Op` dispatch that drives them.

mod browser;
mod diff;
mod dispatch;
mod end;
mod environment;
mod host;
mod plugin_folder;
mod preview;
mod project;
mod settings;
mod skill;
#[cfg(test)]
mod tests;
mod thread;
mod update;
mod workspace;

pub use dispatch::ui::{UiDispatch, UiJob};
pub use kg_terminal::{Ansi, Cell, Color, Cursor, Grid, Motion, Mouse, Weight};
pub use plugin_folder::{plugin_data_dir, plugin_dir, plugin_log_file, plugins_dir};
pub use preview::{human_size, image_size, preview_note};
pub use settings::keys::{host_os, parse_key};
pub use update::{Target, Updater};
pub use workspace::approval::{Approval, Requester};
pub use workspace::entity::{Environment, Project, Thread};

pub use workspace::{
    Permit, PluginDispatch, PluginRequest, SocketLocation, Workspace, WorkspaceEvent,
};
