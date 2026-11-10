use std::ffi::OsString;
use std::path::{Path, PathBuf};

use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{
    Environment, EnvironmentId, EnvironmentProvider as Provider, EnvironmentSpec,
    EnvironmentStatus, OpError, Project, ProjectId, ProjectKind,
};

use crate::Workspace;
use crate::host::database::{Database, Result};
use crate::workspace::{closed, id, missing};

mod checkout;
mod maintenance;
mod personal;
mod restore;
mod script;

pub(crate) use maintenance::{
    Retirement, drop_expired_screens, finish_removal, leftovers, removal_refusal, remove_again,
    remove_folder, sweep,
};
pub(crate) use personal::{describe, personal_folders};
pub(crate) use restore::restore;
pub(crate) use script::pinned_setup;
#[cfg(all(test, unix))]
pub(crate) use script::run_script;
#[cfg(test)]
pub(crate) use script::{SCRIPT_LIMIT, SetupEnd, become_ready, shell_from};

pub(crate) enum Prepared {
    Reuse(EnvironmentId),
    Checkout(PathBuf),
    Managed(Provisioned),
}

pub(crate) struct Provisioned {
    pub(crate) environment: Environment,
    notices: Vec<String>,
}

enum ProvisionMiss {
    Discard(OpError),
    Keep(Box<(Environment, OpError)>),
}

impl From<OpError> for ProvisionMiss {
    fn from(error: OpError) -> Self {
        Self::Discard(error)
    }
}

impl From<std::io::Error> for ProvisionMiss {
    fn from(error: std::io::Error) -> Self {
        Self::Discard(error.into())
    }
}

impl ProvisionMiss {
    fn into_error(self) -> OpError {
        match self {
            Self::Discard(error) => error,
            Self::Keep(kept) => {
                let (_, error) = *kept;
                error
            }
        }
    }
}

pub(crate) async fn prepare(
    this: &WeakEntity<Workspace>,
    project: &Project,
    spec: EnvironmentSpec,
    branch: Option<String>,
    title: &str,
    cx: &mut AsyncApp,
) -> Result<Prepared> {
    let (provider, base) = match spec {
        EnvironmentSpec::Reuse { environment_id } => return Ok(Prepared::Reuse(environment_id)),
        EnvironmentSpec::ProjectDefault => {
            return Ok(Prepared::Checkout(
                canonical(project.path.clone(), cx).await?,
            ));
        }
        EnvironmentSpec::ProjectOn { branch: requested } => {
            return checkout::project_on(this, project, &requested, branch, cx).await;
        }
        EnvironmentSpec::Personal if project.kind == ProjectKind::Personal => {
            (Provider::Personal, None)
        }
        EnvironmentSpec::Personal => {
            return Err(OpError::Invalid(
                "personal environments need the personal project".into(),
            ));
        }
        EnvironmentSpec::Worktree { base_branch, .. } if project.kind == ProjectKind::Standard => {
            (Provider::Worktree, base_branch)
        }
        EnvironmentSpec::Worktree { .. } => {
            return Err(OpError::Invalid(
                "git worktrees need a standard project".into(),
            ));
        }
    };
    let (database, data_dir) = this
        .update(cx, |workspace, _| {
            (workspace.database.clone(), workspace.data_dir.clone())
        })
        .map_err(closed)?;
    // The setup and teardown scripts get the same variables as a program in this project.
    let variables = crate::host::variables::for_project(&data_dir, &project.id)?;
    if provider == Provider::Worktree
        && let Some(existing) = reusable_setup(&database, &project.id).await?
    {
        return retry_setup(this, &database, existing, &variables, cx).await;
    }
    let environment_id = EnvironmentId(id("env"));
    let branch = (provider == Provider::Worktree).then(|| branch.unwrap_or_else(|| id("kg")));
    let root = if let Some(branch) = &branch {
        data_dir
            .join("worktrees")
            .join(slug(&project.name, &project.id.0))
            .join(slug(branch, &environment_id.0))
    } else {
        data_dir
            .join("personal")
            .join(slug(title, &environment_id.0))
    };
    let environment = Environment {
        path: root,
        id: environment_id,
        project_id: project.id.clone(),
        status: EnvironmentStatus::Provisioning,
        provider,
        branch,
        base_branch: base,
        retire_at: None,
        error: None,
    };
    let environment = reserve_path(&database, environment).await?;
    Workspace::refresh(this, cx).await?;
    let project_path = project.path.clone();
    let pending = environment.clone();
    match cx
        .background_executor()
        .spawn(async move { provision(&project_path, pending, &variables) })
        .await
    {
        Ok(done) => Ok(Prepared::Managed(done)),
        Err(ProvisionMiss::Keep(kept)) => {
            let (environment, error) = *kept;
            keep_failed(this, &database, environment, error, cx).await
        }
        Err(ProvisionMiss::Discard(error)) => {
            discard_failed(this, &database, environment, error, cx).await
        }
    }
}

