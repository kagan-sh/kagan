pub(crate) mod agent;
pub(crate) mod archive;
mod conversation;
mod details;
mod extra;
mod halt;
mod interaction;
pub(crate) mod notify;
mod read;
mod run;
mod search;
mod terminal;
pub(crate) mod turn_state;

pub(crate) use agent::note_with;

pub(crate) use extra::Extras;
pub(crate) use halt::{retire, stop};
pub(crate) use interaction::{answer, clear_pending, raise, withdraw};
pub(crate) use read::{wait, wait_event};
pub(crate) use run::resolve_program;
pub(crate) use run::{PreparedRun, commit as commit_run, prepare as prepare_run};
pub(crate) use search::search;
pub(crate) use terminal::StopReason;
pub(crate) use terminal::exited;
#[cfg(test)]
pub(crate) use terminal::remember;
pub(crate) use terminal::start as start_terminal;
pub(crate) use terminal::start_again;
pub(crate) use terminal::{Run, Shown};

use gpui_kit::{AsyncApp, Entity, WeakEntity};
use kg_domain::{OpError, OpResult, PluginId, ThreadId, ThreadOp};

use crate::host::database::{Database, Result};
use crate::workspace::{closed, missing, time};
use crate::{Requester, Thread, Workspace};

use archive::{archive, delete, unarchive};

async fn set_notify(
    this: &WeakEntity<Workspace>,
    database: Database,
    id: ThreadId,
    level: kg_domain::ThreadNotify,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    database
        .call(move |store| store.set_notify(&id, level))
        .await?;
    Workspace::refresh(this, cx).await?;
    Ok(OpResult::Done)
}

async fn set_pinned(
    this: &WeakEntity<Workspace>,
    database: Database,
    id: ThreadId,
    pinned: bool,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    database
        .call(move |store| store.set_pinned(&id, pinned))
        .await?;
    Workspace::refresh(this, cx).await?;
    Ok(OpResult::Done)
}

async fn mark_read(
    this: &WeakEntity<Workspace>,
    database: Database,
    id: ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let at = time(this, cx)?;
    database.call(move |store| store.mark_read(&id, at)).await?;
    Workspace::refresh(this, cx).await?;
    Ok(OpResult::Done)
}

async fn set_metadata(
    this: &WeakEntity<Workspace>,
    database: Database,
    id: ThreadId,
    plugin_id: PluginId,
    value: serde_json::Value,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    thread(this, &id, cx)?;
    database
        .call(move |store| store.set_thread_metadata(&id, &plugin_id, &value))
        .await?;
    crate::workspace::changed(this, cx)?;
    Ok(OpResult::Done)
}

pub(crate) async fn dispatch_thread(
    this: &WeakEntity<Workspace>,
    database: Database,
    op: ThreadOp,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        ThreadOp::Run(_)
        | ThreadOp::Wait { .. }
        | ThreadOp::WaitEvent { .. }
        | ThreadOp::Search { .. } => Err(OpError::Invalid(
            "this operation must use Workspace::dispatch".into(),
        )),
        ThreadOp::Stop { id } => {
            let thread = thread(this, &id, cx)?;
            stop(this, &thread, cx).await?;
            Ok(OpResult::Done)
        }
        op @ (ThreadOp::Input { .. }
        | ThreadOp::Resize { .. }
        | ThreadOp::Screen { .. }
        | ThreadOp::Scroll { .. }
        | ThreadOp::Select { .. }
        | ThreadOp::Copy { .. }
        | ThreadOp::Paste { .. }
        | ThreadOp::Report { .. }) => terminal::run(this, op, cx),
        ThreadOp::Send { id, text } => terminal::send(this, id, &text, cx).await,
        ThreadOp::Turn(op) => terminal::turn_hook(this, op, cx).await,
        ThreadOp::Terminal(op) => extra::run(this, op, cx).await,
        ThreadOp::Details { id } => details::read(database, id, cx).await,
        ThreadOp::Links { id } => extra::links(this, id, cx).await,
        ThreadOp::Archive { id } => archive(this, database, id, cx).await,
        ThreadOp::Unarchive { id } => unarchive(this, database, id, cx).await,
        ThreadOp::Delete {
            id,
            approval_thread_id,
        } => delete(this, database, id, approval_thread_id, requester, cx).await,
        ThreadOp::Notify { id, level } => set_notify(this, database, id, level, cx).await,
        ThreadOp::SetPinned { id, pinned } => set_pinned(this, database, id, pinned, cx).await,
        ThreadOp::Rename { id, title } => rename(this, database, id, title, true, cx).await,
        ThreadOp::Ask {
            id,
            title,
            options,
            fields,
        } => interaction::ask(this, id, title, options, fields, cx).await,
        ThreadOp::MarkRead { id } => mark_read(this, database, id, cx).await,
        ThreadOp::SetMetadata {
            id,
            plugin_id,
            value,
        } => set_metadata(this, database, id, plugin_id, value, cx).await,
        ThreadOp::StartAgain { id } => terminal::start_again(this, id, cx).await,
        ThreadOp::Get { id } => read::get(this, &id, cx),
        ThreadOp::List {
            project_id,
            archived,
            limit,
            before,
        } => read::list(database, project_id, archived, limit, before).await,
        ThreadOp::Metadata { id, plugin_id } => {
            read::metadata(this, database, id, plugin_id, cx).await
        }
        ThreadOp::Events {
            id,
            after_seq,
            limit,
            order,
        } => read::events(this, database, id, after_seq, limit, order, cx).await,
        ThreadOp::Tools { id } => {
            let entity = thread(this, &id, cx)?;
            crate::skill::tools(this, &entity, cx).await
        }
        ThreadOp::CallTool { id, name, input } => read::call_tool(this, id, name, input, cx).await,
        ThreadOp::Interactions { id } => read::interactions(this, database, id, cx).await,
    }
}

