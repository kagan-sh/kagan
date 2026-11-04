use async_channel::Sender;
use gpui_kit::{AsyncApp, Entity, WeakEntity};
use kg_domain::{OpError, OpResult, PendingInteractionId, ThreadId, Timestamp};

use crate::database::Result;
use crate::workspace::{closed, id, missing, time};
use crate::{Thread, Workspace};

use super::thread;

pub(crate) struct Raised {
    pub(crate) id: PendingInteractionId,
    thread_id: ThreadId,
    asked_at: Timestamp,
    attention_before: Timestamp,
}

pub(crate) async fn raise(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    title: String,
    options: Vec<kg_domain::InteractionOption>,
    keep: impl FnOnce(&mut Workspace, &ThreadId, PendingInteractionId),
    cx: &mut AsyncApp,
) -> Result<Raised> {
    let interaction = kg_domain::PendingInteraction {
        id: kg_domain::PendingInteractionId(id("interaction")),
        thread_id: thread.read_with(cx, |thread, _| thread.record.id.clone()),
        title,
        options,
        created_at: time(this, cx)?,
    };
    let saved = interaction.clone();
    let database = crate::workspace::database(this, cx)?;
    let attention_before = database
        .call(move |store| {
            let before = store
                .thread(&saved.thread_id)?
                .map_or(saved.created_at, |thread| thread.latest_attention_at);
            store.put_pending_interaction(&saved)?;
            store.set_attention(&saved.thread_id, saved.created_at)?;
            Ok(before)
        })
        .await?;
    let thread_id = interaction.thread_id.clone();
    let interaction_id = interaction.id.clone();
    let created_at = interaction.created_at;
    let raised = Raised {
        id: interaction_id.clone(),
        thread_id: thread_id.clone(),
        asked_at: created_at,
        attention_before,
    };
    this.update(cx, |workspace, cx| {
        keep(workspace, &thread_id, interaction_id);
        thread.update(cx, |thread, cx| {
            thread.record.latest_attention_at = created_at;
            cx.notify();
        });
        cx.emit(crate::WorkspaceEvent::Changed);
        cx.notify();
    })
    .map_err(closed)?;
    Ok(raised)
}

pub(crate) async fn ask(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    title: String,
    options: Vec<kg_domain::InteractionOption>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let title = title.trim();
    if title.is_empty() {
        return Err(OpError::Invalid("ask needs a title".into()));
    }
    let options: Vec<_> = options
        .into_iter()
        .map(|option| kg_domain::InteractionOption {
            id: option.id.trim().to_owned(),
            label: option.label.trim().to_owned(),
        })
        .collect();
    if options.len() < 2
        || options
            .iter()
            .any(|option| option.id.is_empty() || option.label.is_empty())
    {
        return Err(OpError::Invalid(
            "ask needs at least two options, each with an id and a label".into(),
        ));
    }
    let title = title.to_owned();
    let thread = thread(this, &id, cx)?;
    if thread.read_with(cx, |thread, _| thread.record.archived_at.is_some()) {
        return Err(OpError::Invalid(format!(
            "thread {id} is archived; ask from an active thread"
        )));
    }
    let open = this
        .update(cx, |workspace, _| {
            workspace.asks.get(&id).map(|open| open.0.clone())
        })
        .map_err(closed)?;
    if let Some(open) = open {
        return Ok(OpResult::Text(open.to_string()));
    }
    let (sender, receiver) = async_channel::bounded(1);
    // The receiver stays in the workspace so the person's answer still has a live channel.
    let interaction = raise(
        this,
        &thread,
        title,
        options,
        |workspace, thread_id, interaction| {
            workspace.for_a_person.insert(interaction.clone());
            insert_reply(workspace, thread_id.clone(), interaction.clone(), sender);
            workspace
                .asks
                .insert(thread_id.clone(), (interaction, receiver));
        },
        cx,
    )
    .await?;
    Ok(OpResult::Text(interaction.id.to_string()))
}

pub(crate) fn insert_reply(
    workspace: &mut Workspace,
    thread_id: ThreadId,
    id: PendingInteractionId,
    answer: Sender<String>,
) {
    workspace.interaction_thread.insert(id.clone(), thread_id);
    workspace.replies.insert(id, answer);
}

