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
    /// Show a project's page: its environment variables, default program, agent rules, and automations
    Project {
        id: ProjectId,
        /// Scroll the page to this section once it is open
        #[arg(long, value_enum)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section: Option<ProjectSection>,
    },
}

/// A part of a project's page that a request can bring into view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectSection {
    Automations,
}
