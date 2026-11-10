use std::path::PathBuf;

use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::ProjectId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum ProjectOp {
    /// List projects
    List,
    /// Show a project
    Get {
        #[arg(env = "KG_PROJECT_ID")]
        id: ProjectId,
    },
    /// Add a project for a folder
    Add { path: PathBuf },
    /// Remove a project and its threads
    Remove {
        /// Project to remove
        id: ProjectId,
    },
    /// Set the program a new thread in a project starts with; omit the program to clear it
    SetProgram {
        id: ProjectId,
        /// The command line, as `kg program list` shows it
        program: Option<String>,
    },
    /// Move a project in the sidebar
    Reorder {
        id: ProjectId,
        /// Place before this project; omit to move to the end, still above Personal
        #[arg(long)]
        before: Option<ProjectId>,
    },
}
