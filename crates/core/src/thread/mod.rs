mod agent;
mod archive;
mod conversation;
mod details;
mod extra;
mod halt;
mod interaction;
mod project;
mod read;
mod run;
mod search;
mod section;
mod settings;
mod terminal;

pub(crate) use agent::{note, note_with};
pub(crate) use extra::{Extras, end as end_extras};
pub(crate) use halt::{retire, stop};
pub(crate) use interaction::{answer, insert_reply, raise, withdraw};
pub(crate) use project::remove_project;
pub(crate) use read::{wait, wait_event};
pub(crate) use run::resolve_program;
pub(crate) use run::{PreparedRun, commit as commit_run, prepare as prepare_run};
pub(crate) use search::search;
pub(crate) use settings::remember_keymap;
pub(crate) use terminal::StopReason;
pub(crate) use terminal::exited;
#[cfg(test)]
pub(crate) use terminal::remember;
pub(crate) use terminal::start as start_terminal;
pub(crate) use terminal::{Shown, release_ended};

use gpui_kit::{AsyncApp, Entity, WeakEntity};
use kg_domain::{
    EnvironmentOp, KeymapOp, Op, OpError, OpResult, PreferenceOp, ProgramOp, ProjectOp, SettingsOp,
    Sidebar, TerminalOp, ThemeOp, ThreadId, ThreadOp,
};

use crate::database::{Database, Result};
use crate::workspace::{closed, missing, time};
use crate::{PluginRequest, Requester, Thread, Workspace};

use archive::{archive, delete, unarchive};
use project::add_project;
use read::read_thread;
use section::section;
use settings::{keymap, preference, theme};

fn read_sidebar(store: &mut kg_store::Store) -> kg_store::Result<OpResult> {
    let mut projects = store.projects()?;
    kg_domain::order_sidebar_projects(&mut projects);
    Ok(OpResult::Sidebar(Sidebar {
        projects,
        sections: store.sections()?,
        threads: store
            .all_threads()?
            .into_iter()
            .filter(|thread| {
                thread.visibility == kg_domain::Visibility::Visible && thread.archived_at.is_none()
            })
            .collect(),
    }))
}

pub(crate) async fn execute(
    this: &WeakEntity<Workspace>,
    op: Op,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let database = crate::workspace::database(this, cx)?;
    match op {
        Op::Guide { chapter } => Ok(crate::guide::read(chapter)),
        Op::Skill(op) => crate::skill::run(this, op, cx).await,
        Op::Status {
            thread_id,
            project_id,
            environment_id,
        } => crate::guide::status(this, thread_id, project_id, environment_id, cx).await,
        Op::Open => {
            this.update(cx, |_, cx| cx.emit(crate::WorkspaceEvent::Open))
                .map_err(closed)?;
            Ok(OpResult::Done)
        }
        Op::Settings(SettingsOp::Open) => {
            this.update(cx, |_, cx| cx.emit(crate::WorkspaceEvent::Settings))
                .map_err(closed)?;
            Ok(OpResult::Done)
        }
        Op::Settings(SettingsOp::Plugins) => {
            this.update(cx, |_, cx| cx.emit(crate::WorkspaceEvent::SettingsPlugins))
                .map_err(closed)?;
            Ok(OpResult::Done)
        }
        Op::Window(op) => {
            this.update(cx, |_, cx| cx.emit(crate::WorkspaceEvent::Window(op)))
                .map_err(closed)?;
            Ok(OpResult::Done)
        }
        Op::Sidebar => database.call(read_sidebar).await,
        Op::Project(op) => project_op(this, &database, op, requester, cx).await,
        Op::Environment(EnvironmentOp::Get { id }) => {
            let lookup = id.clone();
            database
                .call(move |store| store.environment(&lookup))
                .await?
                .map(OpResult::Environment)
                .ok_or_else(|| missing("environment", &id))
        }
        Op::Environment(EnvironmentOp::Restore { thread_id }) => {
            let result = crate::environment::restore(this, thread_id, cx).await?;
            Workspace::refresh(this, cx).await?;
            Ok(OpResult::Environment(result))
        }
        Op::Environment(EnvironmentOp::Branches { id }) => {
            crate::git::branches(&database, id, cx).await
        }
        Op::Program(ProgramOp::List) => crate::program::list(this, &database, true, cx).await,
        Op::Program(ProgramOp::Discover) => crate::program::list(this, &database, false, cx).await,
        Op::Environment(EnvironmentOp::SetBase { id, base }) => {
            crate::git::set_base(this, &database, id, base, cx).await
        }
        Op::Environment(EnvironmentOp::Remove { id }) => {
            crate::environment::remove_again(this, id, cx).await
        }
        Op::Theme(op) => theme(this, database, op, cx).await,
        Op::Keymap(op) => keymap(this, database, op, cx).await,
        Op::Preference(op) => preference(this, database, op, cx).await,
        Op::Ui(op) => crate::ui::run(this, op, cx).await,
        Op::Surface(op) => crate::ui::drive(this, crate::UiJob::Surface(op), cx).await,
        Op::Diff(op) => crate::diff::run(database, op, cx).await,
        Op::Diagnostics(op) => crate::diagnostics::export(this, op, cx).await,
        Op::Update(op) => crate::update::run(&database, op, cx).await,
        Op::Thread(op) => dispatch_thread(this, database, op, requester, cx).await,
        Op::Section(op) => section(this, database, op, cx).await,
        Op::Plugin(kg_domain::PluginOp::Start) => {
            this.update(cx, |_, cx| cx.emit(crate::WorkspaceEvent::NewPlugin))
                .map_err(closed)?;
            Ok(OpResult::Done)
        }
        Op::Plugin(op) => forward(this, PluginRequest::Plugin(op, requester), cx).await,
    }
}

