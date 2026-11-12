use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use async_channel::Sender;
use gpui_kit::{
    App, AppContext, AsyncApp, Context, Entity, EventEmitter, Global, Subscription, Task,
    WeakEntity,
};
use kg_domain::{
    EnvironmentId, Event, KeymapOp, Op, OpError, OpResult, PendingInteractionId, PluginOp,
    ProjectId, Run, ThreadId,
};
use serde_json::Value;

use crate::host::database::{Database, Result};
use crate::{Environment, Project, Requester, Thread};

pub(crate) mod approval;
mod clock;
mod data;
pub(crate) mod entity;
mod lifecycle;
mod plugins;
mod questions;
pub(crate) mod recovery;
mod screens;
mod state;

pub(crate) use clock::{changed, closed, database, id, missing, now, time};
pub(crate) use data::open_data;
pub use plugins::Permit;
pub(crate) use questions::{Waiting, approve};

use recovery::interrupted;

/// A step that waits in tests until a value is sent; outside tests `hold` returns at once.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Hold {
    Removal,
    Run,
    Delete,
    Screen,
    Start,
}

#[cfg(any(test, feature = "test"))]
pub(crate) async fn hold(this: &WeakEntity<Workspace>, which: Hold, cx: &mut AsyncApp) {
    if let Ok(Some(held)) = this.update(cx, |workspace, _| workspace.holds.get(&which).cloned()) {
        // A dropped sender also releases the hold.
        let _ = held.recv().await;
    }
}

#[cfg(not(any(test, feature = "test")))]
pub(crate) fn hold(_: &WeakEntity<Workspace>, _: Hold, _: &mut AsyncApp) -> std::future::Ready<()> {
    std::future::ready(())
}

const QUIT_DEADLINE: Duration = Duration::from_secs(3);
pub(crate) enum Request {
    Op(Op),
    RemoveProject {
        id: ProjectId,
        threads: Vec<ThreadId>,
    },
    Run(Box<crate::thread::PreparedRun>),
    Answer {
        thread_id: ThreadId,
        interaction_id: PendingInteractionId,
        option_id: String,
        fields: std::collections::BTreeMap<String, serde_json::Value>,
    },
    AnswerStandalone {
        interaction_id: PendingInteractionId,
        option_id: String,
    },
    Sweep,
    /// The first sweep after open: finish removals the previous process left.
    Leftovers,
    /// Folder I/O finished; apply the outcome only if the env is still `destroyed`.
    RemovalDone {
        environment: kg_domain::Environment,
        outcome: Result<()>,
    },
    /// The program in a thread's terminal ended; record how.
    Exited {
        thread_id: ThreadId,
        exit: kg_terminal::Exit,
    },
    /// A locked title stays, and the rename fails.
    RenameOnce {
        id: ThreadId,
        title: String,
    },
}
type Envelope = (Request, Sender<Result<OpResult>>);

/// The operations core hands to the plugin host.
pub enum PluginRequest {
    Warnings,
    /// The `kg` commands of every running plugin, for the generated plugin-commands skill.
    Commands,
    Tools {
        thread_id: ThreadId,
    },
    CallTool {
        thread_id: ThreadId,
        name: String,
        input: Value,
    },
    Instruct {
        thread_id: ThreadId,
        project_id: ProjectId,
    },
    /// The log counts of each installed plugin, for diagnostics.
    Logs,
    Plugin(PluginOp, Option<Requester>),
    /// A shortcut of a plugin's command, named `<plugin-id>:<command>`
    Keymap(KeymapOp),
}

/// Lets the plugin host serve its operations without core depending on it.
pub type PluginDispatch = Rc<dyn Fn(PluginRequest, &mut App) -> Task<Result<OpResult>>>;

