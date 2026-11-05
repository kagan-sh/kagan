use clap::Subcommand;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsOp {
    /// Show the settings page, opening the window if it is closed
    Open,
    /// Show the plugin management section
    Plugins,
}