async fn project_op(
    this: &WeakEntity<Workspace>,
    database: &Database,
    op: ProjectOp,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        ProjectOp::List => {
            database
                .call(|store| Ok(OpResult::Projects(store.projects()?)))
                .await
        }
        ProjectOp::Get { id } => {
            let lookup = id.clone();
            database
                .call(move |store| store.project(&lookup))
                .await?
                .map(OpResult::Project)
                .ok_or_else(|| missing("project", &id))
        }
        ProjectOp::Add { path } => {
            let project = database
                .call(move |store| Ok(add_project(store, &path)))
                .await??;
            Workspace::refresh(this, cx).await?;
            Ok(OpResult::Project(project))
        }
        ProjectOp::Remove { id } => project::remove(this, id, requester, cx).await,
        ProjectOp::Reorder { id, before } => {
            let result = database
                .call(move |store| project::reorder(store, &id, before.as_ref()))
                .await?;
            Workspace::refresh(this, cx).await?;
            Ok(result)
        }
    }
}

pub(crate) fn serialized(op: &Op) -> bool {
    match op {
        Op::Status { .. }
        | Op::Guide { .. }
        | Op::Skill(_)
        | Op::Open
        | Op::Settings(_)
        | Op::Window(_)
        | Op::Sidebar
        | Op::Project(ProjectOp::List | ProjectOp::Get { .. } | ProjectOp::Remove { .. })
        | Op::Program(ProgramOp::List | ProgramOp::Discover)
        | Op::Environment(EnvironmentOp::Get { .. } | EnvironmentOp::Branches { .. })
        | Op::Theme(ThemeOp::Get | ThemeOp::New { .. } | ThemeOp::Folder)
        | Op::Keymap(KeymapOp::Get)
        | Op::Preference(PreferenceOp::Get { .. })
        | Op::Plugin(_)
        | Op::Ui(kg_domain::UiOp::Snapshot { .. })
        | Op::Diff(_)
        | Op::Diagnostics(_)
        | Op::Update(_) => false,
        Op::Project(ProjectOp::Add { .. } | ProjectOp::Reorder { .. })
        | Op::Environment(
            EnvironmentOp::Restore { .. }
            | EnvironmentOp::SetBase { .. }
            | EnvironmentOp::Remove { .. },
        )
        | Op::Theme(ThemeOp::Set { .. })
        | Op::Keymap(KeymapOp::Set { .. } | KeymapOp::Remove { .. })
        | Op::Preference(PreferenceOp::Set { .. })
        | Op::Section(_) => true,
        Op::Ui(op) => !matches!(op, kg_domain::UiOp::Snapshot { .. }),
        Op::Surface(op) => !matches!(op, kg_domain::SurfaceOp::List),
        Op::Thread(op) => match op {
            ThreadOp::Stop { .. }
            | ThreadOp::Archive { .. }
            | ThreadOp::Unarchive { .. }
            | ThreadOp::SetPinned { .. }
            | ThreadOp::Rename { .. }
            | ThreadOp::Ask { .. }
            | ThreadOp::MarkRead { .. }
            | ThreadOp::SetMetadata { .. }
            | ThreadOp::StartAgain { .. }
            | ThreadOp::Terminal(TerminalOp::New { .. } | TerminalOp::Close { .. }) => true,
            ThreadOp::Run(_)
            | ThreadOp::Input { .. }
            | ThreadOp::Resize { .. }
            | ThreadOp::Screen { .. }
            | ThreadOp::Scroll { .. }
            | ThreadOp::Select { .. }
            | ThreadOp::Copy { .. }
            | ThreadOp::Paste { .. }
            | ThreadOp::Send { .. }
            | ThreadOp::Report { .. }
            | ThreadOp::Delete { .. }
            | ThreadOp::Get { .. }
            | ThreadOp::List { .. }
            | ThreadOp::Metadata { .. }
            | ThreadOp::Events { .. }
            | ThreadOp::Tools { .. }
            | ThreadOp::CallTool { .. }
            | ThreadOp::Interactions { .. }
            | ThreadOp::WaitEvent { .. }
            | ThreadOp::Wait { .. }
            | ThreadOp::Search { .. }
            | ThreadOp::Terminal(TerminalOp::List { .. })
            | ThreadOp::Details { .. }
            | ThreadOp::Links { .. } => false,
        },
    }
}