pub struct Workspace {
    pub(crate) pending: BTreeMap<PendingInteractionId, Waiting>,
    pub(crate) database: Database,
    pub(crate) data_dir: PathBuf,
    pub(crate) cli: PathBuf,
    /// Where this app listens for `kg`; every process it starts gets it as `KG_SOCKET`.
    pub(crate) socket: Option<std::ffi::OsString>,
    pub(crate) projects: BTreeMap<ProjectId, Entity<Project>>,
    pub(crate) preparing_projects: BTreeMap<ProjectId, usize>,
    pub(crate) removing_projects: BTreeSet<ProjectId>,
    pub(crate) environments: BTreeMap<EnvironmentId, Entity<Environment>>,
    pub(crate) threads: BTreeMap<ThreadId, Entity<Thread>>,
    pub(crate) tasks: BTreeMap<ThreadId, Task<()>>,
    pub(crate) runs: BTreeMap<ThreadId, crate::thread::Run>,
    /// Whether the task that watches `terminals` is running.
    pub(crate) watching: bool,
    /// Environments whose folder removal is running outside the operation loop.
    pub(crate) removing: BTreeSet<EnvironmentId>,
    pub(crate) diffs: std::sync::Arc<crate::diff::Cache>,
    pub(crate) usage: crate::usage::Cache,
    keymap: kg_domain::Keymap,
    pub(crate) plugins: Option<PluginDispatch>,
    /// Where this workspace finds programs, when a test has set it; None is the login PATH
    #[cfg(any(test, feature = "test"))]
    pub(crate) program_environment: Option<crate::host::program::ProgramEnvironment>,
    /// Where a test says a program keeps its files, by the variable that names the folder.
    #[cfg(any(test, feature = "test"))]
    pub(crate) homes: BTreeMap<&'static str, PathBuf>,
    pub(crate) ui: Option<crate::dispatch::ui::UiDispatch>,
    sections: Vec<kg_domain::Section>,
    requests: Sender<Envelope>,
    // Wall time at open plus executor time, so grace deadlines follow GPUI's test clock.
    clock: (kg_domain::Timestamp, Instant),
    #[cfg(any(test, feature = "test"))]
    pub(crate) holds: BTreeMap<Hold, async_channel::Receiver<()>>,
    _database_task: Task<()>,
    _requests_task: Task<()>,
    _retire_task: Task<()>,
    _automation_task: Task<()>,
    _quit: Subscription,
}
#[derive(Clone, Debug)]
pub enum WorkspaceEvent {
    Open,
    Skills,
    Settings,
    ProjectSettings(kg_domain::ProjectId, Option<kg_domain::ProjectSection>),
    SettingsPlugins,
    NewPlugin,
    Window(kg_domain::WindowOp),
    Event {
        event: Event,
        project_id: ProjectId,
    },
    Changed,
    /// A terminal's screen changed: the program's, or an extra one by number.
    Screen(ThreadId, Option<u32>),
    Waiting(ThreadId),
    Finished(ThreadId),
    /// A thread may need the person; the window decides whether to post a system notification
    Notice(ThreadId, kg_domain::NotifyReason),
    /// `kg notify test`: post one test system notification
    NotifyTest,
    /// An automation paused itself: its name, and why
    AutomationPaused(String, String),
}
impl EventEmitter<WorkspaceEvent> for Workspace {}

