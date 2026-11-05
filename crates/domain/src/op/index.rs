use clap::Subcommand;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
pub enum IndexOp {
    /// Add an index: `git:<url>@<ref>`, or a file or folder on this computer
    Add { location: String },
    /// The indexes, first searched first
    List,
    /// Remove an index
    Remove { location: String },
}
