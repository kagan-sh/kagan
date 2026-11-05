use std::path::PathBuf;

use clap::Subcommand;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
pub enum DiagnosticsOp {
    /// Write a local diagnostics file. Refuses to replace an existing file.
    Export {
        /// File to write. The parent folder must exist. Omit to write under the data directory.
        #[arg(long)]
        path: Option<PathBuf>,
    },
}
