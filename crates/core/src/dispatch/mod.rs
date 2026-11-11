//! The global `Op` router: one match over every operation, and the queue policy around it.

mod diagnostics;
mod guide;
pub(crate) mod ui;

use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{
    EnvOp, EnvironmentOp, KeymapOp, NotifyOp, Op, OpError, OpResult, PreferenceOp, ProgramOp,
    ProjectOp, SettingsOp, Sidebar, TerminalOp, ThemeOp, ThreadOp, order_sidebar_projects,
};

use crate::host::database::{Database, Result};
use crate::project::section::section;
use crate::project::{add_project, remove as remove_project};
use crate::settings::{keymap, preference, theme};
use crate::thread::dispatch_thread;
use crate::workspace::{closed, missing};
use crate::{PluginRequest, Requester, Workspace};

fn read_sidebar(store: &mut kg_store::Store) -> kg_store::Result<OpResult> {
    Ok(OpResult::Sidebar(Sidebar::new(
        store.projects()?,
        store.sections()?,
        store.all_threads()?,
    )))
}

#[expect(
    clippy::cognitive_complexity,
    reason = "one arm per Op variant; the router is a flat table"
)]
pub(crate) async fn execute(
    this: &WeakEntity<Workspace>,
    op: Op,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let database = crate::workspace::database(this, cx)?;
    match op {
        Op::Guide { chapter } => Ok(crate::dispatch::guide::read(chapter)),
        Op::Skill(op) => crate::skill::run(this, op, cx).await,
        Op::Status {
            thread_id,
            project_id,
            environment_id,
        } => crate::dispatch::guide::status(this, thread_id, project_id, environment_id, cx).await,
        Op::Open => announce(this, crate::WorkspaceEvent::Open, cx),
        Op::Settings(SettingsOp::Open) => announce(this, crate::WorkspaceEvent::Settings, cx),
        Op::Notify(NotifyOp::Test) => announce(this, crate::WorkspaceEvent::NotifyTest, cx),
        Op::Settings(SettingsOp::Plugins) => {
            announce(this, crate::WorkspaceEvent::SettingsPlugins, cx)
        }
        Op::Settings(SettingsOp::Project { id }) => {
            let lookup = id.clone();
            database
                .call(move |store| store.project(&lookup))
                .await?
                .ok_or_else(|| missing("project", &id))?;
            announce(this, crate::WorkspaceEvent::ProjectSettings(id), cx)
        }
        Op::Window(op) => announce(this, crate::WorkspaceEvent::Window(op), cx),
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
        Op::Environment(EnvironmentOp::Branches { project_id }) => {
            crate::host::git::branches(&database, project_id, cx).await
        }
        Op::Environment(EnvironmentOp::Setup { project_id, rev }) => {
            crate::host::git::setup(&database, project_id, rev, cx).await
        }
        Op::Program(ProgramOp::List { project_id }) => {
            crate::host::program::list(this, &database, true, project_id, cx).await
        }
        Op::Program(ProgramOp::Discover { project_id }) => {
            crate::host::program::list(this, &database, false, project_id, cx).await
        }
        Op::Env(op) => crate::host::variables::run(this, op, cx).await,
        Op::Environment(EnvironmentOp::SetBase { id, base }) => {
            crate::host::git::set_base(this, &database, id, base, cx).await
        }
        Op::Environment(EnvironmentOp::Remove { id }) => {
            crate::environment::remove_again(this, id, cx).await
        }
        Op::Theme(op) => theme(this, database, op, cx).await,
        Op::Keymap(op) => keymap(this, database, op, cx).await,
        Op::Preference(op) => preference(this, database, op, cx).await,
        Op::Instructions(op) => crate::instructions::run(this, op, cx).await,
        Op::Ui(op) => crate::dispatch::ui::drive(this, crate::UiJob::Op(op), cx).await,
        Op::Browser(op) => crate::dispatch::ui::drive(this, crate::UiJob::Browser(op), cx).await,
        Op::Surface(op) => crate::dispatch::ui::drive(this, crate::UiJob::Surface(op), cx).await,
        Op::Diff(op) => {
            let cache = this
                .update(cx, |workspace, _| workspace.diffs.clone())
                .map_err(closed)?;
            crate::diff::run(database, cache, op, cx).await
        }
        Op::Preview(op) => crate::preview::run(database, op, cx).await,
        Op::Diagnostics(op) => crate::dispatch::diagnostics::export(this, op, cx).await,
        Op::Update(op) => crate::update::run(&database, op, cx).await,
        Op::Thread(op) => dispatch_thread(this, database, op, requester, cx).await,
        Op::Section(op) => section(this, database, op, cx).await,
        Op::Plugin(kg_domain::PluginOp::Start) => {
            announce(this, crate::WorkspaceEvent::NewPlugin, cx)
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
                .call(|store| {
                    let mut projects = store.projects()?;
                    order_sidebar_projects(&mut projects);
                    Ok(OpResult::Projects(projects))
                })
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
        ProjectOp::Remove { id } => remove_project(this, id, requester, cx).await,
        ProjectOp::SetProgram { id, program } => {
            let lookup = id.clone();
            let project = database
                .call(move |store| {
                    store.set_project_program(&lookup, program.as_deref())?;
                    store.project(&lookup)
                })
                .await?
                .ok_or_else(|| missing("project", &id))?;
            Workspace::refresh(this, cx).await?;
            Ok(OpResult::Project(project))
        }
        ProjectOp::Reorder { id, before } => {
            let result = database
                .call(move |store| crate::project::reorder(store, &id, before.as_ref()))
                .await?;
            Workspace::refresh(this, cx).await?;
            Ok(result)
        }
    }
}

fn announce(
    this: &WeakEntity<Workspace>,
    event: crate::WorkspaceEvent,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    this.update(cx, |_, cx| cx.emit(event)).map_err(closed)?;
    Ok(OpResult::Done)
}

/// Whether `op` runs on the one-at-a-time request queue.
///
/// A serialized handler must finish without waiting for a person or for another request:
/// `Workspace::handle` awaits it before the loop reads the next envelope. An op that asks
/// (`ProjectOp::Remove`, `ThreadOp::Delete`, and the plugin ops that approve powers or a
/// removal) is detached, and a detached handler submits its request only after the person
/// answers. `UiOp::Snapshot` is serialized because a window job never submits a request.
pub(crate) fn serialized(op: &Op) -> bool {
    match op {
        Op::Env(EnvOp::List { .. })
        | Op::Status { .. }
        | Op::Guide { .. }
        | Op::Skill(_)
        | Op::Open
        | Op::Settings(_)
        | Op::Notify(_)
        | Op::Window(_)
        | Op::Sidebar
        | Op::Project(ProjectOp::List | ProjectOp::Get { .. } | ProjectOp::Remove { .. })
        | Op::Program(ProgramOp::List { .. } | ProgramOp::Discover { .. })
        | Op::Environment(
            EnvironmentOp::Get { .. }
            | EnvironmentOp::Branches { .. }
            | EnvironmentOp::Setup { .. },
        )
        | Op::Theme(ThemeOp::Get | ThemeOp::New { .. } | ThemeOp::Folder)
        | Op::Keymap(KeymapOp::Get)
        | Op::Preference(PreferenceOp::Get { .. })
        | Op::Instructions(kg_domain::InstructionsOp::Get { .. })
        | Op::Plugin(_)
        | Op::Diff(_)
        | Op::Preview(_)
        | Op::Diagnostics(_)
        | Op::Browser(_)
        | Op::Update(_) => false,
        Op::Project(
            ProjectOp::Add { .. } | ProjectOp::Reorder { .. } | ProjectOp::SetProgram { .. },
        )
        | Op::Env(EnvOp::Set { .. } | EnvOp::Remove { .. })
        | Op::Environment(
            EnvironmentOp::Restore { .. }
            | EnvironmentOp::SetBase { .. }
            | EnvironmentOp::Remove { .. },
        )
        | Op::Theme(ThemeOp::Set { .. })
        | Op::Keymap(KeymapOp::Set { .. } | KeymapOp::Remove { .. })
        | Op::Preference(PreferenceOp::Set { .. })
        | Op::Instructions(
            kg_domain::InstructionsOp::Set { .. } | kg_domain::InstructionsOp::Clear { .. },
        )
        | Op::Section(_)
        | Op::Ui(_) => true,
        Op::Surface(op) => !matches!(op, kg_domain::SurfaceOp::List),
        Op::Thread(op) => match op {
            ThreadOp::Stop { .. }
            | ThreadOp::Archive { .. }
            | ThreadOp::Unarchive { .. }
            | ThreadOp::SetPinned { .. }
            | ThreadOp::Notify { .. }
            | ThreadOp::Rename { .. }
            | ThreadOp::Ask { .. }
            | ThreadOp::MarkRead { .. }
            | ThreadOp::SetMetadata { .. }
            | ThreadOp::StartAgain { .. }
            | ThreadOp::Turn(_)
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
        (None, PluginRequest::Commands) => Ok(OpResult::PluginCommands(Vec::new())),
        (None, PluginRequest::Tools { .. }) => Ok(OpResult::Tools {
            instructions: None,
            tools: Vec::new(),
        }),
        (None, PluginRequest::CallTool { name, .. }) => Err(OpError::NotFound(format!(
            "agent tool {name} is unavailable; no plugin host is loaded"
        ))),
        (None, PluginRequest::Instruct { .. }) => Ok(OpResult::Output(Some(String::new()))),
        (None, PluginRequest::Logs) => Ok(OpResult::PluginLogs(Vec::new())),
        (None, PluginRequest::Keymap(KeymapOp::Get)) => {
            Ok(OpResult::Setting(serde_json::json!({})))
        }
        (None, PluginRequest::Plugin(..) | PluginRequest::Keymap(_)) => {
            Err(OpError::Invalid("plugin host is not available".into()))
        }
    }
}
