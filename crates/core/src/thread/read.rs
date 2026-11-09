use std::future::Future;
use std::time::Duration;

use gpui_kit::{App, AsyncApp, Entity, Task, WeakEntity};
use kg_domain::{
    EventKind, OpError, OpResult, Order, ThreadId, ThreadStatus, ToolOutcome, WaitOutcome,
};

use crate::host::database::{Database, Result};
use crate::workspace::{closed, missing};
use crate::{PluginRequest, Workspace};

use crate::dispatch::forward;

use super::{notify, thread};

const ARM: Duration = Duration::from_secs(2);

pub(crate) fn wait_event(
    workspace: &Entity<Workspace>,
    id: ThreadId,
    kind: &str,
    after_seq: Option<u64>,
    timeout_ms: u64,
    cx: &mut App,
) -> Task<Result<OpResult>> {
    let child = id.clone();
    let task = wait_event_in(workspace, id, kind, after_seq, timeout_ms, cx);
    told_by_wait(workspace, child, task, cx)
}

pub(crate) fn wait(
    workspace: &Entity<Workspace>,
    id: ThreadId,
    timeout_ms: u64,
    cx: &mut App,
) -> Task<Result<OpResult>> {
    let child = id.clone();
    let task = wait_in(workspace, id, timeout_ms, cx);
    told_by_wait(workspace, child, task, cx)
}

// A wait that returns the child's end already told the parent, so its queued line goes.
fn told_by_wait(
    workspace: &Entity<Workspace>,
    child: ThreadId,
    task: Task<Result<OpResult>>,
    cx: &mut App,
) -> Task<Result<OpResult>> {
    let parent = workspace
        .read(cx)
        .threads
        .get(&child)
        .and_then(|thread| thread.read(cx).record.parent_thread_id.clone());
    let this = workspace.downgrade();
    cx.spawn(async move |cx| {
        let result = task.await;
        if let Some(parent) = parent
            && let Ok(outcome) = &result
            && notify::reports_end(outcome)
        {
            _ = this.update(cx, |workspace, _| {
                notify::heard(workspace, &parent, &child);
            });
        }
        result
    })
}

fn wait_event_in(
    workspace: &Entity<Workspace>,
    id: ThreadId,
    kind: &str,
    after_seq: Option<u64>,
    timeout_ms: u64,
    cx: &mut App,
) -> Task<Result<OpResult>> {
    let kind = kind.strip_prefix("thread.").unwrap_or(kind);
    let Some(kind) = EventKind::LIVE_TYPES.into_iter().find(|name| *name == kind) else {
        return Task::ready(Err(OpError::Invalid(format!(
            "unknown event type {kind}; expected one of {}",
            EventKind::LIVE_TYPES.join(", ")
        ))));
    };
    let database = workspace.read(cx).database.clone();
    let (sent, received) = async_channel::unbounded();
    let watched = id.clone();
    // Subscribe before reading the log so no event falls between the two.
    let subscription = cx.subscribe(workspace, move |_, event, _| {
        if let crate::WorkspaceEvent::Event { event, .. } = event
            && event.thread_id == watched
        {
            // The waiter is gone or stopped listening.
            let _ = sent.try_send(event.clone());
        }
    });
    cx.spawn(async move |cx| {
        let _subscription = subscription;
        let lookup = id.clone();
        let logged = database
            .call(move |store| {
                if store.thread(&lookup)?.is_none() {
                    return Err(kg_store::Error::NotFound {
                        what: "thread",
                        id: lookup.to_string(),
                    });
                }
                store.events_after(&lookup, after_seq, None, Order::Asc)
            })
            .await?;
        let mut after = after_seq;
        for event in logged {
            after = Some(event.seq);
            if event.kind.type_name() == kind {
                return Ok(OpResult::Event(Some(event)));
            }
        }
        let deadline = cx
            .background_executor()
            .timer(Duration::from_millis(timeout_ms));
        futures::pin_mut!(deadline);
        loop {
            let next = received.recv();
            futures::pin_mut!(next);
            let event = match futures::future::select(deadline.as_mut(), next).await {
                futures::future::Either::Left(_) => return Ok(OpResult::Event(None)),
                futures::future::Either::Right((event, _)) => event.map_err(closed)?,
            };
            if after.is_some_and(|after| event.seq <= after) {
                continue;
            }
            if event.kind.type_name() == kind {
                return Ok(OpResult::Event(Some(event)));
            }
            if event.kind == EventKind::Deleted {
                return Err(missing("thread", &id));
            }
        }
    })
}

