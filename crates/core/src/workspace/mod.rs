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
    ProjectId, ThreadId,
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
mod recovery;
mod screens;
mod state;

pub(crate) use clock::{closed, database, id, missing, now, time};
pub(crate) use data::open_data;
pub use plugins::Permit;

use recovery::interrupted;

pub(crate) const GRACE: i64 = 10_000;
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
    Plugin(PluginOp, Option<Requester>),
    /// A shortcut of a plugin's command, named `<plugin-id>:<command>`
    Keymap(KeymapOp),
}

/// Lets the plugin host serve its operations without core depending on it.
pub type PluginDispatch = Rc<dyn Fn(PluginRequest, &mut App) -> Task<Result<OpResult>>>;

pub struct Workspace {
    pub(crate) approvals: BTreeMap<PendingInteractionId, crate::workspace::approval::Waiting>,
    pub(crate) replies: BTreeMap<PendingInteractionId, Sender<String>>,
    pub(crate) for_a_person: BTreeSet<PendingInteractionId>,
    pub(crate) asks: BTreeMap<ThreadId, (PendingInteractionId, async_channel::Receiver<String>)>,
    /// Which thread owns each reply. Taking one thread drops only its waits.
    pub(crate) interaction_thread: BTreeMap<PendingInteractionId, ThreadId>,
    pub(crate) database: Database,
    pub(crate) data_dir: PathBuf,
    pub(crate) cli: PathBuf,
    pub(crate) projects: BTreeMap<ProjectId, Entity<Project>>,
    pub(crate) preparing_projects: BTreeMap<ProjectId, usize>,
    removing_projects: BTreeSet<ProjectId>,
    pub(crate) environments: BTreeMap<EnvironmentId, Entity<Environment>>,
    pub(crate) threads: BTreeMap<ThreadId, Entity<Thread>>,
    pub(crate) tasks: BTreeMap<ThreadId, Task<()>>,
    pub(crate) terminals: BTreeMap<ThreadId, crate::thread::Shown>,
    pub(crate) extras: BTreeMap<ThreadId, crate::thread::Extras>,
    /// The size the window last gave each thread's program. A program started again takes it.
    pub(crate) sizes: BTreeMap<ThreadId, (u16, u16)>,
    /// Whether the task that watches `terminals` is running.
    pub(crate) watching: bool,
    /// Environments whose folder removal is running outside the operation loop.
    pub(crate) removing: BTreeSet<EnvironmentId>,
    /// Programs whose exit is queued for the request loop, so the watcher reports each once.
    pub(crate) exiting: BTreeSet<ThreadId>,
    /// Threads whose program was asked to stop, and why. Quit skips `turn.ended` only.
    pub(crate) stopping: BTreeMap<ThreadId, crate::thread::StopReason>,
    /// The `Started` seq of the run now in progress, or the run a saved screen belongs to.
    pub(crate) runs: BTreeMap<ThreadId, u64>,
    pub(crate) ends: BTreeMap<ThreadId, crate::end::End>,
    /// Loaded with the saved screen, so a reopen shows the same note.
    pub(crate) losses: BTreeMap<ThreadId, crate::end::Loss>,
    /// Runs whose settled screen was already offered to the store.
    pub(crate) saved: BTreeSet<(ThreadId, u64)>,
    pub(crate) diffs: std::sync::Arc<crate::diff::Cache>,
    keymap: kg_domain::Keymap,
    pub(crate) plugins: Option<PluginDispatch>,
    /// Where this workspace finds programs, when a test has set it; None is the login PATH
    pub(crate) program_environment: Option<crate::host::program::ProgramEnvironment>,
    /// Where Claude Code keeps its conversations, when a test has set it; None is `CLAUDE_CONFIG_DIR` or `~/.claude`
    pub(crate) claude_config: Option<std::path::PathBuf>,
    /// Where Codex keeps its conversations, when a test has set it; None is `CODEX_HOME` or `~/.codex`
    pub(crate) codex_home: Option<std::path::PathBuf>,
    /// Where Cursor keeps its conversations, when a test has set it; None is `CURSOR_CONFIG_DIR`, `$XDG_CONFIG_HOME/cursor`, or `~/.cursor`
    pub(crate) cursor_config: Option<std::path::PathBuf>,
    /// Where BB keeps its skills, when a test has set it; None is `BB_DATA_DIR` or `~/.bb`
    pub(crate) bb_data_dir: Option<std::path::PathBuf>,
    pub(crate) ui: Option<crate::dispatch::ui::UiDispatch>,
    sections: Vec<kg_domain::Section>,
    requests: Sender<Envelope>,
    // Wall time at open plus executor time, so grace deadlines follow GPUI's test clock.
    clock: (kg_domain::Timestamp, Instant),
    /// When set, folder removal waits until a value is received (tests only).
    #[cfg(test)]
    pub(crate) removal_hold: Option<async_channel::Receiver<()>>,
    #[cfg(test)]
    pub(crate) run_hold: Option<async_channel::Receiver<()>>,
    #[cfg(test)]
    pub(crate) delete_hold: Option<async_channel::Receiver<()>>,
    #[cfg(any(test, feature = "test"))]
    pub(crate) screen_hold: Option<async_channel::Receiver<()>>,
    _database_task: Task<()>,
    _requests_task: Task<()>,
    _retire_task: Task<()>,
    _quit: Subscription,
}
#[derive(Clone, Debug)]
pub enum WorkspaceEvent {
    Open,
    Settings,
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
}
impl EventEmitter<WorkspaceEvent> for Workspace {}

