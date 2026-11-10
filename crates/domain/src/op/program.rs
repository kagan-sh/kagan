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
    /// Programs found on the PATH, the shell, and the recent threads. The default is the project's program when it is installed, else the first installed one
    List {
        #[arg(long = "project")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
}