fn wait_in(
    workspace: &Entity<Workspace>,
    id: ThreadId,
    timeout_ms: u64,
    cx: &mut App,
) -> Task<Result<OpResult>> {
    let Some(thread) = workspace.read(cx).threads.get(&id).cloned() else {
        return Task::ready(Err(missing("thread", &id)));
    };
    let (events_tx, events_rx) = async_channel::unbounded();
    let watched = id.clone();
    let events = cx.subscribe(workspace, move |_, event, _| {
        if let crate::WorkspaceEvent::Event { event, .. } = event
            && event.thread_id == watched
        {
            // The waiter is gone or stopped listening.
            let _ = events_tx.try_send(event.kind.clone());
        }
    });
    let (wake_tx, wake_rx) = async_channel::unbounded();
    let mut saw_busy = false;
    let mut saw_end = false;
    let busy_watch = cx.observe(&thread, move |thread, cx| {
        let thread = thread.read(cx);
        // A stop while starting writes no event. An end logged before subscribe still changes the row.
        if let Some(reason) = ended(thread.record.status) {
            if !saw_end {
                saw_end = true;
                // The waiter is gone or stopped listening.
                let _ = wake_tx.try_send(Wake::Ended(reason));
            }
        } else if thread.busy && !saw_busy {
            saw_busy = true;
            // The waiter is gone or stopped listening.
            let _ = wake_tx.try_send(Wake::Busy);
        }
    });
    let thread = thread.downgrade();
    cx.spawn(async move |cx| {
        let _events = events;
        let _busy_watch = busy_watch;
        let (status, busy) = thread
            .read_with(cx, |thread, _| (thread.record.status, thread.busy))
            .map_err(closed)?;
        if let Some(reason) = ended(status) {
            return Ok(OpResult::Wait(reason));
        }
        let mut arming = !busy;
        let deadline = cx
            .background_executor()
            .timer(Duration::from_millis(timeout_ms));
        let arm = cx.background_executor().timer(ARM);
        futures::pin_mut!(deadline, arm);
        loop {
            let hit = if arming {
                armed(&mut deadline, &mut arm, &events_rx, &wake_rx).await
            } else {
                quiet(&mut deadline, &events_rx, &wake_rx).await
            };
            match hit {
                Hit::Timeout => {
                    let status = thread
                        .read_with(cx, |thread, _| thread.record.status)
                        .map_err(closed)?;
                    let reason = ended(status).unwrap_or(WaitOutcome::Timeout);
                    return Ok(OpResult::Wait(reason));
                }
                Hit::Armed => {
                    let (status, still) = thread
                        .read_with(cx, |thread, _| (thread.record.status, thread.busy))
                        .map_err(closed)?;
                    if let Some(reason) = ended(status) {
                        return Ok(OpResult::Wait(reason));
                    }
                    if still {
                        arming = false;
                        continue;
                    }
                    return Ok(OpResult::Wait(WaitOutcome::NotBusy));
                }
                Hit::Ended(reason) => return Ok(OpResult::Wait(reason)),
                Hit::Busy => arming = false,
                Hit::Event(kind) => {
                    if let Some(reason) = settle(&kind) {
                        return Ok(OpResult::Wait(reason));
                    }
                    if kind == EventKind::Deleted {
                        return Err(missing("thread", &id));
                    }
                }
                Hit::Closed => return Err(closed(async_channel::RecvError)),
            }
        }
    })
}

enum Wake {
    Busy,
    Ended(WaitOutcome),
}

enum Hit {
    Event(EventKind),
    Busy,
    Ended(WaitOutcome),
    Timeout,
    Armed,
    Closed,
}

async fn armed(
    deadline: &mut (impl Future<Output = ()> + Unpin),
    arm: &mut (impl Future<Output = ()> + Unpin),
    events: &async_channel::Receiver<EventKind>,
    wake: &async_channel::Receiver<Wake>,
) -> Hit {
    let event = events.recv();
    let pulse = wake.recv();
    futures::pin_mut!(event, pulse);
    let incoming = futures::future::select(event, pulse);
    let timers = futures::future::select(deadline, arm);
    match futures::future::select(incoming, timers).await {
        futures::future::Either::Left((incoming, _)) => match incoming {
            futures::future::Either::Left((result, _)) => from_event(result),
            futures::future::Either::Right((result, _)) => from_wake(&result),
        },
        futures::future::Either::Right((timers, _)) => match timers {
            futures::future::Either::Left(_) => Hit::Timeout,
            futures::future::Either::Right(_) => Hit::Armed,
        },
    }
}

async fn quiet(
    deadline: &mut (impl Future<Output = ()> + Unpin),
    events: &async_channel::Receiver<EventKind>,
    wake: &async_channel::Receiver<Wake>,
) -> Hit {
    let event = events.recv();
    let pulse = wake.recv();
    futures::pin_mut!(event, pulse);
    let incoming = futures::future::select(event, pulse);
    match futures::future::select(incoming, deadline).await {
        futures::future::Either::Left((incoming, _)) => match incoming {
            futures::future::Either::Left((result, _)) => from_event(result),
            futures::future::Either::Right((result, _)) => from_wake(&result),
        },
        futures::future::Either::Right(_) => Hit::Timeout,
    }
}

