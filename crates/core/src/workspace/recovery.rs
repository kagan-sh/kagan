use std::collections::BTreeSet;

use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{EventKind, OpResult, ThreadId, ThreadStatus};

use crate::host::database::Result;
use crate::thread::clear_pending;
use crate::workspace::clock::{closed, now};
use kg_store::Store;

use super::{Request, Workspace};

impl Workspace {
    pub(super) async fn sweep(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> Result<OpResult> {
        let (changed, retirements) = crate::environment::sweep(this, cx).await?;
        if changed {
            Self::refresh(this, cx).await?;
        }
        Self::remove(this, retirements, cx)?;
        Ok(OpResult::Done)
    }
    pub(super) async fn leftovers(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> Result<OpResult> {
        let leftovers = crate::environment::leftovers(this, cx).await?;
        Self::remove(this, leftovers, cx)?;
        Ok(OpResult::Done)
    }
    pub(crate) fn remove(
        this: &WeakEntity<Self>,
        retirements: Vec<crate::environment::Retirement>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        for retirement in retirements {
            let environment = retirement.environment.clone();
            this.update(cx, |workspace, cx| {
                workspace.removing.insert(environment.id.clone());
                cx.spawn(async move |this, cx| {
                    crate::workspace::hold(&this, crate::workspace::Hold::Removal, cx).await;
                    let outcome = crate::environment::remove_folder(&this, retirement, cx).await;
                    let submitted = this
                        .update(cx, |workspace, _| {
                            workspace.submit(Request::RemovalDone {
                                environment,
                                outcome,
                            })
                        })
                        .map_err(closed);
                    if let Ok(submitted) = submitted {
                        let _ = submitted.await;
                    }
                    let _ = Self::refresh(&this, cx).await;
                })
                .detach();
            })
            .map_err(closed)?;
        }
        Ok(())
    }
}
pub(super) fn interrupted(store: &mut Store) -> kg_store::Result<()> {
    let at = now();
    let resume = store.resume_threads()?;
    for mut record in store.all_threads()? {
        close_left_turn(store, &mut record, at, &resume)?;
        clear_pending(store, &record.id)?;
    }
    close_left_environments(store)
}

/// Starts again every thread the last process left running, in the order they were created.
pub(super) async fn resume_programs(this: &WeakEntity<Workspace>, cx: &mut AsyncApp) -> Result<()> {
    let database = crate::workspace::database(this, cx)?;
    let remembered = database.call(|store| store.resume_threads()).await?;
    let chosen = this
        .update(cx, |workspace, cx| {
            threads_to_resume(workspace, &remembered, cx)
        })
        .map_err(closed)?;
    if chosen.is_empty() {
        if !remembered.is_empty() {
            database
                .call(|store| store.set_resume_threads(&BTreeSet::new()))
                .await?;
        }
        return Ok(());
    }
    let mut keep = remembered.clone();
    keep.extend(chosen.iter().map(|(id, _)| id.clone()));
    if keep != remembered {
        database
            .call(move |store| store.set_resume_threads(&keep))
            .await?;
    }
    for (id, starting) in &chosen {
        if crate::thread::start_again(this, id.clone(), cx)
            .await
            .is_err()
        {
            // Idle already has the stop, and maybe the screen, from the quit. Leave it.
            let failed = this
                .update(cx, |workspace, cx| {
                    let status = workspace
                        .threads
                        .get(id)
                        .map(|thread| thread.read(cx).record.status);
                    let live =
                        matches!(workspace.shown(id), Some(crate::thread::Shown::Running(_)));
                    !live
                        && matches!(
                            status,
                            Some(
                                ThreadStatus::Active
                                    | ThreadStatus::Starting
                                    | ThreadStatus::Stopping
                            )
                        )
                })
                .map_err(closed)?;
            if failed {
                note_unresumed(this, id, *starting, cx).await?;
            }
        }
    }
    database
        .call(|store| store.set_resume_threads(&BTreeSet::new()))
        .await
}

fn threads_to_resume(
    workspace: &Workspace,
    remembered: &BTreeSet<ThreadId>,
    cx: &gpui_kit::App,
) -> Vec<(ThreadId, bool)> {
    let mut chosen: Vec<_> = workspace
        .threads
        .iter()
        .filter_map(|(id, thread)| {
            let record = &thread.read(cx).record;
            if record.archived_at.is_some() {
                return None;
            }
            let starting = record.status == ThreadStatus::Starting;
            let resume = match record.status {
                ThreadStatus::Active | ThreadStatus::Starting => true,
                ThreadStatus::Idle | ThreadStatus::Stopping => remembered.contains(id),
                ThreadStatus::Pending | ThreadStatus::Error => false,
            };
            resume.then(|| (record.created_at, id.clone(), starting))
        })
        .collect();
    chosen.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    chosen
        .into_iter()
        .map(|(_, id, starting)| (id, starting))
        .collect()
}

async fn note_unresumed(
    this: &WeakEntity<Workspace>,
    id: &ThreadId,
    starting: bool,
    cx: &mut AsyncApp,
) -> Result<()> {
    let thread = crate::thread::thread(this, id, cx)?;
    let mut record = thread.read_with(cx, |thread, _| thread.record.clone());
    let at = crate::workspace::time(this, cx)?;
    let (error, before_turn) = if starting {
        (crate::end::STARTING, true)
    } else {
        (crate::end::QUIT, false)
    };
    record.status = ThreadStatus::Error;
    record.latest_attention_at = at;
    record.updated_at = at;
    let database = crate::workspace::database(this, cx)?;
    let saved = record.clone();
    let text = error.to_owned();
    database
        .call(move |store| {
            let notice = parent_notice(store, &saved, &text, before_turn)?;
            store.recover_thread(
                &saved,
                at,
                [EventKind::Failed { error: Some(text) }],
                notice,
            )
        })
        .await?;
    thread.update(cx, |thread, cx| {
        thread.record = record;
        cx.notify();
    });
    this.update(cx, |workspace, cx| {
        if let Some(end) = crate::end::End::from_newest(&[EventKind::Failed {
            error: Some(error.to_owned()),
        }]) {
            workspace.run_mut(id).end = Some(end);
        }
        cx.emit(crate::WorkspaceEvent::Changed);
        cx.notify();
    })
    .map_err(closed)
}

fn close_left_turn(
    store: &mut Store,
    record: &mut kg_domain::Thread,
    at: kg_domain::Timestamp,
    resume: &BTreeSet<ThreadId>,
) -> kg_store::Result<()> {
    let left_running = matches!(record.status, ThreadStatus::Starting | ThreadStatus::Active)
        || (record.status == ThreadStatus::Stopping && resume.contains(&record.id));
    if record.archived_at.is_none() && left_running {
        return Ok(());
    }
    match record.status {
        ThreadStatus::Starting | ThreadStatus::Active | ThreadStatus::Stopping => {
            let (error, before_turn) = if record.status == ThreadStatus::Starting {
                (crate::end::STARTING, true)
            } else {
                (crate::end::QUIT, false)
            };
            let own = vec![EventKind::Failed {
                error: Some(error.into()),
            }];
            record.status = ThreadStatus::Error;
            record.latest_attention_at = at;
            record.updated_at = at;
            let notice = parent_notice(store, record, error, before_turn)?;
            store.recover_thread(record, at, own, notice)
        }
        ThreadStatus::Pending | ThreadStatus::Idle | ThreadStatus::Error => Ok(()),
    }
}

fn close_left_environments(store: &mut Store) -> kg_store::Result<()> {
    let threads = store.all_threads()?;
    for project in store.projects()? {
        for mut environment in store.environments(&project.id)? {
            if matches!(
                environment.status,
                kg_domain::EnvironmentStatus::Creating | kg_domain::EnvironmentStatus::Provisioning
            ) {
                let used = threads
                    .iter()
                    .any(|thread| thread.environment_id.as_ref() == Some(&environment.id));
                environment.status = if environment.provider.managed() && !used {
                    kg_domain::EnvironmentStatus::Destroyed
                } else {
                    kg_domain::EnvironmentStatus::Error
                };
                store.put_environment(&environment)?;
            }
        }
    }
    Ok(())
}

/// The notice a parent gets when a child fails at startup, if it has a live parent.
fn parent_notice(
    store: &Store,
    child: &kg_domain::Thread,
    error: &str,
    before_turn: bool,
) -> kg_store::Result<Option<(ThreadId, kg_domain::EventKind)>> {
    let Some(parent_id) = &child.parent_thread_id else {
        return Ok(None);
    };
    let Some(parent) = store.thread(parent_id)? else {
        return Ok(None);
    };
    if parent.archived_at.is_some() {
        return Ok(None);
    }
    let text = crate::thread::child_notice(&child.title, Some(error), before_turn);
    Ok(Some((
        parent_id.clone(),
        kg_domain::EventKind::Notice {
            severity: kg_domain::NoticeSeverity::Info,
            text,
        },
    )))
}
