use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::ProjectId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsOp {
    /// Show the settings page, opening the window if it is closed
    Open,
    /// Show the plugin management section
    Plugins,
    /// Show a project's page: its environment variables, default program, and agent rules
    Project { id: ProjectId },
}
