use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{OpResult, ThreadId};

use crate::host::database::Result;
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
                #[cfg(test)]
                let hold = workspace.removal_hold.clone();
                cx.spawn(async move |this, cx| {
                    #[cfg(test)]
                    if let Some(hold) = hold {
                        let _ = hold.recv().await;
                    }
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
    for mut record in store.all_threads()? {
        close_left_turn(store, &mut record, at)?;
        clear_pending(store, &record.id)?;
    }
    close_left_environments(store)
}

fn close_left_turn(
    store: &mut Store,
    record: &mut kg_domain::Thread,
    at: kg_domain::Timestamp,
) -> kg_store::Result<()> {
    match record.status {
        kg_domain::ThreadStatus::Starting
        | kg_domain::ThreadStatus::Active
        | kg_domain::ThreadStatus::Stopping => {
            let (error, before_turn) = if record.status == kg_domain::ThreadStatus::Starting {
                (crate::end::STARTING, true)
            } else {
                (crate::end::QUIT, false)
            };
            let own = vec![kg_domain::EventKind::Failed {
                error: Some(error.into()),
            }];
            record.status = kg_domain::ThreadStatus::Error;
            record.latest_attention_at = at;
            record.updated_at = at;
            let notice = parent_notice(store, record, error, before_turn)?;
            store.recover_thread(record, at, own, notice)
        }
        kg_domain::ThreadStatus::Pending
        | kg_domain::ThreadStatus::Idle
        | kg_domain::ThreadStatus::Error => Ok(()),
    }
}

fn clear_pending(store: &Store, id: &ThreadId) -> kg_store::Result<()> {
    for interaction in store.pending_interactions(id)? {
        store.delete_pending_interaction(&interaction.id)?;
    }
    Ok(())
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
