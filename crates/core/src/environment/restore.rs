use std::path::Path;

use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{
    Environment, EnvironmentProvider as Provider, EnvironmentStatus, OpError, ThreadId,
};

use crate::Workspace;
use crate::host::database::Result;
use crate::workspace::{closed, missing};

use super::{git, provision};

pub(crate) async fn restore(
    this: &WeakEntity<Workspace>,
    thread_id: ThreadId,
    cx: &mut AsyncApp,
) -> Result<Environment> {
    let database = crate::workspace::database(this, cx)?;
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
    let listed = crate::host::git::git(project, &["worktree", "list", "--porcelain"])?;
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
    let head = crate::host::git::git(&path, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if head.trim() != recorded {
        return Err(OpError::Failed(format!(
            "the worktree is on {}, not {recorded}",
            head.trim()
        )));
    }
    Ok(())
}
