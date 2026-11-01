use gpui_kit::{AsyncApp, Entity, WeakEntity};
use kg_domain::{EventKind, ThreadStatus};

use crate::database::Result;
use crate::workspace::{closed, time};
use crate::{Thread, Workspace};

use super::interaction::{clear_interactions, remove_interactions, take_thread_replies};

pub(crate) async fn status(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    status: ThreadStatus,
    cx: &mut AsyncApp,
) -> Result<()> {
    let at = time(this, cx)?;
    let record = thread.update(cx, |thread, cx| {
        if status == ThreadStatus::Error
            && matches!(
                thread.record.status,
                ThreadStatus::Active | ThreadStatus::Starting
            )
        {
            thread.record.latest_attention_at = at;
        }
        thread.record.status = status;
        thread.record.updated_at = at;
        cx.notify();
        thread.record.clone()
    });
    let database = this
        .update(cx, |workspace, _| workspace.database.clone())
        .map_err(closed)?;
    database
        .call(move |store| store.set_thread_status(&record.id, record.status, record.updated_at))
        .await?;
    this.update(cx, |_, cx| {
        cx.emit(crate::WorkspaceEvent::Changed);
        cx.notify();
    })
    .map_err(closed)
}

pub(crate) async fn stop(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    cx: &mut AsyncApp,
) -> Result<()> {
    halt(this, thread, cx).await
}

/// Stop and a natural exit both run it, and each then records its own event.
pub(super) async fn settle(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let thread_id = thread.read_with(cx, |thread, _| thread.record.id.clone());
    thread.update(cx, |thread, _| thread.busy = false);
    let approvals = this
        .update(cx, |workspace, _| {
            take_thread_replies(workspace, &thread_id)
        })
        .map_err(closed)?;
    if !approvals.is_empty() {
        remove_interactions(this, approvals, cx).await?;
    }
    clear_interactions(this, thread, cx).await?;
    this.update(cx, |_, cx| {
        cx.emit(crate::WorkspaceEvent::Changed);
        cx.notify();
    })
    .map_err(closed)
}

pub(super) async fn halt(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let (thread_id, running, starting) = thread.read_with(cx, |thread, _| {
        (
            thread.record.id.clone(),
            thread.running(),
            thread.record.status == ThreadStatus::Starting,
        )
    });
    settle(this, thread, cx).await?;
    if !running {
        return Ok(());
    }
    if starting {
        this.update(cx, |workspace, _| {
            workspace.tasks.remove(&thread_id);
        })
        .map_err(closed)?;
        return status(this, thread, ThreadStatus::Idle, cx).await;
    }
    let ending = this
        .update(cx, |workspace, _| {
            super::terminal::request_stop(workspace, &thread_id)
        })
        .map_err(closed)?;
    if ending {
        // The program is ending now. Its exit records the stop and sets the thread idle.
        return status(this, thread, ThreadStatus::Stopping, cx).await;
    }
    Workspace::events(
        this,
        thread_id,
        vec![EventKind::Stopped, EventKind::Idle],
        cx,
    )
    .await?;
    status(this, thread, ThreadStatus::Idle, cx).await
}

pub(crate) async fn retire(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    cx: &mut AsyncApp,
) -> Result<()> {
    stop(this, thread, cx).await?;
    let id = thread.read_with(cx, |thread, _| thread.record.id.clone());
    this.update(cx, |workspace, cx| {
        super::extra::end(workspace, &id);
        cx.emit(crate::WorkspaceEvent::Changed);
        workspace.tasks.remove(&id);
    })
    .map_err(closed)
}
