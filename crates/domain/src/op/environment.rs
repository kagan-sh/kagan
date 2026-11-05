use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::{EnvironmentId, ProjectId, ThreadId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum EnvironmentOp {
    /// Show an environment
    Get {
        #[arg(env = "KG_ENVIRONMENT_ID")]
        id: EnvironmentId,
    },
    /// Recreate a thread's environment
    Restore {
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// Local branches of a project
    Branches {
        #[arg(env = "KG_PROJECT_ID")]
        project_id: ProjectId,
    },
    /// Which branch change counts compare against
    SetBase {
        #[arg(env = "KG_ENVIRONMENT_ID")]
        id: EnvironmentId,
        base: String,
    },
    /// Remove a worktree again after git refused because of files it would keep
    Remove {
        #[arg(env = "KG_ENVIRONMENT_ID")]
        id: EnvironmentId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum EnvironmentSpec {
    ProjectDefault,
    Reuse {
        environment_id: EnvironmentId,
    },
    Worktree {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_branch: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
    Personal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchList {
    /// The checked-out branch; None when HEAD is detached
    pub current: Option<String>,
    /// The commit HEAD points at, the base a worktree starts from when no branch is checked out
    pub head: Option<String>,
    pub names: Vec<String>,
}

pub(super) fn environment_spec(text: &str) -> Result<EnvironmentSpec, String> {
    match text.split_once(':') {
        None if text == "project-default" => Ok(EnvironmentSpec::ProjectDefault),
        None if text == "personal" => Ok(EnvironmentSpec::Personal),
        None if text == "worktree" => Ok(EnvironmentSpec::Worktree {
            base_branch: None,
            branch: None,
        }),
        Some(("worktree", rest)) => {
            let (base, branch) = match rest.split_once(':') {
                Some((base, branch)) => (present(base), present(branch)),
                None => (present(rest), None),
            };
            Ok(EnvironmentSpec::Worktree {
                base_branch: base,
                branch,
            })
        }
        Some(("reuse", id)) => Ok(EnvironmentSpec::Reuse {
            environment_id: EnvironmentId(id.to_owned()),
        }),
        _ => Err(format!("unknown environment {text:?}")),
    }
}

fn present(text: &str) -> Option<String> {
    if text.is_empty() {
        None
    } else {
        Some(text.to_owned())
    }
}
