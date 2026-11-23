use std::future::Future;
use std::time::Duration;

use gpui_shell::{HostArguments, HostError, HostModule, HostValue};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{
    Event, EventKind, InteractionOption, Link, NoticeSeverity, Op, OpResult, OptionRole, Order,
    PendingInteraction, PendingInteractionId, PlanStatus, Role, ThreadId, ThreadOp, Timestamp,
    ToolKind, ToolOutcome, ToolStatus, TurnOutcome,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::area::{argument, awaited, dispatch, error, from_host, to_host, with_app};
use crate::threads::shown_thread;

/// A call's default and most items, the page's byte ceiling, the longest a wait runs, and the two
/// fields a tool call may carry too much of. All six are in the d.ts.
const DEFAULT: u32 = 100;
const MOST: u32 = 200;
const PAGE: usize = 256 * 1024;
const WAIT: u64 = 60_000;
const ARGS: usize = 4 * 1024;
const OUTPUT: usize = 8 * 1024;

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
    Thought {
        text: String,
        message_id: Option<String>,
    },
    Work {
        work: Work,
    },
    Plan {
        plan: Plan,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    args_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_truncated: Option<bool>,
}

impl Work {
    fn new(id: String, title: String, status: &'static str) -> Self {
        Self {
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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Plan {
    steps: Vec<Step>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Step {
    step: String,
    status: &'static str,
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
        EventKind::Thought { text, message_id } => Body::Thought {
            text: text.clone(),
            message_id: message_id.clone(),
        },
        EventKind::Plan { entries } => Body::Plan {
            plan: Plan {
                steps: entries
                    .iter()
                    .map(|entry| Step {
                        step: entry.text.clone(),
                        status: step_status(entry.status),
                    })
                    .collect(),
            },
        },
        EventKind::ToolStarted { id, name } => Body::Work {
            work: Work::new(id.clone(), name.clone(), "running"),
        },
        EventKind::ToolEnded { id, status } => Body::Work {
            work: Work::new(
                id.clone(),
                String::new(),
                match status {
                    ToolOutcome::Ok => "ok",
                    ToolOutcome::Error => "error",
                },
            ),
        },
        EventKind::Tool {
            id,
            name,
            kind,
            title,
            status,
            input,
            output,
            ..
        } => {
            let (args, args_truncated) = capped_args(input.as_ref());
            let (output, output_truncated) = capped_output(output.as_deref());
            let (tool, named) = classify(name.as_deref(), title.as_deref(), *kind, input.as_ref());
            Body::Work {
                work: Work {
                    id: id.clone(),
                    tool,
                    title: named
                        .or_else(|| title.clone())
                        .or_else(|| name.clone())
                        .unwrap_or_default(),
                    status: tool_status(*status),
                    args,
                    args_truncated,
                    output,
                    output_truncated,
                },
            }
        }
        EventKind::Notice { severity, text } => Body::Notice {
            severity: *severity,
            text: text.clone(),
        },
        EventKind::TurnEnded { outcome, error } => Body::Turn {
            outcome: *outcome,
            error: error.clone(),
        },
        // Usage belongs to the header line, and the rest is turn state the row already shows.
        _ => return None,
    };
    Some(Item {
        seq: event.seq,
        thread_id: event.thread_id.clone(),
        at: event.at,
        body,
    })
}

/// What a work call is about, and the name kagan calls it by: an MCP call by its own name, a skill
/// load, then the agent's kind. The rules are the host's rows' rules, so a page reads as the
/// built-in row does.
fn classify(
    name: Option<&str>,
    title: Option<&str>,
    kind: Option<ToolKind>,
    input: Option<&Value>,
) -> (&'static str, Option<String>) {
    if let Some(rest) = name.and_then(|name| name.strip_prefix("mcp__"))
        && let Some((server, called)) = rest.split_once("__")
    {
        return ("mcp", Some(format!("{server}.{called}")));
    }
    // kagan writes its own MCP server as `kagan_<tool>`.
    if let Some((server, called)) = name.and_then(|name| name.split_once('_'))
        && server == "kagan"
    {
        return ("mcp", Some(format!("{server}.{called}")));
    }
    let loads_skill = name == Some("skill_load")
        || (title.is_some_and(|title| title.starts_with("load skill"))
            && named_skill(input).is_some());
    if loads_skill {
        let named = named_skill(input)
            .or_else(|| title.and_then(|title| title.strip_prefix("load skill ")))
            .map(str::to_owned)
            .unwrap_or_else(|| "a skill".into());
        return ("skill", Some(named));
    }
    (tool(kind), None)
}

fn named_skill(input: Option<&Value>) -> Option<&str> {
    input?.get("name")?.as_str()
}

fn tool(kind: Option<ToolKind>) -> &'static str {
    match kind {
        Some(ToolKind::Read) => "read",
        Some(ToolKind::Edit) => "edit",
        Some(ToolKind::Delete) => "delete",
        Some(ToolKind::Move) => "move",
        Some(ToolKind::Search) => "search",
        Some(ToolKind::Execute) => "execute",
        Some(ToolKind::Think) => "think",
        Some(ToolKind::Fetch) => "fetch",
        Some(ToolKind::SwitchMode | ToolKind::Other) | None => "other",
    }
}

fn tool_status(status: Option<ToolStatus>) -> &'static str {
    match status {
        Some(ToolStatus::Completed) => "ok",
        Some(ToolStatus::Failed) => "error",
        Some(ToolStatus::Pending | ToolStatus::InProgress) | None => "running",
    }
}

fn step_status(status: PlanStatus) -> &'static str {
    match status {
        PlanStatus::Pending => "pending",
        PlanStatus::InProgress => "running",
        PlanStatus::Completed => "done",
    }
}

/// The field cut to its cap, and whether anything was cut. The cut lands on a character boundary,
/// so a letter outside ASCII is never split in two.
fn capped(text: &str, cap: usize) -> (String, bool) {
    let mut end = text.len().min(cap);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), end < text.len())
}

/// The raw input as the agent sent it, or its own text cut to `ARGS` when that is longer. A cut
/// `args` is a string holding the start of the JSON, which `argsTruncated` flags.
fn capped_args(input: Option<&Value>) -> (Option<Value>, Option<bool>) {
    let Some(input) = input else {
        return (None, None);
    };
    let (text, over) = capped(&input.to_string(), ARGS);
    if over {
        (Some(Value::String(text)), Some(true))
    } else {
        (Some(input.clone()), None)
    }
}

fn capped_output(output: Option<&str>) -> (Option<String>, Option<bool>) {
    let Some(output) = output else {
        return (None, None);
    };
    let (text, over) = capped(output, OUTPUT);
    (Some(text), over.then_some(true))
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
