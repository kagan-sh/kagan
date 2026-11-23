use std::future::Future;
use std::time::Duration;

use gpui_shell::{HostArguments, HostError, HostModule, HostValue};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{
    Event, EventKind, InteractionOption, Link, NoticeSeverity, Op, OpResult, OptionRole, Order,
    PendingInteraction, PendingInteractionId, Role, ThreadId, ThreadOp, Timestamp, TurnOutcome,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::area::{argument, awaited, dispatch, error, from_host, to_host, with_app};
use crate::threads::shown_thread;

/// A call's default and most items, the page's byte ceiling, and the longest a wait runs. All four
/// are in the d.ts.
const DEFAULT: u32 = 100;
const MOST: u32 = 200;
const PAGE: usize = 256 * 1024;
const WAIT: u64 = 60_000;

pub(crate) fn module() -> HostModule {
    HostModule::new("kagan/chat")
        .declarations(include_str!("chat.d.ts"))
        .async_function("items", items)
        .async_function("waitItem", wait_item)
        .async_function("approvals", approvals)
        .async_function("send", send)
        .async_function("cancel", cancel)
        .async_function("draft", crate::ui_host::draft)
        .async_function("setDraft", crate::ui_host::set_draft)
}

/// One item of a transcript, as a plugin reads it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Item {
    seq: u64,
    thread_id: ThreadId,
    at: Timestamp,
    #[serde(flatten)]
    body: Body,
}