fn take_reply(workspace: &mut Workspace, id: &PendingInteractionId) -> Option<Sender<String>> {
    let answer = workspace.replies.remove(id)?;
    workspace.interaction_thread.remove(id);
    workspace.for_a_person.remove(id);
    Some(answer)
}

pub(crate) fn take_thread_replies(
    workspace: &mut Workspace,
    thread_id: &ThreadId,
) -> Vec<PendingInteractionId> {
    workspace.asks.remove(thread_id);
    let ids: Vec<_> = workspace
        .interaction_thread
        .iter()
        .filter(|(_, owner)| *owner == thread_id)
        .map(|(id, _)| id.clone())
        .collect();
    ids.into_iter()
        .filter(|id| take_reply(workspace, id).is_some())
        .collect()
}

/// Drops a question whose requester went away, so no answer can reach it, and the attention it
/// raised unless newer attention came since.
pub(crate) async fn withdraw(
    this: &WeakEntity<Workspace>,
    raised: Raised,
    cx: &mut AsyncApp,
) -> Result<()> {
    let database = this
        .update(cx, |workspace, _| {
            take_reply(workspace, &raised.id);
            workspace.database.clone()
        })
        .map_err(closed)?;
    database
        .call(move |store| {
            store.remove_pending_interaction(&raised.id)?;
            if store
                .thread(&raised.thread_id)?
                .is_some_and(|thread| thread.latest_attention_at == raised.asked_at)
            {
                store.set_attention(&raised.thread_id, raised.attention_before)?;
            }
            Ok(())
        })
        .await?;
    Workspace::refresh(this, cx).await?;
    this.update(cx, |_, cx| {
        cx.emit(crate::WorkspaceEvent::Changed);
        cx.notify();
    })
    .map_err(closed)
}

pub(crate) async fn remove_interactions(
    this: &WeakEntity<Workspace>,
    ids: Vec<kg_domain::PendingInteractionId>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let database = crate::workspace::database(this, cx)?;
    database
        .call(move |store| {
            for id in &ids {
                store.remove_pending_interaction(id)?;
            }
            Ok(())
        })
        .await
}

pub(crate) async fn clear_interactions(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let id = thread.read_with(cx, |thread, _| thread.record.id.clone());
    let database = crate::workspace::database(this, cx)?;
    database
        .call(move |store| {
            for interaction in store.pending_interactions(&id)? {
                store.remove_pending_interaction(&interaction.id)?;
            }
            Ok(())
        })
        .await
}

pub(crate) async fn answer(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    interaction_id: kg_domain::PendingInteractionId,
    option_id: String,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    thread(this, &id, cx)?;
    let database = crate::workspace::database(this, cx)?;
    let pending_id = id.clone();
    let pending = database
        .call(move |store| store.pending_interactions(&pending_id))
        .await?;
    let interaction = pending
        .iter()
        .find(|interaction| interaction.id == interaction_id)
        .ok_or_else(|| missing("pending interaction", &interaction_id))?;
    if !interaction
        .options
        .iter()
        .any(|option| option.id == option_id)
    {
        return Err(OpError::Invalid("unknown interaction option".into()));
    }
    let title = interaction.title.clone();
    let chosen = option_id.clone();
    this.update(cx, |workspace, _| {
        let sent = if let Some(answer) = take_reply(workspace, &interaction_id) {
            answer.try_send(option_id).map_err(closed)
        } else {
            Err(OpError::Invalid(
                "nothing is waiting for this answer any more".into(),
            ))
        };
        workspace.asks.retain(|_, open| open.0 != interaction_id);
        sent
    })
    .map_err(closed)??;
    database
        .call(move |store| store.remove_pending_interaction(&interaction_id))
        .await?;
    Workspace::events(
        this,
        id,
        vec![kg_domain::EventKind::Answered {
            title,
            option: chosen,
        }],
        cx,
    )
    .await?;
    this.update(cx, |_, cx| {
        cx.emit(crate::WorkspaceEvent::Changed);
        cx.notify();
    })
    .map_err(closed)?;
    Ok(OpResult::Done)
}