/// Where this app listens for `kg`; every process it starts gets it as `KG_SOCKET`.
pub struct SocketLocation(pub std::ffi::OsString);
impl Global for SocketLocation {}
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
    pub fn open(data_dir: PathBuf, cli: PathBuf, cx: &mut App) -> Task<Result<Entity<Self>>> {
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
                                let _ = workspace.requests.try_send((Request::Sweep, reply));
                                receive
                            });
                            if task.recv().await.is_err() {
                                break;
                            }
                        }
                    });
                    Self {
                        approvals: BTreeMap::new(),
                        replies: BTreeMap::new(),
                        for_a_person: BTreeSet::new(),
                        asks: BTreeMap::new(),
                        interaction_thread: BTreeMap::new(),
                        database,
                        data_dir,
                        cli,
                        sections: Vec::new(),
                        projects: BTreeMap::new(),
                        preparing_projects: BTreeMap::new(),
                        removing_projects: BTreeSet::new(),
                        environments: BTreeMap::new(),
                        threads: BTreeMap::new(),
                        tasks: BTreeMap::new(),
                        terminals: BTreeMap::new(),
                        extras: BTreeMap::new(),
                        sizes: BTreeMap::new(),
                        watching: false,
                        removing: BTreeSet::new(),
                        exiting: BTreeSet::new(),
                        stopping: BTreeMap::new(),
                        runs: BTreeMap::new(),
                        ends: BTreeMap::new(),
                        losses: BTreeMap::new(),
                        saved: BTreeSet::new(),
                        diffs: crate::diff::Cache::new(),
                        keymap: kg_domain::Keymap::default(),
                        plugins: None,
                        program_environment: None,
                        claude_config: None,
                        codex_home: None,
                        cursor_config: None,
                        bb_data_dir: None,
                        ui: None,
                        requests,
                        clock: (now(), cx.background_executor().now()),
                        #[cfg(test)]
                        removal_hold: None,
                        #[cfg(test)]
                        run_hold: None,
                        #[cfg(test)]
                        delete_hold: None,
                        #[cfg(any(test, feature = "test"))]
                        screen_hold: None,
                        _database_task: database_task,
                        _requests_task: requests_task,
                        _retire_task: retire_task,
                        _quit: cx.on_app_quit(|workspace, _| {
                            workspace.end_programs();
                            async {}
                        }),
                    }
                });
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
    /// Picks up what the previous process left: interrupted removals.
    async fn resume(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> Result<()> {
        Self::refresh(this, cx).await?;
        crate::settings::remember_keymap(this, cx).await?;
        let submitted = this
            .update(cx, |workspace, _| workspace.submit(Request::Leftovers))
            .map_err(closed)?;
        submitted.await.map(drop)
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
    pub fn rename_once(id: ThreadId, title: String, cx: &mut App) -> Task<Result<OpResult>> {
        let op = Op::Thread(kg_domain::ThreadOp::Rename {
            id: id.clone(),
            title: title.clone(),
        });
        if let Err(message) = kg_domain::check(&op) {
            return Task::ready(Err(OpError::Invalid(message.to_owned())));
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
            return Task::ready(Err(OpError::Invalid(message.to_owned())));
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
                let project = run.project_id.clone();
                let allowed = workspace.update(cx, |workspace, _| {
                    if workspace.removing_projects.contains(&project) {
                        return false;
                    }
                    *workspace
                        .preparing_projects
                        .entry(project.clone())
                        .or_default() += 1;
                    true
                });
                if !allowed {
                    return Task::ready(Err(OpError::Invalid(
                        "this project is being removed".into(),
                    )));
                }
                detached(cx, async move |cx| {
                    let result = async {
                        let prepared = crate::thread::prepare_run(&this, run, cx).await?;
                        let submitted = this
                            .update(cx, |workspace, _| {
                                workspace.submit(Request::Run(Box::new(prepared)))
                            })
                            .map_err(closed)?;
                        submitted.await
                    }
                    .await;
                    this.update(cx, |workspace, _| {
                        if let Some(count) = workspace.preparing_projects.get_mut(&project) {
                            *count -= 1;
                            if *count == 0 {
                                workspace.preparing_projects.remove(&project);
                            }
                        }
                    })
                    .map_err(closed)?;
                    result
                })
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
            sent.map_err(|_| OpError::Failed("workspace closed".into()))?;
            receive
                .recv()
                .await
                .map_err(|_| OpError::Failed("workspace closed".into()))?
        }
    }
}
/// Runs work that the caller's task cannot cancel by being dropped.
fn detached(
    cx: &mut App,
    work: impl AsyncFnOnce(&mut AsyncApp) -> Result<OpResult> + 'static,
) -> Task<Result<OpResult>> {
    let (reply, receive) = async_channel::bounded(1);
    cx.spawn(async move |cx| {
        let _ = reply.send(work(cx).await).await;
    })
    .detach();
    cx.spawn(async move |_| {
        receive
            .recv()
            .await
            .map_err(|_| OpError::Failed("workspace closed".into()))?
    })
}