// Saves an environment whose setup failed, so the person can retry it, and reports why.
async fn keep_failed(
    this: &WeakEntity<Workspace>,
    database: &Database,
    environment: Environment,
    error: OpError,
    cx: &mut AsyncApp,
) -> Result<Prepared> {
    database
        .call(move |store| store.put_environment(&environment))
        .await?;
    Workspace::refresh(this, cx).await?;
    Err(error)
}

// Drops an environment whose setup cannot be retried, and reports why.
async fn discard_failed(
    this: &WeakEntity<Workspace>,
    database: &Database,
    mut environment: Environment,
    error: OpError,
    cx: &mut AsyncApp,
) -> Result<Prepared> {
    if environment.path.exists() {
        // The folder could not be removed, so keep the row for the next sweep.
        environment.status = EnvironmentStatus::Destroyed;
        environment.error = Some(error.to_string());
        database
            .call(move |store| store.put_environment(&environment))
            .await?;
    } else {
        database
            .call(move |store| store.delete_environment(&environment.id))
            .await?;
    }
    Workspace::refresh(this, cx).await?;
    Err(error)
}

async fn reserve_path(database: &Database, mut environment: Environment) -> Result<Environment> {
    database
        .call(move |store| {
            let mut occupied = Vec::new();
            for project in store.projects()? {
                occupied.extend(
                    store
                        .environments(&project.id)?
                        .into_iter()
                        .map(|env| env.path),
                );
            }
            environment.path = available_path(&environment.path, &occupied);
            store.put_environment(&environment)?;
            Ok(environment)
        })
        .await
}

async fn canonical(path: PathBuf, cx: &mut AsyncApp) -> Result<PathBuf> {
    Ok(cx
        .background_executor()
        .spawn(async move { dunce::canonicalize(path) })
        .await?)
}

fn slug(value: &str, fallback: &str) -> String {
    let slug: String = value
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(80)
        .collect();
    let slug = slug.trim_matches('-');
    let reserved = matches!(slug, "con" | "prn" | "aux" | "nul")
        || (slug.len() == 4
            && (slug.starts_with("com") || slug.starts_with("lpt"))
            && slug.ends_with(|c: char| ('1'..='9').contains(&c)));
    if slug.is_empty() || reserved {
        fallback.to_owned()
    } else {
        slug.to_owned()
    }
}

