use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{
    Environment, EnvironmentId, EnvironmentProvider as Provider, EnvironmentStatus, EventKind,
    NoticeSeverity, OpError, ProjectId,
};

use crate::Workspace;
use crate::host::database::Result;
use crate::workspace::{closed, missing, time};

use super::{git, personal, script};

const GRACE: i64 = 10_000;

pub(crate) struct Retirement {
    pub(crate) environment: Environment,
    pub(crate) project: PathBuf,
}

// Called once at startup, for saved screens that no screen in memory released. Later sweeps
// delete only what they release, so an idle workspace does no screen work.
pub(crate) async fn drop_expired_screens(
    this: &WeakEntity<Workspace>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let database = crate::workspace::database(this, cx)?;
    let archived_before = time(this, cx)?.0.saturating_sub(GRACE);
    database
        .call(move |store| store.delete_archived_screens(archived_before))
        .await
}

pub(crate) async fn sweep(
    this: &WeakEntity<Workspace>,
    cx: &mut AsyncApp,
) -> Result<(bool, Vec<Retirement>)> {
    let database = crate::workspace::database(this, cx)?;
    let (records, environments) = database
        .call(|store| {
            let paths: BTreeMap<ProjectId, PathBuf> = store
                .projects()?
                .into_iter()
                .map(|project| (project.id, project.path))
                .collect();
            let environments: Vec<(Environment, PathBuf)> = store
                .all_environments()?
                .into_iter()
                .filter_map(|environment| {
                    let path = paths.get(&environment.project_id)?.clone();
                    Some((environment, path))
                })
                .collect();
            Ok((store.all_threads()?, environments))
        })
        .await?;
    let at = time(this, cx)?;
    let mut changed = false;
    let archived_before = at.0.saturating_sub(GRACE);
    let (retiring, dropped) = this
        .update(cx, |workspace, cx| {
            let mut retiring = Vec::new();
            let mut dropped = false;
            for record in &records {
                if record
                    .archived_at
                    .is_none_or(|archived| archived.0 > archived_before)
                {
                    continue;
                }
                if let Some(run) = workspace.runs.get_mut(&record.id) {
                    dropped |= matches!(run.shown, Some(crate::thread::Shown::Ended(_)));
                    if run.release() {
                        cx.emit(crate::WorkspaceEvent::Changed);
                    }
                }
                if let Some(thread) = workspace.threads.get(&record.id)
                    && thread.read(cx).running()
                {
                    retiring.push(thread.clone());
                }
            }
            (retiring, dropped)
        })
        .map_err(closed)?;
    for thread in retiring {
        crate::thread::retire(this, &thread, cx).await?;
        changed = true;
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
    if dropped {
        database
            .call(move |store| store.delete_archived_screens(archived_before))
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
            let threads = store.all_threads()?;
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
    let database = crate::workspace::database(this, cx)?;
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
    let data_dir = this
        .update(cx, |workspace, _| workspace.data_dir.clone())
        .map_err(closed)?;
    // A variables file that cannot be read must not block a removal, so the teardown runs without them.
    let variables =
        crate::host::variables::for_project(&data_dir, &environment.project_id).unwrap_or_default();
    let (outcome, notices) = cx
        .background_executor()
        .spawn(async move {
            let mut notices = Vec::new();
            let outcome = match provider {
                Provider::Personal if path.exists() => {
                    personal::remove(&path).map_err(OpError::from)
                }
                Provider::Worktree if path.exists() => {
                    // A teardown failure still attempts removal, and never with --force.
                    if let Ok(found) = script::teardown(&path, &variables) {
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
                .all_threads()?
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
    if let Err(error) = Workspace::events(this, thread, kinds, cx).await {
        eprintln!("kg: could not record the removal notice: {error}");
    }
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
    let database = crate::workspace::database(this, cx)?;
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
    let names = crate::host::git::git(root, &["status", "--porcelain", "-uall"])
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
