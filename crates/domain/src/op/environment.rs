use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::{EnvironmentId, ProjectId, ThreadId};

/// A project-folder branch change would move a checkout another unarchived thread still uses.
pub const FOLDER_IN_USE: &str = "Another thread is using this folder.";

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
    /// The project folder, after checking out an existing branch or creating one.
    ProjectOn {
        branch: ProjectBranch,
    },
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
    /// Why the project folder cannot change branches. None when it can.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkout_block: Option<String>,
}

/// A branch to check out in the project folder, or the base of a branch to create there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum ProjectBranch {
    Checkout { name: String },
    New { base: String },
}

// shortcut: a clap value parser reports the message as a String for the CLI to print.
pub(super) fn environment_spec(text: &str) -> Result<EnvironmentSpec, String> {
    match text.split_once(':') {
        None if text == "project-default" => Ok(EnvironmentSpec::ProjectDefault),
        Some(("project-on", rest)) => project_on(text, rest),
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

fn project_on(text: &str, rest: &str) -> Result<EnvironmentSpec, String> {
    let (kind, name) = rest
        .split_once(':')
        .ok_or_else(|| format!("project-on needs checkout:<branch> or new:<base>, not {text:?}"))?;
    let branch = match kind {
        "checkout" | "new" if name.is_empty() => {
            return Err(format!("project-on needs a branch, not {text:?}"));
        }
        "checkout" => ProjectBranch::Checkout {
            name: name.to_owned(),
        },
        "new" => ProjectBranch::New {
            base: name.to_owned(),
        },
        _ => return Err(format!("project-on needs checkout or new, not {text:?}")),
    };
    Ok(EnvironmentSpec::ProjectOn { branch })
}

fn present(text: &str) -> Option<String> {
    if text.is_empty() {
        None
    } else {
        Some(text.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{EnvironmentSpec, ProjectBranch, environment_spec};
    use crate::EnvironmentId;

    #[test]
    fn an_environment_word_names_its_kind() {
        assert_eq!(
            environment_spec("project-default"),
            Ok(EnvironmentSpec::ProjectDefault)
        );
        assert_eq!(
            environment_spec("project-on:checkout:topic"),
            Ok(EnvironmentSpec::ProjectOn {
                branch: ProjectBranch::Checkout {
                    name: "topic".into()
                }
            })
        );
        assert_eq!(
            environment_spec("project-on:new:main"),
            Ok(EnvironmentSpec::ProjectOn {
                branch: ProjectBranch::New {
                    base: "main".into()
                }
            })
        );
        assert_eq!(environment_spec("personal"), Ok(EnvironmentSpec::Personal));
        assert_eq!(
            environment_spec("worktree"),
            Ok(EnvironmentSpec::Worktree {
                base_branch: None,
                branch: None
            })
        );
        assert_eq!(
            environment_spec("worktree:main:thread-1"),
            Ok(EnvironmentSpec::Worktree {
                base_branch: Some("main".into()),
                branch: Some("thread-1".into())
            })
        );
        assert_eq!(
            environment_spec("worktree::thread-1"),
            Ok(EnvironmentSpec::Worktree {
                base_branch: None,
                branch: Some("thread-1".into())
            })
        );
        assert_eq!(
            environment_spec("reuse:env_1"),
            Ok(EnvironmentSpec::Reuse {
                environment_id: EnvironmentId("env_1".into())
            })
        );
    }

    #[test]
    fn an_unknown_environment_is_refused() {
        assert_eq!(
            environment_spec("elsewhere"),
            Err("unknown environment \"elsewhere\"".into())
        );
        assert_eq!(
            environment_spec("project-on:foo:"),
            Err("project-on needs checkout or new, not \"project-on:foo:\"".into())
        );
        assert_eq!(
            environment_spec("project-on:checkout:"),
            Err("project-on needs a branch, not \"project-on:checkout:\"".into())
        );
    }
}