fn available_path(path: &Path, occupied: &[PathBuf]) -> PathBuf {
    let mut candidate = path.to_path_buf();
    let mut suffix = 2;
    while candidate.exists() || occupied.contains(&candidate) {
        candidate = path.with_file_name(format!(
            "{}-{suffix}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        suffix += 1;
    }
    candidate
}

// A failed setup has no thread, so the next worktree start runs the script there again.
async fn reusable_setup(database: &Database, project: &ProjectId) -> Result<Option<Environment>> {
    let project = project.clone();
    database
        .call(move |store| {
            let threads = store.threads(Some(&project), None, None, None)?;
            let mut reuse = None;
            let mut stale = Vec::new();
            for environment in store.environments(&project)? {
                if environment.provider != Provider::Worktree
                    || environment.status != EnvironmentStatus::Error
                    || environment.error.is_some()
                {
                    continue;
                }
                let used = threads
                    .iter()
                    .any(|thread| thread.environment_id.as_ref() == Some(&environment.id));
                if used {
                    continue;
                }
                if environment.path.exists() && reuse.is_none() {
                    reuse = Some(environment);
                } else if !environment.path.exists() {
                    stale.push(environment.id);
                }
            }
            for id in stale {
                store.delete_environment(&id)?;
            }
            Ok(reuse)
        })
        .await
}

async fn retry_setup(
    this: &WeakEntity<Workspace>,
    database: &Database,
    existing: Environment,
    variables: &[(OsString, OsString)],
    cx: &mut AsyncApp,
) -> Result<Prepared> {
    let variables = variables.to_vec();
    let path = existing.path.clone();
    let branch = existing.branch.clone().unwrap_or_default();
    let project_id = existing.project_id.clone();
    let leftover = existing.clone();
    match cx
        .background_executor()
        .spawn(async move { script::become_ready(existing, &variables) })
        .await
    {
        script::SetupEnd::Ready(done) => Ok(Prepared::Managed(done)),
        script::SetupEnd::Failed(kept) => {
            let (environment, error) = *kept;
            keep_failed(this, database, environment, error, cx).await
        }
        script::SetupEnd::Mismatch(error) => {
            let lookup = project_id;
            let project = database.call(move |store| store.project(&lookup)).await?;
            let doomed = path.clone();
            cx.background_executor()
                .spawn(async move {
                    if let Some(project) = project {
                        abandon_worktree(&project.path, &doomed, &branch);
                    } else if doomed.exists()
                        && let Err(error) = std::fs::remove_dir_all(&doomed)
                    {
                        eprintln!("kg: could not remove {}: {error}", doomed.display());
                    }
                })
                .await;
            discard_failed(this, database, leftover, error, cx).await
        }
    }
}

pub(crate) async fn commit(
    database: &Database,
    project: &Project,
    prepared: Prepared,
) -> Result<(Environment, Vec<String>)> {
    match prepared {
        Prepared::Reuse(id) => {
            let lookup = id.clone();
            let environment = database
                .call(move |store| store.environment(&lookup))
                .await?
                .ok_or_else(|| missing("environment", &id))?;
            if environment.project_id != project.id
                || environment.status != EnvironmentStatus::Ready
            {
                return Err(OpError::Invalid(
                    "environment must be ready and belong to this project".into(),
                ));
            }
            Ok((environment, Vec::new()))
        }
        Prepared::Checkout(path) => {
            let project_id = project.id.clone();
            database
                .call(move |store| {
                    if let Some(environment) =
                        store
                            .environments(&project_id)?
                            .into_iter()
                            .find(|environment| {
                                environment.provider == Provider::Project
                                    && environment.path == path
                                    && environment.status == EnvironmentStatus::Ready
                            })
                    {
                        return Ok((environment, Vec::new()));
                    }
                    let environment = Environment {
                        id: EnvironmentId(id("env")),
                        project_id,
                        path,
                        status: EnvironmentStatus::Ready,
                        provider: Provider::Project,
                        branch: None,
                        base_branch: None,
                        retire_at: None,
                        error: None,
                    };
                    store.put_environment(&environment)?;
                    Ok((environment, Vec::new()))
                })
                .await
        }
        Prepared::Managed(done) => {
            let saved = done.environment.clone();
            database
                .call(move |store| store.put_environment(&saved))
                .await?;
            Ok((done.environment, done.notices))
        }
    }
}

fn provision(
    project: &Path,
    mut environment: Environment,
    variables: &[(OsString, OsString)],
) -> std::result::Result<Provisioned, ProvisionMiss> {
    match environment.provider {
        Provider::Project => {}
        Provider::Personal => std::fs::create_dir_all(&environment.path)?,
        Provider::Worktree => {
            if !crate::host::git::is_repository(project) {
                return Err(OpError::Invalid(format!(
                    "{} is not a git repository, so it has no worktree; run in the project folder instead (--environment project-default)",
                    project.display()
                ))
                .into());
            }
            let requested = environment
                .branch
                .clone()
                .ok_or_else(|| OpError::Invalid("worktree has no branch".into()))?;
            if !kg_domain::valid_branch(&requested) {
                return Err(
                    OpError::Invalid(format!("branch name {requested} is not allowed")).into(),
                );
            }
            let branches = crate::host::git::branch_list(project)?;
            if let Some(base) = environment.base_branch.as_deref() {
                // A branch, or a commit when HEAD is detached.
                let known = branches.names.iter().any(|name| name == base)
                    || crate::host::git::git(
                        project,
                        &[
                            "rev-parse",
                            "--verify",
                            "--quiet",
                            &format!("{base}^{{commit}}"),
                        ],
                    )
                    .is_ok();
                if !kg_domain::valid_branch(base) || !known {
                    return Err(OpError::Invalid(format!(
                        "base {base} is not a branch or commit in this repository"
                    ))
                    .into());
                }
            } else {
                environment.base_branch = branches.current;
            }
            let base = environment
                .base_branch
                .clone()
                .unwrap_or_else(|| "HEAD".into());
            let branch = unique_branch(project, &requested)?;
            environment.branch = Some(branch.clone());
            if let Some(parent) = environment.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if let Err(error) = git(
                project,
                &["worktree", "add", "-b", &branch, "--no-track"],
                Some(&environment.path),
                Some(&base),
            ) {
                abandon_worktree(project, &environment.path, &branch);
                return Err(error.into());
            }
            return settle_setup(project, environment, variables);
        }
    }
    make_ready(&mut environment)?;
    Ok(Provisioned {
        environment,
        notices: Vec::new(),
    })
}

// Points the environment at its canonical folder and marks it ready; on failure it is unchanged.
fn make_ready(environment: &mut Environment) -> std::io::Result<()> {
    environment.path = dunce::canonicalize(&environment.path)?;
    environment.status = EnvironmentStatus::Ready;
    environment.retire_at = None;
    environment.error = None;
    Ok(())
}

fn settle_setup(
    project: &Path,
    environment: Environment,
    variables: &[(OsString, OsString)],
) -> std::result::Result<Provisioned, ProvisionMiss> {
    let path = environment.path.clone();
    let branch = environment.branch.clone().unwrap_or_default();
    match script::become_ready(environment, variables) {
        script::SetupEnd::Ready(done) => Ok(done),
        script::SetupEnd::Failed(kept) => Err(ProvisionMiss::Keep(kept)),
        script::SetupEnd::Mismatch(error) => {
            abandon_worktree(project, &path, &branch);
            Err(error.into())
        }
    }
}

fn git(project: &Path, args: &[&str], path: Option<&Path>, trailing: Option<&str>) -> Result<()> {
    let mut owned: Vec<String> = args.iter().copied().map(str::to_owned).collect();
    if path.is_some() || trailing.is_some() {
        owned.push("--".into());
    }
    if let Some(path) = path {
        owned.push(path.display().to_string());
    }
    if let Some(trailing) = trailing {
        owned.push(trailing.to_owned());
    }
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    crate::host::git::git(project, &refs).map(|_| ())
}

fn unique_branch(project: &Path, name: &str) -> Result<String> {
    let names = crate::host::git::branch_list(project)?.names;
    if !names.iter().any(|branch| branch == name) {
        return Ok(name.to_owned());
    }
    for suffix in 2..100 {
        let candidate = format!("{name}-{suffix}");
        if kg_domain::valid_branch(&candidate) && !names.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(OpError::Failed(format!("branch {name} is already used")))
}

fn abandon_worktree(project: &Path, path: &Path, branch: &str) {
    if let Err(error) = git(
        project,
        &["worktree", "remove", "--force"],
        Some(path),
        None,
    ) {
        eprintln!("kg: could not remove worktree {}: {error}", path.display());
    }
    if path.exists()
        && let Err(error) = std::fs::remove_dir_all(path)
    {
        eprintln!("kg: could not remove {}: {error}", path.display());
    }
    if let Err(error) = git(project, &["branch", "-D"], None, Some(branch)) {
        eprintln!("kg: could not delete branch {branch}: {error}");
    }
}
#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn slugs_are_safe_and_bounded() {
        assert_eq!(slug("Fix/Login: A\\B?", "env_id"), "fix-login--a-b");
        assert_eq!(slug("../", "env_id"), "env_id");
        assert_eq!(slug("CON", "env_id"), "env_id");
        assert_eq!(slug(&"A".repeat(100), "env_id").len(), 80);
    }

    #[test]
    fn paths_skip_existing_and_taken_folders() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes");
        std::fs::create_dir(&path).unwrap();
        assert_eq!(
            available_path(&path, &[dir.path().join("notes-2")]),
            dir.path().join("notes-3")
        );
    }
}
