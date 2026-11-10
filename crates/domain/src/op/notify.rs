use clap::Subcommand;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
pub enum NotifyOp {
    /// Send one test system notification. It is sent even when notifications are off
    Test,
}
