use std::path::PathBuf;

use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::ProjectId;

/// Bytes one rules file may hold: the global file, and each project's file, each up to this.
pub const INSTRUCTIONS_MAX: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum InstructionsOp {
    /// Show the agent rules: global, or one project's with --project
    Get {
        /// Project whose rules to show
        #[arg(long)]
        project: Option<ProjectId>,
    },
    /// Replace the agent rules. Give the text, or --file (`-` for stdin)
    Set {
        /// The rules text. Omit it when `--file` is set
        #[arg(required_unless_present = "file", conflicts_with = "file")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        /// Read the rules from this path, or `-` for stdin
        #[arg(long, value_name = "PATH")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file: Option<PathBuf>,
        /// Project whose rules to replace
        #[arg(long)]
        project: Option<ProjectId>,
    },
    /// Remove the agent rules
    Clear {
        /// Project whose rules to remove
        #[arg(long)]
        project: Option<ProjectId>,
    },
}
