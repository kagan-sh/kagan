use std::path::PathBuf;

use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::{PluginId, PreviewId, ProjectId, ThreadId};

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
    /// Run a local plugin in a hidden kagan of its own, so this window stays free
    Preview {
        #[command(subcommand)]
        op: PluginPreviewOp,
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
    /// Update to the latest commit: one plugin, or every git plugin with --all
    Update {
        #[command(flatten)]
        target: UpdateTarget,
        /// The thread where a person approves new powers
        #[arg(long, env = "KG_THREAD_ID")]
        thread_id: Option<ThreadId>,
    },
    /// Git plugins whose pinned ref now names a newer commit
    Outdated,
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
    /// Hide a plugin's icons from the rail, or show them again. The plugin keeps working.
    Rail {
        /// The plugin id, then hide or show
        id: PluginId,
        #[arg(value_enum)]
        action: RailAction,
    },
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

/// The hidden kagan a plugin is built and driven in, beside the person's own.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum PluginPreviewOp {
    /// Start a hidden kagan with a local plugin. The person approves it in their own window
    Start {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// The thread that asks. A second start for the same plugin on this thread replaces the first
        #[arg(long, env = "KG_THREAD_ID")]
        #[serde(default)]
        thread_id: Option<ThreadId>,
    },
    /// Stop a preview and remove its folder
    Stop { id: PreviewId },
    /// Running previews
    List,
}

// `sockaddr_un.sun_path` holds the path and a trailing NUL. Linux gives the path 108 bytes;
// macOS gives it 104.
#[cfg(unix)]
pub const SOCKET_PATH_MAX: usize = if cfg!(target_os = "linux") { 107 } else { 103 };

/// A plugin preview: its id, the plugin, the folder it watches, and the two variables that point a
/// `kg` command at it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPreview {
    pub id: PreviewId,
    pub plugin: PluginId,
    pub name: String,
    pub thread_id: Option<ThreadId>,
    pub path: PathBuf,
    pub data_dir: PathBuf,
    pub socket: PathBuf,
}

/// Whether a plugin's rail icons are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum RailAction {
    Hide,
    Show,
}

/// What an update covers: one plugin, or every git plugin that can move.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateTarget {
    One(PluginId),
    All,
}

// Clap reads `<ID>` or `--all` into the enum, so an id and --all can never both reach an Op.
impl clap::Args for UpdateTarget {
    fn augment_args(cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("plugin")
                .help("The plugin to update")
                .value_name("ID")
                .value_parser(clap::value_parser!(PluginId))
                .required_unless_present("all")
                .conflicts_with("all"),
        )
        .arg(
            clap::Arg::new("all")
                .help("Update every git plugin that has a newer commit")
                .long("all")
                .action(clap::ArgAction::SetTrue)
                .conflicts_with("plugin"),
        )
    }

    fn augment_args_for_update(cmd: clap::Command) -> clap::Command {
        Self::augment_args(cmd)
    }
}

impl clap::FromArgMatches for UpdateTarget {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        if matches.get_flag("all") {
            return Ok(Self::All);
        }
        matches
            .get_one::<PluginId>("plugin")
            .cloned()
            .map(Self::One)
            .ok_or_else(|| {
                clap::Error::raw(
                    clap::error::ErrorKind::MissingRequiredArgument,
                    "give a plugin id, or --all",
                )
            })
    }

    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

/// A git plugin whose ref names a newer commit than the one kagan installed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutdatedPlugin {
    pub id: PluginId,
    pub name: String,
    pub current: String,
    pub latest: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::PluginOp;

    #[test]
    fn an_update_names_one_plugin_or_all_and_never_both() {
        let all: PluginOp =
            serde_json::from_value(json!({ "update": { "target": "all", "threadId": null } }))
                .unwrap();
        assert!(matches!(all, PluginOp::Update { .. }));
        let both = serde_json::from_value::<PluginOp>(
            json!({ "update": { "id": "echo", "all": true, "threadId": null } }),
        );
        assert!(both.is_err(), "an id and all were both accepted");
        let neither = serde_json::from_value::<PluginOp>(
            json!({ "update": { "id": null, "all": false, "threadId": null } }),
        );
        assert!(neither.is_err(), "neither an id nor all was accepted");
    }
}
