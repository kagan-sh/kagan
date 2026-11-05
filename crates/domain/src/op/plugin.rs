use std::path::PathBuf;

use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::{PluginId, ProjectId, ThreadId};

use super::{ConfigOp, IndexOp, Slot, SlotChoice};

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
        #[serde(skip)]
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