#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum Body {
    Message {
        role: Role,
        text: String,
        message_id: Option<String>,
    },
    Work {
        work: Work,
    },
    Notice {
        severity: NoticeSeverity,
        text: String,
    },
    Turn {
        outcome: TurnOutcome,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Work {
    id: String,
    tool: &'static str,
    title: String,
    status: &'static str,
    // Slice 2's `Tool` event carries these; today's does not.
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    args_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_truncated: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Approval {
    interaction_id: PendingInteractionId,
    title: String,
    options: Vec<OptionShown>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OptionShown {
    id: String,
    label: String,
    role: Option<OptionRole>,
}

/// One logged event as a transcript item, or nothing when the transcript does not draw it.
pub(crate) fn item(event: &Event) -> Option<Item> {
    let body = match &event.kind {
        EventKind::Message {
            role,
            text,
            message_id,
            ..
        } => Body::Message {
            role: *role,
            text: text.clone(),
            message_id: message_id.clone(),
        },
        EventKind::ToolStarted { id, name } => Body::Work {
            work: work(id.clone(), name.clone(), "running"),
        },
        EventKind::ToolEnded { id, status } => Body::Work {
            work: work(
                id.clone(),
                String::new(),
                match status {
                    kg_domain::ToolOutcome::Ok => "ok",
                    kg_domain::ToolOutcome::Error => "error",
                },
            ),
        },
        EventKind::Tool { id, name, status } => Body::Work {
            work: work(
                id.clone(),
                name.clone().unwrap_or_default(),
                match status {
                    kg_domain::ToolStatus::Pending | kg_domain::ToolStatus::InProgress => "running",
                    kg_domain::ToolStatus::Completed => "ok",
                    kg_domain::ToolStatus::Failed => "error",
                },
            ),
        },
        EventKind::Notice { severity, text } => Body::Notice {
            severity: *severity,
            text: text.clone(),
        },
        EventKind::TurnEnded { outcome, error } => Body::Turn {
            outcome: *outcome,
            error: error.clone(),
        },
        _ => return None,
    };
    Some(Item {
        seq: event.seq,
        thread_id: event.thread_id.clone(),
        at: event.at,
        body,
    })
}

fn work(id: String, title: String, status: &'static str) -> Work {
    Work {
        id,
        tool: "other",
        title,
        status,
        args: None,
        args_truncated: None,
        output: None,
        output_truncated: None,
    }
}

/// A page is a prefix of the asked order, and holds one item even when that item alone is over the
/// ceiling: a message can be a whole MiB, and a page that skipped it would never return it.
pub(crate) fn page(items: Vec<Item>) -> serde_json::Result<Value> {
    let mut kept = Vec::new();
    let mut bytes = 0;
    for item in items {
        let value = serde_json::to_value(&item)?;
        let size = value.to_string().len() + 1;
        if !kept.is_empty() && bytes + size > PAGE {
            break;
        }
        bytes += size;
        kept.push(value);
    }
    Ok(Value::Array(kept))
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Options {
    after_seq: Option<u64>,
    limit: Option<u32>,
    order: Option<Order>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WaitOptions {
    after_seq: u64,
    timeout_ms: u64,
}

/// A thread a chat call may read or drive: one kagan lists, and one whose transcript is its events.
pub(crate) fn chat(id: &ThreadId, cx: &gpui_kit::App) -> Result<(), HostError> {
    shown_thread(id, cx)?;
    let acp = Workspace::global(cx)
        .read(cx)
        .thread(id)
        .is_some_and(|thread| thread.read(cx).record().link == Link::Acp);
    if acp {
        Ok(())
    } else {
        Err(HostError::new(format!(
            "thread {id} is not a chat; its transcript is its terminal"
        )))
    }
}

fn items(
    arguments: &HostArguments,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let id = ThreadId(arguments.string(0)?.to_owned());
    let Options {
        after_seq,
        limit,
        order,
    } = optional(arguments, 1)?;
    if limit.is_some_and(|limit| limit > MOST) {
        return Err(HostError::new(format!("at most {MOST} items a call")));
    }
    let order = order.unwrap_or(Order::Desc);
    let logged = with_app(|cx| {
        chat(&id, cx)?;
        Ok::<_, HostError>(awaited(
            Workspace::dispatch(
                Op::Thread(ThreadOp::Events {
                    id: id.clone(),
                    after_seq,
                    limit: Some(limit.unwrap_or(DEFAULT)),
                    order: Some(order),
                }),
                cx,
            ),
            cx,
        ))
    })??;
    Ok(async move {
        let OpResult::Events(events) = logged.await?.map_err(error)? else {
            return Err(HostError::new("the thread's events did not read as events"));
        };
        let shown: Vec<Item> = events.iter().filter_map(item).collect();
        Ok(to_host(page(shown).map_err(error)?))
    })
}

/// The open approvals and questions of a chat, oldest first, over the same read the host draws from.
fn approvals(
    arguments: &HostArguments,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let id = ThreadId(arguments.string(0)?.to_owned());
    let asked = with_app(|cx| {
        chat(&id, cx)?;
        Ok::<_, HostError>(awaited(
            Workspace::dispatch(Op::Thread(ThreadOp::Interactions { id: id.clone() }), cx),
            cx,
        ))
    })??;
    Ok(async move {
        let OpResult::Interactions(pending) = asked.await.map_err(error)?.map_err(error)? else {
            return Err(HostError::new(
                "the thread's approvals did not read as approvals",
            ));
        };
        let shown: Vec<Approval> = pending.into_iter().map(approval).collect();
        Ok(to_host(serde_json::to_value(shown).map_err(error)?))
    })
}

fn approval(interaction: PendingInteraction) -> Approval {
    Approval {
        interaction_id: interaction.id,
        title: interaction.title,
        options: interaction.options.into_iter().map(option).collect(),
    }
}

fn option(option: InteractionOption) -> OptionShown {
    OptionShown {
        id: option.id,
        label: option.label,
        role: option.role,
    }
}

fn optional<T: serde::de::DeserializeOwned + Default>(
    arguments: &HostArguments,
    ix: usize,
) -> Result<T, HostError> {
    arguments.get(ix).map_or_else(
        || Ok(T::default()),
        |value| serde_json::from_value(from_host(value)).map_err(error),
    )
}

fn wait_item(
    arguments: &HostArguments,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let id = ThreadId(arguments.string(0)?.to_owned());
    let WaitOptions {
        after_seq,
        timeout_ms,
    } = argument(arguments, 1)?;
    let wait = with_app(|cx| {
        chat(&id, cx)?;
        let task = wait_in(id.clone(), after_seq, wait_duration(timeout_ms), cx);
        Ok::<_, HostError>(awaited(task, cx))
    })??;
    Ok(async move {
        Ok(match wait.await?? {
            Some(item) => to_host(serde_json::to_value(&item).map_err(error)?),
            None => HostValue::Null,
        })
    })
}

enum Wake {
    Logged,
    Asked,
}

pub(crate) fn wait_duration(timeout_ms: u64) -> Duration {
    Duration::from_millis(timeout_ms.min(WAIT))
}

fn wait_in(
    id: ThreadId,
    after_seq: u64,
    timeout: Duration,
    cx: &mut gpui_kit::App,
) -> gpui_kit::Task<Result<Option<Item>, HostError>> {
    let (sender, receiver) = async_channel::unbounded();
    let watched = id.clone();
    // Subscribe before reading the log, so nothing logged between the two is missed.
    let subscription = cx.subscribe(&Workspace::global(cx), move |_, event, _| {
        let wake = match event {
            WorkspaceEvent::Event { event, .. } if event.thread_id == watched => Wake::Logged,
            WorkspaceEvent::Notice(thread, _) if *thread == watched => Wake::Asked,
            _ => return,
        };
        let _ = sender.try_send(wake);
    });
    cx.spawn(async move |cx| {
        let _subscription = subscription;
        let deadline = cx.background_executor().timer(timeout);
        futures::pin_mut!(deadline);
        let mut after = after_seq;
        loop {
            let read = cx.update(|cx| {
                Workspace::dispatch(
                    Op::Thread(ThreadOp::Events {
                        id: id.clone(),
                        after_seq: Some(after),
                        limit: Some(MOST),
                        order: Some(Order::Asc),
                    }),
                    cx,
                )
            });
            let OpResult::Events(events) = read.await.map_err(error)? else {
                return Err(HostError::new("the thread's events did not read as events"));
            };
            let full = events.len() == MOST as usize;
            for event in events {
                after = event.seq;
                if let Some(item) = item(&event) {
                    return Ok(Some(item));
                }
            }
            // A full batch may be hiding the next item behind it, so read again before waiting.
            if full {
                continue;
            }
            let next = receiver.recv();
            futures::pin_mut!(next);
            match futures::future::select(deadline.as_mut(), next).await {
                futures::future::Either::Left(_)
                | futures::future::Either::Right((Ok(Wake::Asked) | Err(_), _)) => {
                    return Ok(None);
                }
                futures::future::Either::Right((Ok(Wake::Logged), _)) => {}
            }
        }
    })
}

fn send(
    arguments: &HostArguments,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let id = ThreadId(arguments.string(0)?.to_owned());
    let text = arguments.string(1)?.to_owned();
    with_app(|cx| chat(&id, cx))??;
    dispatch(Op::Thread(ThreadOp::Input {
        id,
        terminal: None,
        text,
    }))
}

fn cancel(
    arguments: &HostArguments,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let id = ThreadId(arguments.string(0)?.to_owned());
    with_app(|cx| chat(&id, cx))??;
    dispatch(Op::Thread(ThreadOp::Cancel { id }))
}
