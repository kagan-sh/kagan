#[cfg(test)]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(test)]
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{
    BranchList, Environment, EnvironmentId, EnvironmentStatus, OpError, OpResult, ProjectId,
};

use crate::Workspace;
use crate::host::database::{Database, Result};
use crate::workspace::missing;

pub(crate) const GIT_LIMIT: Duration = Duration::from_secs(20);

enum ToolError {
    Missing,
    TimedOut,
    Failed(String),
}

#[cfg(test)]
type Programs = HashMap<(PathBuf, String), PathBuf>;

#[cfg(test)]
fn overrides() -> &'static Mutex<Programs> {
    static CELL: OnceLock<Mutex<Programs>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(all(test, unix))]
pub(crate) fn set_program(cwd: &Path, name: &str, path: Option<PathBuf>) {
    let mut map = overrides()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match path {
        Some(path) => {
            map.insert((cwd.to_owned(), name.to_owned()), path);
        }
        None => {
            map.remove(&(cwd.to_owned(), name.to_owned()));
        }
    }
}

#[cfg(test)]
fn program(name: &str, cwd: &Path) -> Option<PathBuf> {
    if let Ok(map) = overrides().lock()
        && let Some(path) = map.get(&(cwd.to_owned(), name.to_owned()))
    {
        return Some(path.clone());
    }
    find_program(name)
}

#[cfg(not(test))]
fn program(name: &str, _: &Path) -> Option<PathBuf> {
    find_program(name)
}

fn find_program(name: &str) -> Option<PathBuf> {
    kg_process::find_program(
        Path::new(name),
        kg_process::user_path().as_deref(),
        kg_process::pathext().as_deref(),
    )
}

fn run(
    cwd: &Path,
    args: &[&str],
    index: Option<&Path>,
    limit: Duration,
) -> std::result::Result<kg_process::Captured, ToolError> {
    let program = program("git", cwd).ok_or(ToolError::Missing)?;
    let mut command = Command::new(program);
    command.current_dir(cwd).args(args);
    command.env("GIT_TERMINAL_PROMPT", "0");
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    kg_process::capture(command, limit, || false).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => ToolError::Missing,
        std::io::ErrorKind::TimedOut => ToolError::TimedOut,
        _ => ToolError::Failed(error.to_string()),
    })
}

/// Runs git in `cwd` and returns what it printed, even when it exits non-zero.
pub(crate) fn git_captured(
    cwd: &Path,
    args: &[&str],
    index: Option<&Path>,
) -> Result<kg_process::Captured> {
    run(cwd, args, index, GIT_LIMIT).map_err(|error| match error {
        ToolError::Missing => OpError::Failed("git is not installed".into()),
        ToolError::TimedOut => OpError::Failed("git timed out".into()),
        ToolError::Failed(message) => OpError::Failed(message),
    })
}

pub(crate) fn git_bytes(cwd: &Path, args: &[&str], index: Option<&Path>) -> Result<Vec<u8>> {
    let output = git_captured(cwd, args, index)?;
    if output.code == 0 {
        Ok(output.stdout)
    } else if output.stderr.is_empty() {
        Err(OpError::Failed(format!("git failed ({})", output.code)))
    } else {
        Err(OpError::Failed(output.stderr))
    }
}

pub(crate) fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8_lossy(&git_bytes(cwd, args, None)?).into_owned())
}

pub(crate) fn is_repository(cwd: &Path) -> bool {
    git(cwd, &["rev-parse", "--is-inside-work-tree"]).is_ok_and(|text| text.trim() == "true")
}

