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

#[cfg(test)]
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
    name: &str,
    cwd: &Path,
    args: &[&str],
    limit: Duration,
) -> std::result::Result<kg_process::Captured, ToolError> {
    let program = program(name, cwd).ok_or(ToolError::Missing)?;
    let mut command = Command::new(program);
    command.current_dir(cwd).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0800_0000);
    }
    kg_process::capture_with_limit(command, limit).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => ToolError::Missing,
        std::io::ErrorKind::TimedOut => ToolError::TimedOut,
        _ => ToolError::Failed(error.to_string()),
    })
}

pub(crate) fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = run("git", cwd, args, GIT_LIMIT).map_err(tool_error("git"))?;
    if output.code != 0 {
        return Err(OpError::Failed(if output.stderr.is_empty() {
            "git failed".into()
        } else {
            output.stderr
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn tool_error(name: &str) -> impl Fn(ToolError) -> OpError + '_ {
    move |error| match error {
        ToolError::Missing => OpError::Failed(format!("{name} is not installed")),
        ToolError::TimedOut => OpError::Failed(format!("{name} timed out")),
        ToolError::Failed(message) => OpError::Failed(message),
    }
}

pub(crate) fn is_repository(cwd: &Path) -> bool {
    git(cwd, &["rev-parse", "--is-inside-work-tree"]).is_ok_and(|text| text.trim() == "true")
}

pub(crate) fn current_branch(cwd: &Path) -> Option<String> {
    let text = git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).ok()?;
    let name = text.trim();
    if name.is_empty() || name == "HEAD" {
        None
    } else {
        Some(name.to_owned())
    }
}

fn is_lock_suffix(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 5 && bytes[bytes.len() - 5..].eq_ignore_ascii_case(b".lock")
}

pub(crate) fn valid_branch(name: &str) -> bool {
    let bytes = name.as_bytes();
    !name.is_empty()
        && name != "HEAD"
        && name != "@"
        && !name.starts_with('-')
        && !name.starts_with('/')
        && !name.ends_with('/')
        && !name.ends_with('.')
        && name
            .split('/')
            .all(|part| !part.starts_with('.') && !is_lock_suffix(part))
        && !name.contains("//")
        && !name.contains("..")
        && !name.contains("@{")
        && !bytes.iter().any(|byte| {
            byte.is_ascii_control()
                || matches!(byte, b' ' | b'~' | b'^' | b':' | b'\\' | b'?' | b'*' | b'[')
        })
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

pub(crate) fn branch_list(path: &Path) -> Result<BranchList> {
    if !path.exists() {
        return Err(OpError::Failed(format!("{} is gone", path.display())));
    }
    if !is_repository(path) {
        return Ok(BranchList {
            current: None,
            head: None,
            names: Vec::new(),
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
    })
}

pub(crate) async fn set_base(
    this: &WeakEntity<Workspace>,
    database: &Database,
    id: EnvironmentId,
    base: String,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    if !valid_branch(&base) {
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
    use super::valid_branch;

    #[cfg(unix)]
    #[test]
    fn git_times_out() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("git");
        std::fs::write(&script, "#!/bin/sh\nexec sleep 60\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        super::set_program(dir.path(), "git", Some(script));
        let result = super::run("git", dir.path(), &[], std::time::Duration::from_millis(20));
        super::set_program(dir.path(), "git", None);
        assert!(matches!(result, Err(super::ToolError::TimedOut)));
    }

    #[test]
    fn a_branch_name_cannot_look_like_a_flag() {
        assert!(valid_branch("1936"));
        assert!(!valid_branch("-force"));
    }
}
