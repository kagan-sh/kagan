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
    fields: Option<Vec<kg_domain::AskField>>,
    keep: impl FnOnce(&mut Workspace, &ThreadId, PendingInteractionId),
    cx: &mut AsyncApp,
) -> Result<Raised> {
    let interaction = kg_domain::PendingInteraction {
        id: kg_domain::PendingInteractionId(id("interaction")),
        thread_id: thread.read_with(cx, |thread, _| thread.record.id.clone()),
        title,
        options,
        fields,
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
    fields: Option<Vec<kg_domain::AskField>>,
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
    let fields = match fields {
        None => Some(Vec::new()),
        Some(fields) => Some(validate_fields(fields)?),
    };
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
        fields,
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

const MAX_ID: usize = 64;
const MAX_LABEL: usize = 128;
const MAX_TEXT: usize = 4096;

fn validate_fields(fields: Vec<kg_domain::AskField>) -> Result<Vec<kg_domain::AskField>> {
    if fields.len() > 32 {
        return Err(OpError::Invalid("ask allows at most 32 fields".into()));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut cleaned = Vec::with_capacity(fields.len());
    for field in fields {
        let cleaned_field = match field {
            kg_domain::AskField::String {
                id,
                label,
                placeholder,
                default,
            } => {
                let id = trim_id(&id, "field")?;
                let label = trim_label(&label, "field")?;
                kg_domain::AskField::String {
                    id,
                    label,
                    placeholder: trim_text(placeholder, "placeholder")?,
                    default: trim_text(default, "default")?,
                }
            }
            kg_domain::AskField::Boolean { id, label, default } => kg_domain::AskField::Boolean {
                id: trim_id(&id, "field")?,
                label: trim_label(&label, "field")?,
                default,
            },
            kg_domain::AskField::Number { id, label, default } => kg_domain::AskField::Number {
                id: trim_id(&id, "field")?,
                label: trim_label(&label, "field")?,
                default,
            },
            kg_domain::AskField::Select {
                id,
                label,
                choices,
                default,
            } => {
                let choices: Vec<_> = choices
                    .into_iter()
                    .map(|choice| {
                        Ok(kg_domain::InteractionOption {
                            id: trim_id(&choice.id, "choice")?,
                            label: trim_label(&choice.label, "choice")?,
                        })
                    })
                    .collect::<Result<_>>()?;
                if choices.is_empty() {
                    return Err(OpError::Invalid(
                        "a select field needs at least one choice, each with an id and a label"
                            .into(),
                    ));
                }
                if choices.len() > 32 {
                    return Err(OpError::Invalid(
                        "a select field allows at most 32 choices".into(),
                    ));
                }
                let mut choice_ids = std::collections::BTreeSet::new();
                for choice in &choices {
                    if !choice_ids.insert(choice.id.clone()) {
                        return Err(OpError::Invalid(format!(
                            "select choice id {} is repeated",
                            choice.id
                        )));
                    }
                }
                let default = trim_text(default, "default")?;
                if let Some(default) = &default
                    && !choices.iter().any(|choice| choice.id == *default)
                {
                    return Err(OpError::Invalid(
                        "a select field default must be one of its choices".into(),
                    ));
                }
                kg_domain::AskField::Select {
                    id: trim_id(&id, "field")?,
                    label: trim_label(&label, "field")?,
                    choices,
                    default,
                }
            }
        };
        if !ids.insert(cleaned_field.id().to_owned()) {
            return Err(OpError::Invalid(format!(
                "ask field id {} is repeated",
                cleaned_field.id()
            )));
        }
        cleaned.push(cleaned_field);
    }
    Ok(cleaned)
}

fn trim_id(id: &str, kind: &str) -> Result<String> {
    let id = id.trim();
    if id.is_empty() {
        return Err(OpError::Invalid(format!("{kind} needs an id")));
    }
    if id.chars().count() > MAX_ID {
        return Err(OpError::Invalid(format!(
            "{kind} id is at most {MAX_ID} characters"
        )));
    }
    Ok(id.to_owned())
}

fn trim_label(label: &str, kind: &str) -> Result<String> {
    let label = label.trim();
    if label.is_empty() {
        return Err(OpError::Invalid(format!("{kind} needs a label")));
    }
    if label.chars().count() > MAX_LABEL {
        return Err(OpError::Invalid(format!(
            "{kind} label is at most {MAX_LABEL} characters"
        )));
    }
    Ok(label.to_owned())
}

fn trim_text(value: Option<String>, kind: &str) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > MAX_TEXT {
        return Err(OpError::Invalid(format!(
            "ask {kind} is at most {MAX_TEXT} characters"
        )));
    }
    Ok(Some(trimmed.to_owned()))
}

fn capped_answer(text: String) -> Result<String> {
    if text.chars().count() > MAX_TEXT {
        return Err(OpError::Invalid(format!(
            "ask answer text is at most {MAX_TEXT} characters"
        )));
    }
    Ok(text)
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
            store.delete_pending_interaction(&raised.id)?;
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
                store.delete_pending_interaction(id)?;
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
                store.delete_pending_interaction(&interaction.id)?;
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
    fields: std::collections::BTreeMap<String, serde_json::Value>,
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
    let fields = match &interaction.fields {
        Some(declared) => check_answer_fields(declared, fields)?,
        None if fields.is_empty() => fields,
        None => {
            return Err(OpError::Invalid("this question has no form fields".into()));
        }
    };
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
        .call(move |store| store.delete_pending_interaction(&interaction_id))
        .await?;
    Workspace::events(
        this,
        id,
        vec![kg_domain::EventKind::Answered {
            title,
            option: chosen,
            fields,
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

fn check_answer_fields(
    declared: &[kg_domain::AskField],
    mut fields: std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<std::collections::BTreeMap<String, serde_json::Value>> {
    let mut cleaned = std::collections::BTreeMap::new();
    for field in declared {
        let value = fields.remove(field.id()).ok_or_else(|| {
            OpError::Invalid(format!("ask answer is missing field {}", field.id()))
        })?;
        let value = match (field, value) {
            (kg_domain::AskField::String { .. }, serde_json::Value::String(text)) => {
                serde_json::Value::String(capped_answer(text)?)
            }
            (kg_domain::AskField::Boolean { .. }, serde_json::Value::Bool(flag)) => {
                serde_json::Value::Bool(flag)
            }
            (kg_domain::AskField::Number { .. }, serde_json::Value::Number(number))
                if number.as_i64().is_some() =>
            {
                serde_json::Value::Number(number)
            }
            (kg_domain::AskField::Select { choices, .. }, serde_json::Value::String(choice))
                if choices.iter().any(|option| option.id == choice) =>
            {
                serde_json::Value::String(capped_answer(choice)?)
            }
            _ => {
                return Err(OpError::Invalid(format!(
                    "ask answer field {} has the wrong kind of value",
                    field.id()
                )));
            }
        };
        cleaned.insert(field.id().to_owned(), value);
    }
    if let Some(extra) = fields.keys().next() {
        return Err(OpError::Invalid(format!(
            "ask answer has unknown field {extra}"
        )));
    }
    Ok(cleaned)
}