pub(crate) struct WorkspaceGlobal(Entity<Workspace>);
impl Global for WorkspaceGlobal {}
impl Workspace {
    /// Built-in shortcut overrides last read or written by a keymap operation.
    #[must_use]
    pub fn keymap(&self) -> &kg_domain::Keymap {
        &self.keymap
    }
    pub(crate) fn set_keymap(&mut self, keymap: kg_domain::Keymap) {
        self.keymap = keymap;
    }
    /// Opens the database and installs the workspace global. I/O runs in the background.
    ///
    /// # Errors
    /// Returns an error if the data folder or database cannot be opened.
    pub fn open(
        data_dir: PathBuf,
        cli: PathBuf,
        socket: Option<std::ffi::OsString>,
        cx: &mut App,
    ) -> Task<Result<Entity<Self>>> {
        Self::open_prepared(data_dir, cli, socket, |_| {}, cx)
    }
    // Like `open`, but `prepare` runs before the restart's resume, so a test sets up what resume reads.
    #[cfg(test)]
    pub(crate) fn open_with(
        data_dir: PathBuf,
        cli: PathBuf,
        socket: Option<std::ffi::OsString>,
        prepare: impl FnOnce(&mut Self) + 'static,
        cx: &mut App,
    ) -> Task<Result<Entity<Self>>> {
        Self::open_prepared(data_dir, cli, socket, prepare, cx)
    }
    fn open_prepared(
        data_dir: PathBuf,
        cli: PathBuf,
        socket: Option<std::ffi::OsString>,
        prepare: impl FnOnce(&mut Self) + 'static,
        cx: &mut App,
    ) -> Task<Result<Entity<Self>>> {
        let executor = cx.background_executor().clone();
        let opened = executor.spawn(async move { open_data(&data_dir) });
        cx.spawn(async move |cx| {
            let (store, data_dir) = opened.await?;
            let (database, database_task) = Database::new(store, &executor);
            database.call(interrupted).await?;
            let updates = database.clone();
            let workspace = cx.update(|cx| {
                crate::update::Updater::install(updates, cx);
                let (requests, incoming) = async_channel::unbounded::<Envelope>();
                let workspace = cx.new(|cx: &mut Context<Self>| {
                    let requests_task = cx.spawn(async move |this, cx| {
                        while let Ok((request, reply)) = incoming.recv().await {
                            let result = Self::handle(&this, request, cx).await;
                            // The caller dropped its task, so nobody reads the reply.
                            let _ = reply.send(result).await;
                        }
                    });
                    let retire_task = cx.spawn(async move |this, cx| {
                        loop {
                            cx.background_executor().timer(Duration::from_secs(1)).await;
                            let Some(workspace) = this.upgrade() else {
                                break;
                            };
                            let task = workspace.update(cx, |workspace, _| {
                                let (reply, receive) = async_channel::bounded(1);
                                // The next sweep waits for this reply, so sweeps never pile up.
                                // A closed queue means the workspace is going.
                                let _ = workspace.requests.try_send((Request::Sweep, reply));
                                receive
                            });
                            if task.recv().await.is_err() {
                                break;
                            }
                        }
                    });
                    Self {
                        pending: BTreeMap::new(),
                        database,
                        data_dir,
                        cli,
                        socket,
                        sections: Vec::new(),
                        projects: BTreeMap::new(),
                        preparing_projects: BTreeMap::new(),
                        removing_projects: BTreeSet::new(),
                        environments: BTreeMap::new(),
                        threads: BTreeMap::new(),
                        tasks: BTreeMap::new(),
                        runs: BTreeMap::new(),
                        watching: false,
                        removing: BTreeSet::new(),
                        diffs: crate::diff::Cache::new(),
                        usage: crate::usage::Cache::new(),
                        keymap: kg_domain::Keymap::default(),
                        plugins: None,
                        #[cfg(any(test, feature = "test"))]
                        program_environment: None,
                        #[cfg(any(test, feature = "test"))]
                        homes: BTreeMap::new(),
                        ui: None,
                        requests,
                        clock: (now(), cx.background_executor().now()),
                        #[cfg(any(test, feature = "test"))]
                        holds: BTreeMap::new(),
                        _database_task: database_task,
                        _requests_task: requests_task,
                        _retire_task: retire_task,
                        _automation_task: crate::automation::watch(cx),
                        _quit: cx.on_app_quit(|workspace, _| {
                            workspace.end_programs();
                            async {}
                        }),
                    }
                });
                workspace.update(cx, |workspace, _| prepare(workspace));
                cx.set_global(WorkspaceGlobal(workspace.clone()));
                workspace
            });
            Self::resume(&workspace.downgrade(), cx).await?;
            Ok(workspace)
        })
    }
    async fn handle(
        this: &WeakEntity<Self>,
        request: Request,
        cx: &mut AsyncApp,
    ) -> Result<OpResult> {
        match request {
            Request::Op(op) => crate::dispatch::execute(this, op, None, cx).await,
            Request::RemoveProject { id, threads } => {
                this.update(cx, |workspace, _| {
                    if workspace.preparing_projects.contains_key(&id) {
                        return Err(OpError::Invalid(
                            "Stop the programs in this project first.".into(),
                        ));
                    }
                    workspace.removing_projects.insert(id.clone());
                    Ok(())
                })
                .map_err(closed)??;
                let result = crate::project::remove_project(this, id.clone(), threads, cx).await;
                this.update(cx, |workspace, _| {
                    workspace.removing_projects.remove(&id);
                })
                .map_err(closed)?;
                result
            }
            Request::Run(prepared) => crate::thread::commit_run(this, *prepared, cx).await,
            Request::Answer {
                thread_id,
                interaction_id,
                option_id,
                fields,
            } => {
                crate::thread::answer(this, thread_id, interaction_id, option_id, fields, cx).await
            }
            Request::AnswerStandalone {
                interaction_id,
                option_id,
            } => this
                .update(cx, |workspace, cx| {
                    workspace.answer_approval(&interaction_id, option_id, cx)
                })
                .map_err(closed)?,
            Request::Sweep => Self::sweep(this, cx).await,
            Request::Leftovers => Self::leftovers(this, cx).await,
            Request::RemovalDone {
                environment,
                outcome,
            } => crate::environment::finish_removal(this, environment, outcome, cx).await,
            Request::Exited { thread_id, exit } => {
                crate::thread::exited(this, thread_id, exit, cx).await
            }
            Request::RenameOnce { id, title } => {
                crate::thread::rename_once(this, id, title, cx).await
            }
        }
    }
    // Picks up what the previous process left: interrupted removals, then programs it left running.
    async fn resume(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> Result<()> {
        Self::refresh(this, cx).await?;
        crate::environment::drop_expired_screens(this, cx).await?;
        crate::settings::remember_keymap(this, cx).await?;
        let submitted = this
            .update(cx, |workspace, _| workspace.submit(Request::Leftovers))
            .map_err(closed)?;
        submitted.await?;
        recovery::resume_programs(this, cx).await
    }
    #[must_use]
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<WorkspaceGlobal>().0.clone()
    }
    /// Finds this workspace's programs on `path` and starts them with `shell`, instead of the login
    /// PATH and the shell. A test calls this so its program list does not depend on the machine.
    #[cfg(any(test, feature = "test"))]
    pub fn use_program_environment(&mut self, path: &std::path::Path, shell: &std::path::Path) {
        self.program_environment = Some(crate::host::program::ProgramEnvironment {
            path: path.as_os_str().to_owned(),
            shell: shell.as_os_str().to_owned(),
        });
    }
    #[cfg(any(test, feature = "test"))]
    pub fn use_program_home(&mut self, home: &std::path::Path) {
        self.homes
            .insert(crate::host::program::home_variable(), home.to_owned());
    }
    pub fn rename_once(id: ThreadId, title: String, cx: &mut App) -> Task<Result<OpResult>> {
        let op = Op::Thread(kg_domain::ThreadOp::Rename {
            id: id.clone(),
            title: title.clone(),
        });
        if let Err(message) = kg_domain::check(&op) {
            return Task::ready(Err(OpError::Invalid(message)));
        }
        let Some(global) = cx.try_global::<WorkspaceGlobal>() else {
            return Task::ready(Err(OpError::Failed("workspace is not open".into())));
        };
        let submitted = global.0.read(cx).submit(Request::RenameOnce { id, title });
        cx.spawn(async move |_| submitted.await)
    }
    pub fn dispatch(op: Op, cx: &mut App) -> Task<Result<OpResult>> {
        Self::dispatch_for(op, None, cx)
    }
    /// Runs `op` for a socket client. A person-only question it raises goes when the client does.
    pub fn dispatch_for(
        op: Op,
        requester: Option<Requester>,
        cx: &mut App,
    ) -> Task<Result<OpResult>> {
        if let Err(message) = kg_domain::check(&op) {
            return Task::ready(Err(OpError::Invalid(message)));
        }
        let Some(global) = cx.try_global::<WorkspaceGlobal>() else {
            return Task::ready(Err(OpError::Failed("workspace is not open".into())));
        };
        let workspace = global.0.clone();
        let this = workspace.downgrade();
        match op {
            Op::Preference(kg_domain::PreferenceOp::Set { key, .. })
                if requester.is_some()
                    && kg_domain::Slot::ALL
                        .iter()
                        .any(|slot| slot.preference() == key.as_str()) =>
            {
                Task::ready(Err(OpError::Invalid(format!(
                    "{key} is set in Settings, or with kg plugin use, which asks a person"
                ))))
            }
            Op::Thread(kg_domain::ThreadOp::Rename { id, title }) if requester.is_some() => {
                Self::rename_once(id, title, cx)
            }
            Op::Preview(preview @ kg_domain::PreviewOp::Show { .. }) if requester.is_some() => {
                shown_preview(&this, preview, requester, cx)
            }
            Op::Thread(kg_domain::ThreadOp::WaitEvent {
                id,
                kind,
                after_seq,
                timeout_ms,
            }) => crate::thread::wait_event(&workspace, id, &kind, after_seq, timeout_ms, cx),
            Op::Thread(kg_domain::ThreadOp::Wait { id, timeout_ms }) => {
                crate::thread::wait(&workspace, id, timeout_ms, cx)
            }
            Op::Thread(kg_domain::ThreadOp::Search {
                text,
                limit,
                archived,
            }) => crate::thread::search(&workspace, &text, limit, archived, cx),
            Op::Thread(kg_domain::ThreadOp::Run(run)) => {
                match workspace.update(cx, |workspace, cx| workspace.start_run(run, cx)) {
                    Ok(started) => cx.spawn(async move |_| started.await.1),
                    Err(error) => Task::ready(Err(error)),
                }
            }
            op if crate::dispatch::serialized(&op) => {
                let submitted = workspace.read(cx).submit(Request::Op(op));
                cx.spawn(async move |_| submitted.await)
            }
            op => detached(cx, async move |cx| {
                crate::dispatch::execute(&this, op, requester, cx).await
            }),
        }
    }
    pub(crate) fn submit(
        &self,
        request: Request,
    ) -> impl std::future::Future<Output = Result<OpResult>> + use<> {
        let (reply, receive) = async_channel::bounded(1);
        let sent = self.requests.try_send((request, reply));
        async move {
            sent.map_err(|_| closed("workspace closed"))?;
            receive
                .recv()
                .await
                .map_err(|_| closed("workspace closed"))?
        }
    }
}
// A socket preview also asks the window to show its tab, once the read has succeeded.
fn shown_preview(
    this: &WeakEntity<Workspace>,
    preview: kg_domain::PreviewOp,
    requester: Option<Requester>,
    cx: &mut App,
) -> Task<Result<OpResult>> {
    let this = this.clone();
    detached(cx, async move |cx| {
        let kg_domain::PreviewOp::Show { thread_id, path } = preview.clone();
        let result = crate::dispatch::execute(&this, Op::Preview(preview), requester, cx).await?;
        // Without a window there is no tab to show; the read itself still succeeded.
        let _ = crate::dispatch::ui::drive(
            &this,
            crate::UiJob::Preview {
                thread: thread_id,
                path,
            },
            cx,
        )
        .await;
        Ok(result)
    })
}

