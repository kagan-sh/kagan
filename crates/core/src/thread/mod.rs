pub(crate) mod agent;
pub(crate) mod archive;
mod conversation;
mod details;
mod extra;
mod halt;
mod interaction;
mod read;
mod run;
mod search;
mod terminal;

pub(crate) use agent::{note_with, read_instructions};

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
use kg_domain::{OpError, OpResult, ThreadId, ThreadOp};

use crate::host::database::{Database, Result};
use crate::workspace::{closed, missing, time};
use crate::{Requester, Thread, Workspace};

use archive::{archive, delete, unarchive};
use read::read_thread;

pub(crate) async fn dispatch_thread(
    this: &WeakEntity<Workspace>,
    database: Database,
    op: ThreadOp,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        ThreadOp::Run(_) => Err(OpError::Invalid("run must use Workspace::dispatch".into())),
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
        ThreadOp::Send { id, text } => {
            let typed = terminal::submit(this, &id, &text, cx).await?;
            crate::Workspace::events(
                this,
                id,
                vec![kg_domain::EventKind::Message {
                    role: kg_domain::Role::User,
                    text: typed,
                    message_id: None,
                    attachments: Vec::new(),
                }],
                cx,
            )
            .await?;
            Ok(OpResult::Done)
        }
        ThreadOp::Terminal(op) => extra::run(this, op, cx).await,
        ThreadOp::Details { id } => details::read(database, id, cx).await,
        ThreadOp::Links { id } => extra::links(this, id, cx).await,
        ThreadOp::Archive { id } => archive(this, database, id, cx).await,
        ThreadOp::Unarchive { id } => unarchive(this, database, id, cx).await,
        ThreadOp::Delete {
            id,
            approval_thread_id,
        } => delete(this, database, id, approval_thread_id, requester, cx).await,
        ThreadOp::SetPinned { id, pinned } => {
            database
                .call(move |store| store.set_pinned(&id, pinned))
                .await?;
            Workspace::refresh(this, cx).await?;
            Ok(OpResult::Done)
        }
        ThreadOp::Rename { id, title } => rename(this, database, id, title, true, cx).await,
        ThreadOp::Ask {
            id,
            title,
            options,
            fields,
        } => interaction::ask(this, id, title, options, fields, cx).await,
        ThreadOp::MarkRead { id } => {
            let at = time(this, cx)?;
            database.call(move |store| store.mark_read(&id, at)).await?;
            Workspace::refresh(this, cx).await?;
            Ok(OpResult::Done)
        }
        ThreadOp::SetMetadata {
            id,
            plugin_id,
            value,
        } => {
            thread(this, &id, cx)?;
            database
                .call(move |store| store.set_thread_metadata(&id, &plugin_id, &value))
                .await?;
            this.update(cx, |_, cx| {
                cx.emit(crate::WorkspaceEvent::Changed);
                cx.notify();
            })
            .map_err(closed)?;
            Ok(OpResult::Done)
        }
        ThreadOp::StartAgain { id } => terminal::start_again(this, id, cx).await,
        op @ (ThreadOp::Get { .. }
        | ThreadOp::List { .. }
        | ThreadOp::Metadata { .. }
        | ThreadOp::Events { .. }
        | ThreadOp::Tools { .. }
        | ThreadOp::CallTool { .. }
        | ThreadOp::Interactions { .. }
        | ThreadOp::Wait { .. }
        | ThreadOp::WaitEvent { .. }
        | ThreadOp::Search { .. }) => read_thread(this, database, op, cx).await,
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
