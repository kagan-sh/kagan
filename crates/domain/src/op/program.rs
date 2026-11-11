use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::ProjectId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum ProgramOp {
    #[command(skip)]
    Discover {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
    /// Programs found on the PATH, the shell, the saved programs, and the recent threads. The default is the project's program when it is installed, else the first installed one
    List {
        #[arg(long = "project")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
    /// Save a program line under a name. A name already saved takes the new line and keeps its pin
    Save { name: String, line: String },
    /// Give a saved program another name
    Rename { name: String, to: String },
    /// Delete a saved program after the person approves it in kagan. The project and threads that used its line keep the line
    Remove { name: String },
    /// Pin or unpin a saved program. Pinned ones are listed first among the saved programs
    Pin {
        name: String,
        #[arg(action = clap::ArgAction::Set)]
        pinned: bool,
    },
}