fn from_event(result: std::result::Result<EventKind, async_channel::RecvError>) -> Hit {
    match result {
        Ok(kind) => Hit::Event(kind),
        Err(_) => Hit::Closed,
    }
}

fn from_wake(result: &std::result::Result<Wake, async_channel::RecvError>) -> Hit {
    match result {
        Ok(Wake::Busy) => Hit::Busy,
        Ok(Wake::Ended(reason)) => Hit::Ended(*reason),
        Err(_) => Hit::Closed,
    }
}

fn ended(status: ThreadStatus) -> Option<WaitOutcome> {
    match status {
        ThreadStatus::Idle => Some(WaitOutcome::Exited),
        ThreadStatus::Error => Some(WaitOutcome::Failed),
        ThreadStatus::Pending
        | ThreadStatus::Starting
        | ThreadStatus::Active
        | ThreadStatus::Stopping => None,
    }
}

fn settle(kind: &EventKind) -> Option<WaitOutcome> {
    match kind {
        EventKind::Waiting => Some(WaitOutcome::Waiting),
        EventKind::Stopped | EventKind::Idle => Some(WaitOutcome::Exited),
        EventKind::Exited { code, signal } if signal.is_none() && code.unwrap_or(0) == 0 => {
            Some(WaitOutcome::Exited)
        }
        EventKind::Failed { .. } | EventKind::Exited { .. } => Some(WaitOutcome::Failed),
        _ => None,
    }
}

pub(super) async fn call_tool(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    name: String,
    input: serde_json::Value,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let entity = thread(this, &id, cx)?;
    let tool = crate::workspace::id("tool");
    record(
        this,
        &id,
        EventKind::ToolStarted {
            id: tool.clone(),
            name: name.clone(),
        },
        cx,
    )
    .await;
    let result = if name == crate::skill::SKILL_LOAD {
        crate::skill::load(this, &entity, &input, cx).await
    } else {
        forward(
            this,
            PluginRequest::CallTool {
                thread_id: id.clone(),
                name,
                input,
            },
            cx,
        )
        .await
    };
    let status = match &result {
        Ok(OpResult::ToolReply {
            is_error: false, ..
        }) => ToolOutcome::Ok,
        _ => ToolOutcome::Error,
    };
    record(this, &id, EventKind::ToolEnded { id: tool, status }, cx).await;
    result
}

async fn record(this: &WeakEntity<Workspace>, id: &ThreadId, kind: EventKind, cx: &mut AsyncApp) {
    if let Err(error) = Workspace::events(this, id.clone(), vec![kind], cx).await {
        eprintln!("kg: could not record a tool event: {error}");
    }
}

pub(super) fn get(
    this: &WeakEntity<Workspace>,
    id: &ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    Ok(OpResult::Thread(
        thread(this, id, cx)?.read_with(cx, |thread, _| thread.record.clone()),
    ))
}

pub(super) async fn list(
    database: Database,
    project_id: Option<kg_domain::ProjectId>,
    archived: Option<bool>,
    limit: Option<u32>,
    before: Option<kg_domain::Timestamp>,
) -> Result<OpResult> {
    database
        .call(move |store| {
            Ok(OpResult::Threads(store.threads(
                project_id.as_ref(),
                archived,
                limit,
                before,
            )?))
        })
        .await
}

pub(super) async fn metadata(
    this: &WeakEntity<Workspace>,
    database: Database,
    id: ThreadId,
    plugin_id: kg_domain::PluginId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    thread(this, &id, cx)?;
    database
        .call(move |store| {
            Ok(OpResult::Metadata(
                store.thread_metadata(&id, &plugin_id)?.unwrap_or_default(),
            ))
        })
        .await
}

pub(super) async fn events(
    this: &WeakEntity<Workspace>,
    database: Database,
    id: ThreadId,
    after_seq: Option<u64>,
    limit: Option<u32>,
    order: Option<Order>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    thread(this, &id, cx)?;
    let order = order.unwrap_or(Order::Asc);
    let events = database
        .call(move |store| store.events_after(&id, after_seq, limit, order))
        .await?;
    Ok(OpResult::Events(events))
}

pub(super) async fn interactions(
    this: &WeakEntity<Workspace>,
    database: Database,
    id: ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    thread(this, &id, cx)?;
    database
        .call(move |store| Ok(OpResult::Interactions(store.pending_interactions(&id)?)))
        .await
}
