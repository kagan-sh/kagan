use std::path::{Path, PathBuf};

use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{
    Environment, EnvironmentId, EnvironmentProvider as Provider, EnvironmentSpec,
    EnvironmentStatus, EventKind, NoticeSeverity, OpError, Project, ProjectId, ProjectKind,
    ThreadId,
};

use crate::Workspace;
use crate::database::{Database, Result};
use crate::workspace::{GRACE, closed, id, missing, time};

mod personal;
mod script;

pub(crate) use personal::{describe, personal_folders};
#[cfg(test)]
pub(crate) use script::{SCRIPT_LIMIT, SetupEnd, become_ready, run_script, shell_from};

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
            let path = project.path.clone();
            return Ok(Prepared::Checkout(
                cx.background_executor()
                    .spawn(async move { dunce::canonicalize(path) })
                    .await?,
            ));
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
    if provider == Provider::Worktree
        && let Some(existing) = reusable_setup(&database, &project.id).await?
    {
        return retry_setup(this, &database, existing, cx).await;
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
        .spawn(async move { provision(&project_path, pending) })
        .await
    {
        Ok(done) => Ok(Prepared::Managed(done)),
        Err(ProvisionMiss::Keep(kept)) => {
            let (environment, error) = *kept;
            let saved = environment.clone();
            database
                .call(move |store| store.put_environment(&saved))
                .await?;
            Workspace::refresh(this, cx).await?;
            Err(error)
        }
        Err(ProvisionMiss::Discard(error)) => {
            let id = environment.id.clone();
            database
                .call(move |store| store.delete_environment(&id))
                .await?;
            Workspace::refresh(this, cx).await?;
            Err(error)
        }
    }
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
    cx: &mut AsyncApp,
) -> Result<Prepared> {
    let path = existing.path.clone();
    let branch = existing.branch.clone().unwrap_or_default();
    let id = existing.id.clone();
    let project_id = existing.project_id.clone();
    match cx
        .background_executor()
        .spawn(async move { script::become_ready(existing) })
        .await
    {
        script::SetupEnd::Ready(done) => Ok(Prepared::Managed(done)),
        script::SetupEnd::Failed(kept) => {
            let (environment, error) = *kept;
            let saved = environment.clone();
            database
                .call(move |store| store.put_environment(&saved))
                .await?;
            Workspace::refresh(this, cx).await?;
            Err(error)
        }
        script::SetupEnd::Mismatch(error) => {
            let lookup = project_id.clone();
            if let Some(project) = database.call(move |store| store.project(&lookup)).await? {
                abandon_worktree(&project.path, &path, &branch);
            } else if path.exists() {
                let _ = std::fs::remove_dir_all(&path);
            }
            let deleting = id.clone();
            database
                .call(move |store| store.delete_environment(&deleting))
                .await?;
            Workspace::refresh(this, cx).await?;
            Err(error)
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
) -> std::result::Result<Provisioned, ProvisionMiss> {
    match environment.provider {
        Provider::Project => {}
        Provider::Personal => std::fs::create_dir_all(&environment.path)?,
        Provider::Worktree => {
            if !crate::changes::repository(project) {
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
            if !crate::changes::valid_branch(&requested) {
                return Err(
                    OpError::Invalid(format!("branch name {requested} is not allowed")).into(),
                );
            }
            let branches = crate::changes::branch_list(project)?;
            if let Some(base) = environment.base_branch.as_deref() {
                // A branch, or a commit when HEAD is detached.
                let known = branches.names.iter().any(|name| name == base)
                    || crate::changes::git(
                        project,
                        &[
                            "rev-parse",
                            "--verify",
                            "--quiet",
                            &format!("{base}^{{commit}}"),
                        ],
                    )
                    .is_ok();
                if !crate::changes::valid_branch(base) || !known {
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
            return settle_setup(project, environment);
        }
    }
    environment.path = dunce::canonicalize(&environment.path)?;
    environment.status = EnvironmentStatus::Ready;
    environment.retire_at = None;
    Ok(Provisioned {
        environment,
        notices: Vec::new(),
    })
}

fn settle_setup(
    project: &Path,
    environment: Environment,
) -> std::result::Result<Provisioned, ProvisionMiss> {
    let path = environment.path.clone();
    let branch = environment.branch.clone().unwrap_or_default();
    match script::become_ready(environment) {
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
    crate::changes::git(project, &refs).map(|_| ())
}

fn unique_branch(project: &Path, name: &str) -> Result<String> {
    let names = crate::changes::branch_list(project)?.names;
    if !names.iter().any(|branch| branch == name) {
        return Ok(name.to_owned());
    }
    for suffix in 2..100 {
        let candidate = format!("{name}-{suffix}");
        if crate::changes::valid_branch(&candidate) && !names.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(OpError::Failed(format!("branch {name} is already used")))
}

fn abandon_worktree(project: &Path, path: &Path, branch: &str) {
    let _ = git(
        project,
        &["worktree", "remove", "--force"],
        Some(path),
        None,
    );
    if path.exists() {
        let _ = std::fs::remove_dir_all(path);
    }
    let _ = git(project, &["branch", "-D"], None, Some(branch));
}

pub(crate) async fn restore(
    this: &WeakEntity<Workspace>,
    thread_id: ThreadId,
    cx: &mut AsyncApp,
) -> Result<Environment> {
    let database = this
        .update(cx, |workspace, _| workspace.database.clone())
        .map_err(closed)?;
    let lookup = thread_id.clone();
    let record = database
        .call(move |store| store.thread(&lookup))
        .await?
        .ok_or_else(|| missing("thread", &thread_id))?;
    let environment_id = record
        .environment_id
        .ok_or_else(|| OpError::Invalid("thread has no environment".into()))?;
    if this
        .update(cx, |workspace, _| {
            workspace.removing.contains(&environment_id)
        })
        .map_err(closed)?
    {
        return Err(OpError::Invalid(
            "environment is being removed; restore again when removal finishes".into(),
        ));
    }
    let lookup = environment_id.clone();
    let (environment, project, others) = database
        .call(move |store| {
            let environment = store.environment(&lookup)?;
            let (project, others) = match &environment {
                Some(environment) => (
                    store.project(&environment.project_id)?,
                    store.environments(&environment.project_id)?,
                ),
                None => (None, Vec::new()),
            };
            Ok((environment, project, others))
        })
        .await?;
    let environment = environment.ok_or_else(|| missing("environment", &environment_id))?;
    if environment.status == EnvironmentStatus::Ready {
        return Ok(environment);
    }
    let project = project.ok_or_else(|| missing("project", &environment.project_id))?;
    let ready = cx
        .background_executor()
        .spawn(async move {
            match environment.provider {
                // A worktree git refused to remove still holds the user's edits.
                Provider::Worktree if environment.path.exists() => {
                    check_kept_worktree(&project.path, &environment, &others)?;
                }
                Provider::Worktree => {
                    let branch = environment
                        .branch
                        .as_deref()
                        .ok_or_else(|| OpError::Invalid("worktree has no branch".into()))?;
                    git(&project.path, &["worktree", "prune"], None, None)?;
                    git(
                        &project.path,
                        &["worktree", "add"],
                        Some(&environment.path),
                        Some(branch),
                    )?;
                }
                Provider::Project | Provider::Personal => {
                    return match provision(&project.path, environment) {
                        Ok(done) => Ok(done.environment),
                        Err(error) => Err(error.into_error()),
                    };
                }
            }
            let mut environment = environment;
            environment.path = dunce::canonicalize(&environment.path)?;
            environment.status = EnvironmentStatus::Ready;
            environment.retire_at = None;
            environment.error = None;
            Ok(environment)
        })
        .await?;
    let saved = ready.clone();
    database
        .call(move |store| store.put_environment(&saved))
        .await?;
    Ok(ready)
}

fn check_kept_worktree(
    project: &Path,
    environment: &Environment,
    others: &[Environment],
) -> Result<()> {
    let path = dunce::canonicalize(&environment.path)?;
    let listed = crate::changes::git(project, &["worktree", "list", "--porcelain"])?;
    let is_worktree = listed
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .any(|listed| dunce::canonicalize(listed).is_ok_and(|listed| listed == path));
    if !is_worktree {
        return Err(OpError::Failed(format!(
            "{} is no longer a worktree of this project",
            path.display()
        )));
    }
    if let Some(other) = others.iter().find(|other| {
        other.id != environment.id
            && other.status != EnvironmentStatus::Destroyed
            && dunce::canonicalize(&other.path).is_ok_and(|other| other == path)
    }) {
        return Err(OpError::Failed(format!(
            "{} belongs to environment {}",
            path.display(),
            other.id.0
        )));
    }
    let recorded = environment
        .branch
        .as_deref()
        .ok_or_else(|| OpError::Invalid("worktree has no branch".into()))?;
    let head = crate::changes::git(&path, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if head.trim() != recorded {
        return Err(OpError::Failed(format!(
            "the worktree is on {}, not {recorded}",
            head.trim()
        )));
    }
    Ok(())
}

pub(crate) struct Retirement {
    pub(crate) environment: Environment,
    pub(crate) project: PathBuf,
}

pub(crate) async fn sweep(
    this: &WeakEntity<Workspace>,
    cx: &mut AsyncApp,
) -> Result<(bool, Vec<Retirement>)> {
    let database = this
        .update(cx, |workspace, _| workspace.database.clone())
        .map_err(closed)?;
    let (records, environments) = database
        .call(|store| {
            let mut environments = Vec::new();
            for project in store.projects()? {
                for environment in store.environments(&project.id)? {
                    environments.push((environment, project.path.clone()));
                }
            }
            Ok((store.threads(None, None, None, None)?, environments))
        })
        .await?;
    let at = time(this, cx)?;
    let mut changed = false;
    let mut drop_screens = Vec::new();
    for record in &records {
        if record
            .archived_at
            .is_some_and(|archived| at.0 >= archived.0.saturating_add(GRACE))
        {
            let thread = this
                .update(cx, |workspace, _| {
                    crate::thread::release_ended(workspace, &record.id);
                    drop_screens.push(record.id.clone());
                    workspace.threads.get(&record.id).cloned()
                })
                .map_err(closed)?;
            if let Some(thread) = thread
                && thread.read_with(cx, |thread, _| thread.running())
            {
                crate::thread::retire(this, &thread, cx).await?;
                changed = true;
            }
        }
    }
    let mut retirements = Vec::new();
    for (mut environment, project) in environments {
        if environment.status != EnvironmentStatus::Ready || !environment.provider.managed() {
            continue;
        }
        if environment.provider == Provider::Personal {
            if environment.retire_at.take().is_some() {
                let saved = environment.clone();
                database
                    .call(move |store| store.put_environment(&saved))
                    .await?;
                changed = true;
            }
            continue;
        }
        let in_use = records.iter().any(|thread| {
            thread.environment_id.as_ref() == Some(&environment.id) && thread.archived_at.is_none()
        });
        match environment.retire_at {
            Some(_) if in_use => environment.retire_at = None,
            None if in_use => continue,
            None => environment.retire_at = Some(kg_domain::Timestamp(at.0.saturating_add(GRACE))),
            Some(retire) if retire <= at => environment.status = EnvironmentStatus::Destroyed,
            Some(_) => continue,
        }
        let saved = environment.clone();
        database
            .call(move |store| store.put_environment(&saved))
            .await?;
        changed = true;
        if environment.status == EnvironmentStatus::Destroyed {
            retirements.push(Retirement {
                environment,
                project,
            });
        }
    }
    if !drop_screens.is_empty() {
        database
            .call(move |store| {
                for id in &drop_screens {
                    store.delete_screen(id)?;
                }
                Ok(())
            })
            .await?;
    }
    Ok((changed, retirements))
}

pub(crate) async fn leftovers(
    this: &WeakEntity<Workspace>,
    cx: &mut AsyncApp,
) -> Result<Vec<Retirement>> {
    let (database, removing) = this
        .update(cx, |workspace, _| {
            (workspace.database.clone(), workspace.removing.clone())
        })
        .map_err(closed)?;
    let destroyed = database
        .call(move |store| {
            let threads = store.threads(None, None, None, None)?;
            let mut destroyed = Vec::new();
            for project in store.projects()? {
                for environment in store.environments(&project.id)? {
                    let kept = environment.provider == Provider::Personal
                        && threads
                            .iter()
                            .any(|thread| thread.environment_id.as_ref() == Some(&environment.id));
                    if environment.status == EnvironmentStatus::Destroyed
                        && !removing.contains(&environment.id)
                        && environment.provider.managed()
                        && !kept
                    {
                        destroyed.push(Retirement {
                            environment,
                            project: project.path.clone(),
                        });
                    }
                }
            }
            Ok(destroyed)
        })
        .await?;
    Ok(cx
        .background_executor()
        .spawn(async move {
            destroyed
                .into_iter()
                .filter(|retirement| retirement.environment.path.exists())
                .collect()
        })
        .await)
}

pub(crate) async fn remove_again(
    this: &WeakEntity<Workspace>,
    id: EnvironmentId,
    cx: &mut AsyncApp,
) -> Result<kg_domain::OpResult> {
    let database = this
        .update(cx, |workspace, _| workspace.database.clone())
        .map_err(closed)?;
    let lookup = id.clone();
    let found = database
        .call(move |store| {
            let environment = store.environment(&lookup)?;
            let project = environment
                .as_ref()
                .map(|environment| store.project(&environment.project_id))
                .transpose()?;
            Ok((environment, project.flatten()))
        })
        .await?;
    let Some(mut environment) = found.0 else {
        return Err(missing("environment", &id));
    };
    let Some(project) = found.1 else {
        return Err(missing("project", &environment.project_id));
    };
    if environment.provider != Provider::Worktree
        || environment.status != EnvironmentStatus::Error
        || environment.error.is_none()
    {
        if environment.provider == Provider::Worktree
            && (environment.status == EnvironmentStatus::Destroyed || !environment.path.exists())
        {
            return Ok(kg_domain::OpResult::Done);
        }
        return Err(OpError::Invalid(
            "this worktree is not waiting to be removed".into(),
        ));
    }
    environment.status = EnvironmentStatus::Destroyed;
    environment.retire_at = None;
    let saved = environment.clone();
    database
        .call(move |store| store.put_environment(&saved))
        .await?;
    this.update(cx, |workspace, _| {
        workspace.removing.insert(id);
    })
    .map_err(closed)?;
    let outcome = remove_folder(
        this,
        Retirement {
            environment: environment.clone(),
            project: project.path,
        },
        cx,
    )
    .await;
    let refusal = outcome
        .as_ref()
        .err()
        .map(|error| removal_refusal(&environment.path, error, RETRY));
    let result = finish_removal(this, environment, outcome, cx).await?;
    Workspace::refresh(this, cx).await?;
    if let Some(reason) = refusal {
        return Err(OpError::Failed(reason));
    }
    Ok(result)
}

pub(crate) async fn remove_folder(
    this: &WeakEntity<Workspace>,
    retirement: Retirement,
    cx: &mut AsyncApp,
) -> Result<()> {
    let Retirement {
        environment,
        project,
    } = retirement;
    let environment_id = environment.id.clone();
    let provider = environment.provider;
    let path = environment.path.clone();
    let (outcome, notices) = cx
        .background_executor()
        .spawn(async move {
            let mut notices = Vec::new();
            let outcome = match provider {
                Provider::Personal if path.exists() => {
                    std::fs::remove_dir_all(&path).map_err(OpError::from)
                }
                Provider::Worktree if path.exists() => {
                    // A teardown failure still attempts removal, and never with --force.
                    if let Ok(found) = script::teardown(&path) {
                        notices = found;
                    }
                    git(&project, &["worktree", "remove"], Some(&path), None)
                }
                Provider::Project | Provider::Personal | Provider::Worktree => Ok(()),
            };
            (outcome, notices)
        })
        .await;
    if !notices.is_empty() {
        tell_removal(this, &environment_id, notices, cx).await;
    }
    outcome
}

async fn tell_removal(
    this: &WeakEntity<Workspace>,
    id: &EnvironmentId,
    notices: Vec<String>,
    cx: &mut AsyncApp,
) {
    let Ok(database) = this.update(cx, |workspace, _| workspace.database.clone()) else {
        return;
    };
    let lookup = id.clone();
    let Ok(thread) = database
        .call(move |store| {
            Ok(store
                .threads(None, None, None, None)?
                .into_iter()
                .find(|thread| thread.environment_id.as_ref() == Some(&lookup))
                .map(|thread| thread.id))
        })
        .await
    else {
        return;
    };
    let Some(thread) = thread else {
        return;
    };
    let kinds = notices
        .into_iter()
        .map(|text| EventKind::Notice {
            severity: NoticeSeverity::Warning,
            text,
        })
        .collect();
    let _ = Workspace::events(this, thread, kinds, cx).await;
}

pub(crate) async fn finish_removal(
    this: &WeakEntity<Workspace>,
    mut environment: Environment,
    outcome: Result<()>,
    cx: &mut AsyncApp,
) -> Result<kg_domain::OpResult> {
    let id = environment.id.clone();
    this.update(cx, |workspace, _| {
        workspace.removing.remove(&id);
    })
    .map_err(closed)?;
    let database = this
        .update(cx, |workspace, _| workspace.database.clone())
        .map_err(closed)?;
    if let Err(error) = &outcome {
        let reason = removal_refusal(&environment.path, error, RETRY);
        let lookup = id.clone();
        database
            .call(move |store| {
                let Some(current) = store.environment(&lookup)? else {
                    return Ok(());
                };
                if current.status != EnvironmentStatus::Destroyed {
                    return Ok(());
                }
                environment.status = EnvironmentStatus::Error;
                environment.retire_at = None;
                environment.error = Some(reason);
                store.put_environment(&environment)
            })
            .await?;
    } else if environment.error.is_some() {
        let lookup = id.clone();
        database
            .call(move |store| {
                let Some(mut current) = store.environment(&lookup)? else {
                    return Ok(());
                };
                current.status = EnvironmentStatus::Destroyed;
                current.error = None;
                current.retire_at = None;
                store.put_environment(&current)
            })
            .await?;
    } else if environment.provider == Provider::Personal {
        // Its folder is gone, so the record goes too.
        database
            .call(move |store| store.delete_environment(&id))
            .await?;
    }
    Ok(kg_domain::OpResult::Done)
}

const NAMED_FILES: usize = 5;
const RETRY: &str = "Commit, delete, or git-ignore them, then remove the worktree again.";

pub(crate) fn removal_refusal(root: &Path, error: &OpError, next: &str) -> String {
    let raw = error.to_string();
    if !raw.contains("modified or untracked") {
        return raw;
    }
    let names = crate::changes::git(root, &["status", "--porcelain", "-uall"])
        .ok()
        .map(|text| {
            text.lines()
                .filter_map(|line| {
                    let rest = line.get(3..)?.trim();
                    let path = rest
                        .rsplit_once(" -> ")
                        .map_or(rest, |(_, path)| path)
                        .trim()
                        .trim_matches('"');
                    (!path.is_empty()).then(|| path.to_owned())
                })
                .collect::<Vec<_>>()
        });
    let Some(names) = names.as_deref().filter(|names| !names.is_empty()) else {
        return format!("The worktree still has files git would keep. {next}");
    };
    let mut listed = names
        .iter()
        .take(NAMED_FILES)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let extra = names.len().saturating_sub(NAMED_FILES);
    if extra > 0 {
        listed = format!("{listed}, and {extra} more");
    }
    format!("The worktree still has files git would keep: {listed}. {next}")
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
