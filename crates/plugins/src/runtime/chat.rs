use std::future::Future;
use std::time::Duration;

use gpui_shell::{HostArguments, HostError, HostModule, HostValue};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{
    Event, EventKind, InteractionOption, Link, NoticeSeverity, Op, OpResult, OptionRole, Order,
    PendingInteractionId, Role, ThreadId, ThreadOp, Timestamp, TurnOutcome,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::area::{argument, awaited, dispatch, error, from_host, to_host, with_app};
use crate::threads::shown_thread;

/// A call's default and most items, and the page's byte ceiling. All three are in the d.ts.
const DEFAULT: u32 = 100;
const MOST: u32 = 200;
const PAGE: usize = 256 * 1024;

pub(crate) fn module() -> HostModule {
    HostModule::new("kagan/chat")
        .declarations(include_str!("chat.d.ts"))
        .async_function("items", items)
        .async_function("waitItem", wait_item)
        .async_function("send", send)
        .async_function("cancel", cancel)
        .async_function("draft", crate::ui_host::draft)
        .async_function("setDraft", crate::ui_host::set_draft)
}

/// One item of a transcript, as a plugin reads it. `thought` and `plan` are typed here and filled
/// once their events exist.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Item {
    seq: u64,
    thread_id: ThreadId,
    at: Timestamp,
    #[serde(flatten)]
    body: Body,
}

// `thought` and `plan` are typed for the wire before their events exist; the test below pins them.
#[allow(
    dead_code,
    reason = "typed for the wire; slice 2 writes these two events"
)]
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
    Thought {
        text: String,
        done: bool,
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
    Approval {
        approval: Approval,
    },
    Plan {
        plan: Plan,
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

#[derive(Serialize)]
struct Plan {
    steps: Vec<Step>,
}

#[derive(Serialize)]
struct Step {
    step: String,
    status: &'static str,
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
    let (logged, asked) = with_app(|cx| {
        chat(&id, cx)?;
        let logged = awaited(
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
        );
        let asked = awaited(
            Workspace::dispatch(Op::Thread(ThreadOp::Interactions { id: id.clone() }), cx),
            cx,
        );
        Ok::<_, HostError>((logged, asked))
    })??;
    Ok(async move {
        let OpResult::Events(events) = logged.await?.map_err(error)? else {
            return Err(HostError::new("the thread's events did not read as events"));
        };
        let mut shown: Vec<Item> = events.iter().filter_map(item).collect();
        // An approval is drawn after every item, so it cannot hide behind an `afterSeq` the plugin
        // already passed. With no item on the page, the asked cursor is the log's newest.
        let seq = shown
            .iter()
            .map(|item| item.seq)
            .max()
            .or(after_seq)
            .unwrap_or(0);
        if let Ok(OpResult::Interactions(pending)) = asked.await.map_err(error)? {
            shown.extend(pending.into_iter().map(|interaction| Item {
                seq,
                thread_id: interaction.thread_id,
                at: interaction.created_at,
                body: Body::Approval {
                    approval: Approval {
                        interaction_id: interaction.id,
                        title: interaction.title,
                        options: interaction.options.into_iter().map(option).collect(),
                    },
                },
            }));
        }
        Ok(to_host(page(shown).map_err(error)?))
    })
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
        let task = wait_in(id.clone(), after_seq, Duration::from_millis(timeout_ms), cx);
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
            for event in events {
                after = event.seq;
                if let Some(item) = item(&event) {
                    return Ok(Some(item));
                }
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

#[cfg(test)]
#[path = "chat_test.rs"]
mod tests;