pub(crate) async fn rename_once(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    title: String,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let database = crate::workspace::database(this, cx)?;
    rename(this, database, id, title, false, cx).await
}

pub(crate) fn clean_title(title: &str) -> Result<String> {
    let title: String = title.chars().filter(|ch| !ch.is_control()).collect();
    let title = title.trim();
    if title.is_empty() {
        return Err(OpError::Invalid("a thread needs a title".into()));
    }
    Ok(title.to_owned())
}

async fn rename(
    this: &WeakEntity<Workspace>,
    database: Database,
    id: ThreadId,
    title: String,
    force: bool,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let title = clean_title(&title)?;
    let stored = id.clone();
    let wrote = database
        .call(move |store| {
            if force {
                store.set_title(&stored, &title)?;
                Ok(true)
            } else {
                store.set_title_once(&stored, &title)
            }
        })
        .await?;
    if !wrote {
        return Err(OpError::Invalid(
            "the title is kept; rename it in the window".into(),
        ));
    }
    Workspace::refresh(this, cx).await?;
    Ok(OpResult::Thread(
        thread(this, &id, cx)?.read_with(cx, |thread, _| thread.record.clone()),
    ))
}

pub(crate) fn thread(
    this: &WeakEntity<Workspace>,
    id: &ThreadId,
    cx: &mut AsyncApp,
) -> Result<Entity<Thread>> {
    this.update(cx, |workspace, _| workspace.threads.get(id).cloned())
        .map_err(closed)?
        .ok_or_else(|| missing("thread", id))
}

pub(crate) async fn ready_environment(
    database: &Database,
    id: &ThreadId,
) -> Result<kg_domain::Environment> {
    let lookup = id.clone();
    let record = database
        .call(move |store| store.thread(&lookup))
        .await?
        .ok_or_else(|| missing("thread", id))?;
    let environment_id = record
        .environment_id
        .ok_or_else(|| OpError::Invalid("thread has no environment".into()))?;
    let lookup = environment_id.clone();
    let environment = database
        .call(move |store| store.environment(&lookup))
        .await?
        .ok_or_else(|| missing("environment", &environment_id))?;
    if environment.status != kg_domain::EnvironmentStatus::Ready {
        return Err(OpError::Failed("environment is not ready".into()));
    }
    Ok(environment)
}

pub(crate) fn child_notice(title: &str, error: Option<&str>, before_turn: bool) -> String {
    let because = error.map(|error| format!(": {error}")).unwrap_or_default();
    if before_turn {
        format!("Child thread \"{title}\" failed before its first turn{because}.")
    } else {
        format!("Child thread \"{title}\" failed{because}.")
    }
}
