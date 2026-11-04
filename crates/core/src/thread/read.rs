use std::future::Future;
use std::time::Duration;

use gpui_kit::{App, AsyncApp, Entity, Task, WeakEntity};
use kg_domain::{EventKind, OpError, OpResult, Order, ThreadId, ThreadOp, ThreadStatus};

use crate::database::{Database, Result};
use crate::workspace::{closed, missing};
use crate::{PluginRequest, Workspace};

use super::forward;
use super::thread;

pub(crate) fn wait_event(
    workspace: &Entity<Workspace>,
    id: ThreadId,
    kind: &str,
    after_seq: Option<u64>,
    timeout_ms: u64,
    cx: &mut App,
) -> Task<Result<OpResult>> {
    let kind = kind.strip_prefix("thread.").unwrap_or(kind);
    let Some(kind) = EventKind::TYPES.into_iter().find(|name| *name == kind) else {
        return Task::ready(Err(OpError::Invalid(format!(
            "unknown event type {kind}; expected one of {}",
            EventKind::TYPES.join(", ")
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

pub(crate) fn wait(
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
                let _ = wake_tx.try_send(Wake::Ended(reason));
            }
        } else if thread.busy && !saw_busy {
            saw_busy = true;
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
            return Ok(OpResult::Text(reason.to_owned()));
        }
        let mut arming = !busy;
        let deadline = cx
            .background_executor()
            .timer(Duration::from_millis(timeout_ms));
        let arm = cx.background_executor().timer(super::terminal::ARM);
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
                    let reason = ended(status).unwrap_or("timeout");
                    return Ok(OpResult::Text(reason.to_owned()));
                }
                Hit::Armed => {
                    let (status, still) = thread
                        .read_with(cx, |thread, _| (thread.record.status, thread.busy))
                        .map_err(closed)?;
                    if let Some(reason) = ended(status) {
                        return Ok(OpResult::Text(reason.to_owned()));
                    }
                    if still {
                        arming = false;
                        continue;
                    }
                    return Ok(OpResult::Text("not-busy".to_owned()));
                }
                Hit::Ended(reason) => return Ok(OpResult::Text(reason.to_owned())),
                Hit::Busy => arming = false,
                Hit::Event(kind) => {
                    if let Some(reason) = settle(&kind) {
                        return Ok(OpResult::Text(reason.to_owned()));
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
    Ended(&'static str),
}

enum Hit {
    Event(EventKind),
    Busy,
    Ended(&'static str),
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
        Ok(Wake::Ended(reason)) => Hit::Ended(reason),
        Err(_) => Hit::Closed,
    }
}

fn ended(status: ThreadStatus) -> Option<&'static str> {
    match status {
        ThreadStatus::Idle => Some("exited"),
        ThreadStatus::Error => Some("failed"),
        ThreadStatus::Pending
        | ThreadStatus::Starting
        | ThreadStatus::Active
        | ThreadStatus::Stopping => None,
    }
}

fn settle(kind: &EventKind) -> Option<&'static str> {
    match kind {
        EventKind::Waiting => Some("waiting"),
        EventKind::Stopped | EventKind::Idle => Some("exited"),
        EventKind::Exited { code, signal } if signal.is_none() && code.unwrap_or(0) == 0 => {
            Some("exited")
        }
        EventKind::Failed { .. } | EventKind::Exited { .. } => Some("failed"),
        _ => None,
    }
}

pub(super) async fn read_thread(
    this: &WeakEntity<Workspace>,
    database: Database,
    op: ThreadOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        ThreadOp::Get { id } => Ok(OpResult::Thread(
            thread(this, &id, cx)?.read_with(cx, |thread, _| thread.record.clone()),
        )),
        ThreadOp::List {
            project_id,
            archived,
            limit,
            before,
        } => {
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
        ThreadOp::Metadata { id, plugin_id } => {
            thread(this, &id, cx)?;
            database
                .call(move |store| {
                    Ok(OpResult::Metadata(
                        store.thread_metadata(&id, &plugin_id)?.unwrap_or_default(),
                    ))
                })
                .await
        }
        ThreadOp::Events {
            id,
            after_seq,
            limit,
            order,
        } => {
            thread(this, &id, cx)?;
            let order = order.unwrap_or(Order::Asc);
            let events = database
                .call(move |store| store.events_after(&id, after_seq, limit, order))
                .await?;
            Ok(OpResult::Events(events))
        }
        ThreadOp::Tools { id } => {
            let entity = thread(this, &id, cx)?;
            crate::skill::tools(this, &entity, cx).await
        }
        ThreadOp::CallTool { id, name, input } => call_tool(this, id, name, input, cx).await,
        ThreadOp::Interactions { id } => {
            thread(this, &id, cx)?;
            database
                .call(move |store| Ok(OpResult::Interactions(store.pending_interactions(&id)?)))
                .await
        }
        ThreadOp::WaitEvent { .. } => Err(OpError::Invalid(
            "wait events must use Workspace::dispatch".into(),
        )),
        ThreadOp::Wait { .. } => Err(OpError::Invalid("wait must use Workspace::dispatch".into())),
        ThreadOp::Search { .. } => Err(OpError::Invalid(
            "search must use Workspace::dispatch".into(),
        )),
        ThreadOp::Run(_)
        | ThreadOp::Stop { .. }
        | ThreadOp::Input { .. }
        | ThreadOp::Resize { .. }
        | ThreadOp::Screen { .. }
        | ThreadOp::Scroll { .. }
        | ThreadOp::Select { .. }
        | ThreadOp::Copy { .. }
        | ThreadOp::Paste { .. }
        | ThreadOp::Send { .. }
        | ThreadOp::Report { .. }
        | ThreadOp::Terminal(_)
        | ThreadOp::Details { .. }
        | ThreadOp::Links { .. }
        | ThreadOp::Archive { .. }
        | ThreadOp::Unarchive { .. }
        | ThreadOp::Delete { .. }
        | ThreadOp::SetPinned { .. }
        | ThreadOp::Rename { .. }
        | ThreadOp::Ask { .. }
        | ThreadOp::MarkRead { .. }
        | ThreadOp::SetMetadata { .. }
        | ThreadOp::StartAgain { .. } => Err(OpError::Invalid("operation changes a thread".into())),
    }
}

async fn call_tool(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    name: String,
    input: serde_json::Value,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let entity = thread(this, &id, cx)?;
    if name == "skill_load" {
        return crate::skill::load(this, &entity, &input, cx).await;
    }
    forward(
        this,
        PluginRequest::CallTool {
            thread_id: id,
            name,
            input,
        },
        cx,
    )
    .await
}
