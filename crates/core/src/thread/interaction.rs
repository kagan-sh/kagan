use gpui_kit::{AsyncApp, Entity, WeakEntity};
use kg_domain::{OpError, OpResult, PendingInteractionId, ThreadId, Timestamp};

use crate::host::database::Result;
use crate::workspace::{Waiting, closed, id, missing, time};
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
    mut waiting: Waiting,
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
        waiting.thread = Some(thread_id);
        workspace.pending.insert(interaction_id, waiting);
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
            role: option.role,
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
            workspace
                .pending
                .iter()
                .find(|(_, waiting)| waiting.thread.as_ref() == Some(&id) && waiting.ask.is_some())
                .map(|(open, _)| open.clone())
        })
        .map_err(closed)?;
    if let Some(open) = open {
        return Ok(OpResult::Text(open.to_string()));
    }
    let (answer, receiver) = async_channel::bounded(1);
    let waiting = Waiting {
        thread: None,
        answer,
        for_a_person: true,
        approval: None,
        ask: Some(receiver),
    };
    let interaction = raise(this, &thread, title, options, fields, waiting, cx).await?;
    Ok(OpResult::Text(interaction.id.to_string()))
}

const MAX_ID: usize = 64;
const MAX_LABEL: usize = 128;
const MAX_TEXT: usize = kg_domain::ASK_TEXT_MAX_CHARS;

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
                required,
            } => {
                let choices: Vec<_> = choices
                    .into_iter()
                    .map(|choice| {
                        Ok(kg_domain::InteractionOption {
                            id: trim_id(&choice.id, "choice")?,
                            label: trim_label(&choice.label, "choice")?,
                            role: None,
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
                    required,
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

fn take_reply(workspace: &mut Workspace, id: &PendingInteractionId) -> Option<Waiting> {
    workspace.pending.remove(id)
}

pub(crate) fn take_thread_replies(
    workspace: &mut Workspace,
    thread_id: &ThreadId,
) -> Vec<PendingInteractionId> {
    let ids: Vec<_> = workspace
        .pending
        .iter()
        .filter(|(_, waiting)| waiting.thread.as_ref() == Some(thread_id))
        .map(|(id, _)| id.clone())
        .collect();
    for id in &ids {
        workspace.pending.remove(id);
    }
    ids
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
    crate::workspace::changed(this, cx)
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
    database.call(move |store| clear_pending(store, &id)).await
}

pub(crate) fn clear_pending(store: &kg_store::Store, id: &ThreadId) -> kg_store::Result<()> {
    for interaction in store.pending_interactions(id)? {
        store.delete_pending_interaction(&interaction.id)?;
    }
    Ok(())
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
        if let Some(waiting) = take_reply(workspace, &interaction_id) {
            waiting.answer.try_send(option_id).map_err(closed)
        } else {
            Err(OpError::Invalid(
                "nothing is waiting for this answer any more".into(),
            ))
        }
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
    crate::workspace::changed(this, cx)?;
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
            (
                kg_domain::AskField::Select {
                    required: false, ..
                },
                serde_json::Value::Null,
            ) => serde_json::Value::Null,
            (kg_domain::AskField::Select { required: true, .. }, serde_json::Value::Null) => {
                return Err(OpError::Invalid(format!(
                    "ask answer field {} needs a choice",
                    field.id()
                )));
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