/// A run's result, and the thread it made. The thread is known once the run has been prepared,
/// so a run that then fails still names the thread it left behind.
pub(crate) type RunOutcome = (Option<kg_domain::ThreadId>, Result<OpResult>);

impl Workspace {
    /// Starts a run in its project. The project counts as preparing until the run is committed
    /// or fails, so removing the project waits for it. The run finishes even if the caller stops
    /// waiting.
    pub(crate) fn start_run(
        &mut self,
        run: Run,
        cx: &mut Context<Self>,
    ) -> Result<Task<RunOutcome>> {
        let project = run.project_id.clone();
        if self.removing_projects.contains(&project) {
            return Err(OpError::Invalid("this project is being removed".into()));
        }
        *self.preparing_projects.entry(project.clone()).or_default() += 1;
        let (reply, receive) = async_channel::bounded(1);
        cx.spawn(async move |this, cx| {
            let outcome = crate::thread::start_run(&this, run, cx).await;
            let _ = this.update(cx, |workspace, _| workspace.finish_preparing(&project));
            let _ = reply.send(outcome).await;
        })
        .detach();
        Ok(cx.spawn(async move |_, _| {
            receive
                .recv()
                .await
                .unwrap_or((None, Err(closed("workspace closed"))))
        }))
    }

    fn finish_preparing(&mut self, project: &ProjectId) {
        let Some(count) = self.preparing_projects.get_mut(project) else {
            return;
        };
        *count = count.saturating_sub(1);
        if *count == 0 {
            self.preparing_projects.remove(project);
        }
    }
}

// Runs work that the caller's task cannot cancel by being dropped.
fn detached(
    cx: &mut App,
    work: impl AsyncFnOnce(&mut AsyncApp) -> Result<OpResult> + 'static,
) -> Task<Result<OpResult>> {
    let (reply, receive) = async_channel::bounded(1);
    cx.spawn(async move |cx| {
        // The caller stopped waiting, but the work itself was meant to finish.
        let _ = reply.send(work(cx).await).await;
    })
    .detach();
    cx.spawn(async move |_| {
        receive
            .recv()
            .await
            .map_err(|_| closed("workspace closed"))?
    })
}