async fn dispatch_thread(
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
        ThreadOp::Delete { id, thread_id } => {
            delete(this, database, id, thread_id, requester, cx).await
        }
        ThreadOp::SetPinned { id, pinned } => {
            database
                .call(move |store| store.set_pinned(&id, pinned))
                .await?;
            Workspace::refresh(this, cx).await?;
            Ok(OpResult::Done)
        }
        ThreadOp::Rename { id, title } => rename(this, database, id, title, true, cx).await,
        ThreadOp::Ask { id, title, options } => {
            interaction::ask(this, id, title, options, cx).await
        }
        ThreadOp::MarkRead { id } => {
            let at = time(this, cx)?;
            database.call(move |store| store.mark_read(&id, at)).await?;
            Workspace::refresh(this, cx).await?;
            Ok(OpResult::Done)
        }
        ThreadOp::SetMetadata { id, metadata } => {
            thread(this, &id, cx)?;
            database
                .call(move |store| {
                    store.set_thread_metadata(&id, &metadata.plugin_id, &metadata.value)
                })
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

pub(crate) async fn forward(
    this: &WeakEntity<Workspace>,
    request: PluginRequest,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let host = this
        .update(cx, |workspace, _| workspace.plugins.clone())
        .map_err(closed)?;
    match (host, request) {
        (Some(host), request) => cx.update(|cx| host(request, cx)).await,
        (None, PluginRequest::Warnings) => Ok(OpResult::Warnings(Vec::new())),
        (None, PluginRequest::Tools { .. }) => Ok(OpResult::Tools {
            instructions: None,
            tools: Vec::new(),
        }),
        (None, PluginRequest::CallTool { name, .. }) => Err(OpError::NotFound(format!(
            "agent tool {name} is unavailable; no plugin host is loaded"
        ))),
        (None, PluginRequest::Instruct { .. }) => Ok(OpResult::Output(Some(String::new()))),
        (None, PluginRequest::Keymap(KeymapOp::Get)) => {
            Ok(OpResult::Setting(serde_json::json!({})))
        }
        (None, PluginRequest::Plugin(..) | PluginRequest::Keymap(_)) => {
            Err(OpError::Invalid("plugin host is not available".into()))
        }
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

pub(crate) fn child_notice(title: &str, error: Option<&str>, before_turn: bool) -> String {
    let because = error.map(|error| format!(": {error}")).unwrap_or_default();
    if before_turn {
        format!("Child thread \"{title}\" failed before its first turn{because}.")
    } else {
        format!("Child thread \"{title}\" failed{because}.")
    }
}