pub(crate) fn current_branch(cwd: &Path) -> Option<String> {
    // `rev-parse --abbrev-ref HEAD` is empty when a branch named `head` exists on a
    // case-insensitive checkout, and it fails on an unborn branch. `branch --show-current`
    // still prints the branch that is checked out.
    let name = git(cwd, &["branch", "--show-current"]).ok()?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

pub(crate) async fn branches(
    database: &Database,
    id: ProjectId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let lookup = id.clone();
    let project = database
        .call(move |store| store.project(&lookup))
        .await?
        .ok_or_else(|| missing("project", &id))?;
    let path = project.path;
    let list = cx
        .background_executor()
        .spawn(async move { branch_list(&path) })
        .await?;
    Ok(OpResult::Branches(list))
}

pub(crate) async fn setup(
    database: &Database,
    id: ProjectId,
    rev: String,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let lookup = id.clone();
    let project = database
        .call(move |store| store.project(&lookup))
        .await?
        .ok_or_else(|| missing("project", &id))?;
    let path = project.path;
    let setup = cx
        .background_executor()
        .spawn(async move { crate::environment::pinned_setup(&path, &rev) })
        .await;
    Ok(OpResult::Setup(setup))
}

pub(crate) fn branch_list(path: &Path) -> Result<BranchList> {
    if !path.exists() {
        return Err(OpError::Failed(format!("{} is gone", path.display())));
    }
    if !is_repository(path) {
        return Ok(BranchList {
            current: None,
            head: None,
            names: Vec::new(),
            checkout_block: None,
        });
    }
    let current = current_branch(path);
    let head = git(path, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());
    let text = git(
        path,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )?;
    let mut names: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    names.sort();
    names.dedup();
    Ok(BranchList {
        current,
        head,
        names,
        checkout_block: checkout_block(path),
    })
}

/// Why `path` cannot create or switch branches. None when it can, or when it is not a repository.
pub(crate) fn checkout_block(path: &Path) -> Option<String> {
    if !is_repository(path) {
        return None;
    }
    if let Some(message) = operation_block(path) {
        return Some(message);
    }
    if dirty(path) {
        return Some(
            "Commit or stash the uncommitted changes in this checkout to create or switch branches."
                .into(),
        );
    }
    if current_branch(path).is_none() {
        let unborn = git(path, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_err();
        return Some(if unborn {
            "Create the first commit in this checkout to create or switch branches.".into()
        } else {
            "HEAD is detached. Check out a branch here before creating or switching branches."
                .into()
        });
    }
    None
}

fn dirty(path: &Path) -> bool {
    git(path, &["status", "--porcelain"]).is_ok_and(|text| !text.trim().is_empty())
}

fn git_path_exists(path: &Path, name: &str) -> bool {
    let Ok(text) = git(path, &["rev-parse", "--git-path", name]) else {
        return false;
    };
    let rel = text.trim();
    if rel.is_empty() {
        return false;
    }
    let candidate = PathBuf::from(rel);
    let full = if candidate.is_absolute() {
        candidate
    } else {
        path.join(candidate)
    };
    full.exists()
}

fn operation_block(path: &Path) -> Option<String> {
    if git_path_exists(path, "rebase-merge") || git_path_exists(path, "rebase-apply") {
        return Some("Finish or abort the rebase before changing branches.".into());
    }
    if git_path_exists(path, "CHERRY_PICK_HEAD") {
        return Some("Finish or abort the cherry-pick before changing branches.".into());
    }
    if git_path_exists(path, "REVERT_HEAD") {
        return Some("Finish or abort the revert before changing branches.".into());
    }
    if git_path_exists(path, "MERGE_HEAD") {
        let conflicts = git(path, &["diff", "--name-only", "--diff-filter=U"])
            .is_ok_and(|text| !text.trim().is_empty());
        return Some(if conflicts {
            "Resolve the conflicts and finish or abort the merge before changing branches.".into()
        } else {
            "Finish or abort the merge before changing branches.".into()
        });
    }
    None
}

/// Whether applying `branch` would move HEAD.
pub(crate) fn branch_moves(path: &Path, branch: &kg_domain::ProjectBranch) -> bool {
    match branch {
        kg_domain::ProjectBranch::Checkout { name } => {
            current_branch(path).as_deref() != Some(name.as_str())
        }
        kg_domain::ProjectBranch::New { .. } => true,
    }
}

/// Checks out `branch` in `path`, or creates one from its base. A checkout of the current branch
/// does nothing, including when the folder is dirty. `occupied` is reported only after the git
/// reasons, and only when HEAD would move.
pub(crate) fn place_project_branch(
    path: &Path,
    branch: &kg_domain::ProjectBranch,
    new_name: Option<&str>,
    occupied: bool,
) -> Result<()> {
    match branch {
        kg_domain::ProjectBranch::Checkout { name } => {
            if !kg_domain::valid_branch(name) {
                return Err(refused_name(name));
            }
            if !branch_moves(path, branch) {
                return Ok(());
            }
            if let Some(message) = checkout_block(path) {
                return Err(OpError::Invalid(message));
            }
            if !local_branch(path, name) {
                return Err(missing_branch(name));
            }
            if occupied {
                return Err(OpError::Invalid(kg_domain::FOLDER_IN_USE.into()));
            }
            git(path, &["switch", name])?;
        }
        kg_domain::ProjectBranch::New { base } => {
            let Some(name) = new_name else {
                return Err(OpError::Invalid("branch name is empty".into()));
            };
            if !kg_domain::valid_branch(name) {
                return Err(refused_name(name));
            }
            if !kg_domain::valid_branch(base) {
                return Err(refused_name(base));
            }
            if let Some(message) = checkout_block(path) {
                return Err(OpError::Invalid(message));
            }
            if local_branch(path, name) {
                return Err(OpError::Invalid(format!("branch {name} already exists")));
            }
            if !local_branch(path, base) {
                return Err(missing_branch(base));
            }
            if occupied {
                return Err(OpError::Invalid(kg_domain::FOLDER_IN_USE.into()));
            }
            git(path, &["switch", "-c", name, base])?;
        }
    }
    Ok(())
}

fn local_branch(path: &Path, name: &str) -> bool {
    let spec = format!("refs/heads/{name}");
    git(path, &["show-ref", "--verify", "--quiet", &spec]).is_ok()
}

fn refused_name(name: &str) -> OpError {
    OpError::Invalid(format!("branch name {name} is not allowed"))
}

fn missing_branch(name: &str) -> OpError {
    OpError::Invalid(format!("branch {name} is not in this repository"))
}

pub(crate) async fn set_base(
    this: &WeakEntity<Workspace>,
    database: &Database,
    id: EnvironmentId,
    base: String,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    if !kg_domain::valid_branch(&base) {
        return Err(OpError::Invalid(format!(
            "base branch {base} is not allowed"
        )));
    }
    let mut environment = load(database, &id).await?;
    if environment.status != EnvironmentStatus::Destroyed && environment.path.exists() {
        let path = environment.path.clone();
        let known = cx
            .background_executor()
            .spawn(async move { branch_list(&path) })
            .await?;
        if !known.names.iter().any(|branch| branch == &base) {
            return Err(OpError::Invalid(format!(
                "base branch {base} is not in this repository"
            )));
        }
    }
    environment.base_branch = Some(base);
    let saved = environment.clone();
    database
        .call(move |store| store.put_environment(&saved))
        .await?;
    Workspace::refresh(this, cx).await?;
    Ok(OpResult::Environment(environment))
}

async fn load(database: &Database, id: &EnvironmentId) -> Result<Environment> {
    let lookup = id.clone();
    database
        .call(move |store| store.environment(&lookup))
        .await?
        .ok_or_else(|| missing("environment", id))
}

#[cfg(test)]
mod tests {
    use kg_domain::valid_branch;

    #[cfg(unix)]
    #[test]
    fn git_times_out() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("git");
        std::fs::write(&script, "#!/bin/sh\nexec sleep 60\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        super::set_program(dir.path(), "git", Some(script));
        let result = super::run(dir.path(), &[], None, std::time::Duration::from_millis(20));
        super::set_program(dir.path(), "git", None);
        assert!(matches!(result, Err(super::ToolError::TimedOut)));
    }

    #[test]
    fn a_branch_name_cannot_look_like_a_flag() {
        assert!(valid_branch("1936"));
        assert!(!valid_branch("-force"));
        assert!(!valid_branch("head"));
        assert!(!valid_branch("HEAD"));
    }
}
