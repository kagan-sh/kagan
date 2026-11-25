use gpui_kit::{AsyncApp, Entity, WeakEntity};
use kg_domain::{EventKind, ThreadStatus, TurnOutcome};

use crate::host::database::Result;
use crate::workspace::{closed, time};
use crate::{Thread, Workspace};

use super::interaction::{clear_interactions, take_thread_replies};
use super::link;

pub(crate) async fn status(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    status: ThreadStatus,
    cx: &mut AsyncApp,
) -> Result<()> {
    let at = time(this, cx)?;
    let record = thread.update(cx, |thread, cx| {
        if status == ThreadStatus::Error && thread.record.status.live() {
            thread.record.latest_attention_at = at;
        }
        thread.record.status = status;
        thread.record.updated_at = at;
        cx.notify();
        thread.record.clone()
    });
    let database = crate::workspace::database(this, cx)?;
    database
        .call(move |store| store.set_thread_status(&record.id, record.status, record.updated_at))
        .await?;
    crate::workspace::changed(this, cx)
}

/// Stop and a natural exit both run it, and each then records its own event.
pub(super) async fn settle(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    cx: &mut AsyncApp,
) -> Result<()> {
    thread.update(cx, |thread, _| thread.busy = false);
    drop_questions(this, thread, cx).await?;
    crate::workspace::changed(this, cx)
}

async fn drop_questions(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let thread_id = thread.read_with(cx, |thread, _| thread.record.id.clone());
    this.update(cx, |workspace, _| {
        take_thread_replies(workspace, &thread_id)
    })
    .map_err(closed)?;
    clear_interactions(this, thread, cx).await
}

pub(crate) async fn stop(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let (thread_id, running, starting, turn_open) = thread.read_with(cx, |thread, _| {
        (
            thread.record.id.clone(),
            thread.running(),
            thread.record.status == ThreadStatus::Starting,
            thread.turn.open(),
        )
    });
    settle(this, thread, cx).await?;
    if !running {
        return Ok(());
    }
    if starting {
        this.update(cx, |workspace, _| {
            workspace.tasks.remove(&thread_id);
            // The program is running even though the thread is not: Stop ends it here too.
            super::terminal::request_stop(workspace, &thread_id);
        })
        .map_err(closed)?;
        close_open_turn(this, thread, &thread_id, turn_open, cx).await?;
        return status(this, thread, ThreadStatus::Idle, cx).await;
    }
    let ending = this
        .update(cx, |workspace, _| {
            super::terminal::request_stop(workspace, &thread_id)
        })
        .map_err(closed)?;
    if ending {
        let acp = link(this, &thread_id, cx)? == kg_domain::Link::Acp;
        if acp {
            let reason = this
                .update(cx, |workspace, _| {
                    workspace
                        .runs
                        .get_mut(&thread_id)
                        .and_then(|run| run.stopping.take())
                })
                .map_err(closed)?
                .unwrap_or(super::StopReason::Stop);
            // An ACP thread has no watcher to report the end, so the stop records it here.
            crate::thread::finish::ended(this, thread_id, super::finish::stopped(reason), cx)
                .await?;
            return Ok(());
        }
        // The program is ending now. Its exit records the stop and sets the thread idle.
        return status(this, thread, ThreadStatus::Stopping, cx).await;
    }
    close_open_turn(this, thread, &thread_id, turn_open, cx).await?;
    Workspace::events(
        this,
        thread_id,
        vec![EventKind::Stopped, EventKind::Idle],
        cx,
    )
    .await?;
    status(this, thread, ThreadStatus::Idle, cx).await
}

async fn close_open_turn(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    id: &kg_domain::ThreadId,
    open: bool,
    cx: &mut AsyncApp,
) -> Result<()> {
    if !open {
        return Ok(());
    }
    let quit = this
        .update(cx, |workspace, _| {
            workspace.runs.get(id).and_then(|run| run.stopping) == Some(super::StopReason::Quit)
        })
        .map_err(closed)?;
    let now = cx.background_executor().now();
    thread.update(cx, |thread, _| {
        thread
            .turn
            .apply(crate::thread::turn_state::Signal::Ended, now)
    });
    if quit {
        return Ok(());
    }
    Workspace::events(
        this,
        id.clone(),
        vec![EventKind::TurnEnded {
            outcome: TurnOutcome::Interrupted,
            error: None,
        }],
        cx,
    )
    .await
}

pub(crate) async fn retire(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    cx: &mut AsyncApp,
) -> Result<()> {
    stop(this, thread, cx).await?;
    let id = thread.read_with(cx, |thread, _| thread.record.id.clone());
    this.update(cx, |workspace, _| {
        workspace.tasks.remove(&id);
    })
    .map_err(closed)
}
