use std::future::Future;
use std::time::Duration;

use gpui_shell::{HostArguments, HostError, HostModule, HostValue};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{
    Event, InteractionOption, Link, Op, OpResult, Order, PendingInteraction, PendingInteractionId,
    ThreadId, ThreadOp,
};
use serde::Deserialize;

use crate::area::{argument, awaited, dispatch, error, to_host, with_app};
use crate::threads::{EventsOptions, optional, shown_thread};

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
    let EventsOptions {
        after_seq,
        limit,
        order,
    } = optional(arguments, 1)?;
    with_app(|cx| chat(&id, cx))??;
    dispatch(Op::Thread(ThreadOp::Events {
        id,
        after_seq,
        limit,
        order,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WaitOptions {
    after_seq: u64,
    timeout_ms: u64,
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
            Some(event) => to_host(serde_json::to_value(&event).map_err(error)?),
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
) -> gpui_kit::Task<Result<Option<Event>, HostError>> {
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
        loop {
            let read = cx.update(|cx| {
                Workspace::dispatch(
                    Op::Thread(ThreadOp::Events {
                        id: id.clone(),
                        after_seq: Some(after_seq),
                        limit: Some(1),
                        order: Some(Order::Asc),
                    }),
                    cx,
                )
            });
            let OpResult::Events(events) = read.await.map_err(error)? else {
                return Err(HostError::new("the thread's events did not read as events"));
            };
            if let Some(event) = events.into_iter().next() {
                return Ok(Some(event));
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Approval {
    interaction_id: PendingInteractionId,
    title: String,
    options: Vec<OptionShown>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OptionShown {
    id: String,
    label: String,
    role: Option<kg_domain::OptionRole>,
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
