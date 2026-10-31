use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{InteractionOption, OpError, OpResult, ThreadId};

use crate::database::{Database, Result};
use crate::environment;
use crate::workspace::{closed, time};
use crate::{Requester, Workspace};

use super::interaction::{remove_interactions, take_thread_replies};
use super::thread;

pub(super) async fn archive(
    this: &WeakEntity<Workspace>,
    database: Database,
    id: ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    thread(this, &id, cx)?;
    let at = time(this, cx)?;
    let archived = database
        .call(move |store| store.archive_thread(&id, at))
        .await?;
    for event in &archived {
        let approvals = this
            .update(cx, |workspace, _| {
                take_thread_replies(workspace, &event.thread_id)
            })
            .map_err(closed)?;
        remove_interactions(this, approvals, cx).await?;
    }
    let ids = archived
        .iter()
        .map(|event| event.thread_id.clone())
        .collect();
    emit(this, archived, cx)?;
    Workspace::refresh(this, cx).await?;
    super::agent::forget(this, ids, cx).await;
    Ok(OpResult::Done)
}

fn emit(
    this: &WeakEntity<Workspace>,
    events: Vec<kg_domain::Event>,
    cx: &mut AsyncApp,
) -> Result<()> {
    this.update(cx, |workspace, cx| {
        for event in events {
            if let Some(thread) = workspace.threads.get(&event.thread_id) {
                let project_id = thread.read(cx).record.project_id.clone();
                cx.emit(crate::WorkspaceEvent::Event { event, project_id });
            }
        }
        cx.notify();
    })
    .map_err(closed)
}

pub(super) async fn unarchive(
    this: &WeakEntity<Workspace>,
    database: Database,
    id: ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let record = thread(this, &id, cx)?.read_with(cx, |thread, _| thread.record.clone());
    if let Some(owner) = &record.lifecycle_owner_thread_id {
        let owner = thread(this, owner, cx)?;
        if owner.read_with(cx, |thread, _| thread.record.archived_at.is_some()) {
            return Err(OpError::Invalid(
                "unarchive the lifecycle owner first".into(),
            ));
        }
    }
    if record.environment_id.is_some() {
        crate::environment::restore(this, id.clone(), cx).await?;
    }
    let at = time(this, cx)?;
    let events = database
        .call(move |store| store.unarchive_thread(&id, at))
        .await?;
    emit(this, events, cx)?;
    Workspace::refresh(this, cx).await?;
    Ok(OpResult::Done)
}

pub(super) async fn delete(
    this: &WeakEntity<Workspace>,
    database: Database,
    id: ThreadId,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let title = thread(this, &id, cx)?.read_with(cx, |thread, _| thread.record.title.clone());
    let lookup = id.clone();
    let blocked = database
        .call(move |store| {
            for thread_id in store.owned_threads(&lookup)? {
                let Some(thread) = store.thread(&thread_id)? else {
                    continue;
                };
                let Some(environment_id) = thread.environment_id else {
                    continue;
                };
                if store
                    .environment(&environment_id)?
                    .is_some_and(|environment| environment.error.is_some())
                {
                    return Ok(true);
                }
            }
            Ok(false)
        })
        .await?;
    if blocked {
        return Err(OpError::Invalid(
            "remove the worktree again before deleting this thread".into(),
        ));
    }
    let folders = environment::personal_folders(&database, id.clone()).await?;
    let question = if folders.is_empty() {
        format!("Delete “{title}”?")
    } else {
        let described = environment::describe(&folders, cx).await;
        format!(
            "Delete “{title}” and permanently delete {described}? Its files cannot be recovered."
        )
    };
    approve(thread_id, requester, question, "Delete", cx).await?;
    #[cfg(test)]
    {
        let hold = this
            .update(cx, |workspace, _| workspace.delete_hold.clone())
            .map_err(closed)?;
        if let Some(hold) = hold {
            let _ = hold.recv().await;
        }
    }
    thread(this, &id, cx)?;
    let expected = folders
        .iter()
        .map(|folder| folder.environment.id.clone())
        .collect::<Vec<_>>();
    let lookup = id.clone();
    let at = time(this, cx)?;
    let deleted = database
        .call(move |store| store.delete_thread(&lookup, at, &expected))
        .await?;
    let ids = deleted
        .iter()
        .map(|(event, _)| event.thread_id.clone())
        .collect();
    this.update(cx, |workspace, cx| forget_deleted(workspace, deleted, cx))
        .map_err(closed)?;
    Workspace::remove(this, folders, cx)?;
    Workspace::refresh(this, cx).await?;
    super::agent::forget(this, ids, cx).await;
    Ok(OpResult::Done)
}

pub(super) fn forget_deleted(
    workspace: &mut Workspace,
    deleted: Vec<(kg_domain::Event, kg_domain::ProjectId)>,
    cx: &mut gpui_kit::Context<Workspace>,
) {
    for (event, project_id) in deleted {
        let _ = take_thread_replies(workspace, &event.thread_id);
        super::terminal::kill(workspace, &event.thread_id);
        workspace.threads.remove(&event.thread_id);
        workspace.tasks.remove(&event.thread_id);
        cx.emit(crate::WorkspaceEvent::Event { event, project_id });
    }
    cx.notify();
}

pub(super) async fn approve(
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    question: String,
    verb: &str,
    cx: &mut AsyncApp,
) -> Result<()> {
    let options = vec![
        InteractionOption {
            id: "decline".into(),
            label: "Cancel".into(),
        },
        InteractionOption {
            id: "approve".into(),
            label: verb.into(),
        },
    ];
    let answer = cx
        .update(|cx| {
            Workspace::ask_for_person(thread_id.as_ref(), requester, question.clone(), options, cx)
        })
        .await?;
    if answer == "approve" {
        Ok(())
    } else {
        Err(OpError::Invalid(format!("declined: {question}")))
    }
}
