use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_channel::Sender;
use gpui_kit::{
    App, AppContext, AsyncApp, Context, Entity, EventEmitter, Global, Subscription, Task,
    WeakEntity,
};
use kg_domain::{
    EnvironmentId, Event, InteractionOption, KeymapOp, Op, OpError, OpResult, PendingInteractionId,
    Plugin, PluginId, PluginOp, ProjectId, ThreadId,
};
use kg_store::Store;
use serde_json::Value;

use crate::database::{Database, Result};
use crate::{Environment, Project, Requester, Thread};

pub(crate) const GRACE: i64 = 10_000;
const QUIT_DEADLINE: Duration = Duration::from_secs(3);

fn ensure_personal(data_dir: &Path, store: &Store) -> Result<()> {
    let personal = data_dir.join("personal");
    std::fs::create_dir_all(&personal)?;
    if store
        .project(&kg_domain::ProjectId("personal".into()))
        .map_err(closed)?
        .is_none()
    {
        store
            .put_project(&kg_domain::Project {
                id: kg_domain::ProjectId("personal".into()),
                kind: kg_domain::ProjectKind::Personal,
                name: "Personal".into(),
                path: dunce::canonicalize(&personal)?,
                git_remote_url: None,
            })
            .map_err(closed)?;
    }
    Ok(())
}

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
    SocketRename {
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
    Plugin(PluginOp, Option<Requester>),
    /// A shortcut of a plugin's command, named `<plugin-id>:<command>`
    Keymap(KeymapOp),
}

/// Lets the plugin host serve its operations without core depending on it.
pub type PluginDispatch = Rc<dyn Fn(PluginRequest, &mut App) -> Task<Result<OpResult>>>;

pub struct Workspace {
    pub(crate) approvals: BTreeMap<PendingInteractionId, crate::approval::Waiting>,
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
    /// Threads whose program a person asked to stop; its exit records the stop.
    pub(crate) stopping: BTreeSet<ThreadId>,
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
    pub(crate) program_environment: Option<crate::program::Environment>,
    pub(crate) ui: Option<crate::ui::UiDispatch>,
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
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) screen_hold: Option<async_channel::Receiver<()>>,
    _database_task: Task<()>,
    _requests_task: Task<()>,
    _retire_task: Task<()>,
    _quit: Subscription,
}

impl Workspace {
    /// Built-in shortcut overrides last read or written by a keymap operation.
    #[must_use]
    pub fn keymap(&self) -> &kg_domain::Keymap {
        &self.keymap
    }

    pub(crate) fn set_keymap(&mut self, keymap: kg_domain::Keymap) {
        self.keymap = keymap;
    }
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
    /// Opens the database and installs the workspace global. I/O runs in the background.
    ///
    /// # Errors
    /// Returns an error if the data folder or database cannot be opened.
    pub fn open(data_dir: PathBuf, cli: PathBuf, cx: &mut App) -> Task<Result<Entity<Self>>> {
        let executor = cx.background_executor().clone();
        let opened = executor.spawn(async move {
            std::fs::create_dir_all(&data_dir)?;
            let data_dir = dunce::canonicalize(&data_dir)?;
            let store = Store::open(data_dir.join("kagan.sqlite3"))
                .map_err(|error| OpError::Failed(error.to_string()))?;
            ensure_personal(&data_dir, &store)?;
            Ok::<_, OpError>((store, data_dir))
        });
        cx.spawn(async move |cx| {
            let (store, data_dir) = opened.await?;
            let (database, database_task) = Database::new(store, &executor);
            database.call(interrupted).await?;
            let workspace = cx.update(|cx| {
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
                        stopping: BTreeSet::new(),
                        runs: BTreeMap::new(),
                        ends: BTreeMap::new(),
                        losses: BTreeMap::new(),
                        saved: BTreeSet::new(),
                        diffs: crate::diff::Cache::new(),
                        keymap: kg_domain::Keymap::default(),
                        plugins: None,
                        program_environment: None,
                        ui: None,
                        requests,
                        clock: (now(), cx.background_executor().now()),
                        #[cfg(test)]
                        removal_hold: None,
                        #[cfg(test)]
                        run_hold: None,
                        #[cfg(test)]
                        delete_hold: None,
                        #[cfg(any(test, feature = "test-support"))]
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
            Request::Op(op) => crate::thread::execute(this, op, None, cx).await,
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
                let result = crate::thread::remove_project(this, id.clone(), threads, cx).await;
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
            } => crate::thread::answer(this, thread_id, interaction_id, option_id, cx).await,
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
            Request::SocketRename { id, title } => {
                crate::thread::rename_from_socket(this, id, title, cx).await
            }
        }
    }

    /// Picks up what the previous process left: interrupted removals.
    async fn resume(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> Result<()> {
        Self::refresh(this, cx).await?;
        crate::thread::remember_keymap(this, cx).await?;
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
    #[cfg(any(test, feature = "test-support"))]
    pub fn use_program_environment(&mut self, path: &std::path::Path, shell: &std::path::Path) {
        self.program_environment = Some(crate::program::Environment {
            path: path.as_os_str().to_owned(),
            shell: shell.as_os_str().to_owned(),
        });
    }

    pub fn rename_from_socket(id: ThreadId, title: String, cx: &mut App) -> Task<Result<OpResult>> {
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
        let submitted = global
            .0
            .read(cx)
            .submit(Request::SocketRename { id, title });
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
                if requester.is_some() && key.starts_with("slot.") =>
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
            op if crate::thread::serialized(&op) => {
                let submitted = workspace.read(cx).submit(Request::Op(op));
                cx.spawn(async move |_| submitted.await)
            }
            op => detached(cx, async move |cx| {
                crate::thread::execute(&this, op, requester, cx).await
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

    async fn sweep(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> Result<OpResult> {
        let (changed, retirements) = crate::environment::sweep(this, cx).await?;
        if changed {
            Self::refresh(this, cx).await?;
        }
        Self::remove(this, retirements, cx)?;
        Ok(OpResult::Done)
    }

    async fn leftovers(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> Result<OpResult> {
        let leftovers = crate::environment::leftovers(this, cx).await?;
        Self::remove(this, leftovers, cx)?;
        Ok(OpResult::Done)
    }

    pub(crate) fn remove(
        this: &WeakEntity<Self>,
        retirements: Vec<crate::environment::Retirement>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        for retirement in retirements {
            let environment = retirement.environment.clone();
            this.update(cx, |workspace, cx| {
                workspace.removing.insert(environment.id.clone());
                #[cfg(test)]
                let hold = workspace.removal_hold.clone();
                cx.spawn(async move |this, cx| {
                    #[cfg(test)]
                    if let Some(hold) = hold {
                        let _ = hold.recv().await;
                    }
                    let outcome = crate::environment::remove_folder(&this, retirement, cx).await;
                    let submitted = this
                        .update(cx, |workspace, _| {
                            workspace.submit(Request::RemovalDone {
                                environment,
                                outcome,
                            })
                        })
                        .map_err(closed);
                    if let Ok(submitted) = submitted {
                        let _ = submitted.await;
                    }
                    let _ = Self::refresh(&this, cx).await;
                })
                .detach();
            })
            .map_err(closed)?;
        }
        Ok(())
    }

    pub fn set_plugin_host(&mut self, dispatch: PluginDispatch) {
        self.plugins = Some(dispatch);
    }

    pub fn set_ui_driver(&mut self, dispatch: crate::ui::UiDispatch) {
        self.ui = Some(dispatch);
    }

    pub fn end_programs(&mut self) {
        self.extras.clear();
        self.terminals.clear();
    }

    pub fn stop_for_quit(cx: &mut App) -> Task<Result<()>> {
        let workspace = Self::global(cx);
        let ids: Vec<_> = workspace
            .read(cx)
            .threads
            .iter()
            .filter(|(_, thread)| thread.read(cx).running())
            .map(|(id, _)| id.clone())
            .collect();
        let stops: Vec<_> = ids
            .iter()
            .map(|id| Self::dispatch(Op::Thread(kg_domain::ThreadOp::Stop { id: id.clone() }), cx))
            .collect();
        cx.spawn(async move |cx| {
            let deadline = cx.background_executor().timer(QUIT_DEADLINE);
            let work = async {
                let mut stopped = Ok(());
                for stop in stops {
                    if let Err(error) = stop.await {
                        stopped = Err(error);
                    }
                }
                loop {
                    let done = workspace.read_with(cx, |workspace, cx| {
                        ids.iter().all(|id| {
                            let running = workspace
                                .threads
                                .get(id)
                                .is_some_and(|thread| thread.read(cx).running());
                            let saving = matches!(
                                workspace.terminals.get(id),
                                Some(crate::thread::Shown::Ended(_))
                            ) && workspace
                                .runs
                                .get(id)
                                .is_some_and(|seq| !workspace.saved.contains(&(id.clone(), *seq)));
                            !running && !saving
                        })
                    });
                    if done {
                        return stopped;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(50))
                        .await;
                }
            };
            futures::pin_mut!(work, deadline);
            match futures::future::select(work, deadline).await {
                futures::future::Either::Left((result, _)) => result,
                futures::future::Either::Right(_) => {
                    Err(OpError::Failed("timed out stopping programs".into()))
                }
            }
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn hold_screen_save(&mut self, hold: async_channel::Receiver<()>) {
        self.screen_hold = Some(hold);
    }

    #[must_use]
    pub fn running_programs(&self, cx: &App) -> usize {
        self.threads
            .values()
            .filter(|thread| thread.read(cx).running())
            .count()
    }

    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn terminal_process_id(&self, id: &ThreadId) -> Option<u32> {
        match self.terminals.get(id)? {
            crate::thread::Shown::Running(terminal) => Some(terminal.process_id()),
            crate::thread::Shown::Ended(_) => None,
        }
    }

    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn call<R, F>(&self, call: F) -> impl Future<Output = Result<R>> + use<R, F>
    where
        R: Send + 'static,
        F: FnOnce(&mut Store) -> kg_store::Result<R> + Send + 'static,
    {
        let database = self.database.clone();
        async move { database.call(call).await }
    }

    /// # Errors
    /// When the database is closed or the query fails.
    pub fn plugins(&self) -> impl Future<Output = Result<Vec<Plugin>>> + use<> {
        self.call(|store| store.plugins())
    }

    /// # Errors
    /// When the database is closed, the id exists, or the query fails.
    pub fn insert_plugin(&self, plugin: Plugin) -> impl Future<Output = Result<()>> + use<> {
        self.call(move |store| store.insert_plugin(&plugin))
    }

    /// Leaves every column other than `enabled` as stored.
    ///
    /// # Errors
    /// When the database is closed, the plugin is missing, or the query fails.
    pub fn set_plugin_enabled(
        &self,
        id: PluginId,
        enabled: bool,
    ) -> impl Future<Output = Result<()>> + use<> {
        self.call(move |store| store.set_plugin_enabled(&id, enabled))
    }

    /// Keeps stored paths for folders the new grant still names, and leaves `enabled` unchanged.
    ///
    /// # Errors
    /// When the database is closed or the query fails.
    pub fn keep_plugin(
        &self,
        id: PluginId,
        root: PathBuf,
        source: String,
        commit: Option<String>,
        mut approved: Value,
    ) -> impl Future<Output = Result<Option<Plugin>>> + use<> {
        self.call(move |store| {
            let Some(existing) = store.plugins()?.into_iter().find(|plugin| plugin.id == id) else {
                return Ok(None);
            };
            copy_kept_paths(&mut approved, &existing.approved);
            store.keep_plugin_checkout(&id, &root, &source, &commit, &approved)?;
            Ok(Some(Plugin {
                id,
                root,
                source,
                commit,
                enabled: existing.enabled,
                approved,
                error: None,
            }))
        })
    }

    /// # Errors
    /// When the database is closed or removing the plugin fails.
    pub fn remove_plugin(&self, id: PluginId) -> impl Future<Output = Result<()>> + use<> {
        self.call(move |store| store.remove_plugin(&id))
    }

    /// # Errors
    /// When the database is closed or the query fails.
    pub fn plugin_value(
        &self,
        id: PluginId,
        key: String,
    ) -> impl Future<Output = Result<Option<Value>>> + use<> {
        self.call(move |store| store.plugin_value(&id, &key))
    }

    /// # Errors
    /// When the database is closed or the query fails.
    pub fn set_plugin_value(
        &self,
        id: PluginId,
        key: String,
        value: Value,
    ) -> impl Future<Output = Result<()>> + use<> {
        self.call(move |store| store.set_plugin_value(&id, &key, &value))
    }

    /// Writes the setting value only while `permit` is `Some(activation)`. A folder save
    /// also writes that path on `approved` when the key is still a folder, and does not
    /// change `root`, `source`, `commit`, or `enabled`. When that path cannot be written,
    /// it writes nothing.
    ///
    /// # Errors
    /// When the database is closed or the query fails.
    pub fn commit_plugin_config(
        &self,
        id: PluginId,
        key: String,
        value: Value,
        folder: Option<String>,
        permit: Arc<Mutex<Option<u64>>>,
        activation: u64,
    ) -> impl Future<Output = Result<bool>> + use<> {
        self.call(move |store| {
            write_plugin_config(
                store,
                &id,
                &key,
                &value,
                folder.as_deref(),
                &permit,
                activation,
            )
        })
    }

    /// Answers a pending question. Not an `Op`, so the socket and MCP cannot answer.
    /// The threads host calls this for a question a plugin may answer.
    pub fn answer(
        thread_id: &ThreadId,
        interaction_id: PendingInteractionId,
        option_id: String,
        cx: &mut App,
    ) -> Task<Result<OpResult>> {
        let Some(global) = cx.try_global::<WorkspaceGlobal>() else {
            return Task::ready(Err(OpError::Failed("workspace is not open".into())));
        };
        let workspace = global.0.clone();
        let submitted = workspace.read(cx).submit(Request::Answer {
            thread_id: thread_id.clone(),
            interaction_id,
            option_id,
        });
        cx.spawn(async move |_| submitted.await)
    }

    /// Raises a question on the thread and resolves with the chosen option id.
    /// [`Self::answer`] answers it, including from a plugin. Fails when the thread
    /// stops or the question is otherwise cancelled.
    pub fn ask(
        thread_id: &ThreadId,
        title: String,
        options: Vec<InteractionOption>,
        cx: &mut App,
    ) -> Task<Result<String>> {
        Self::raise_question(thread_id, title, options, false, None, cx)
    }

    /// Raises a question a plugin cannot answer, on the thread or standalone. Install, a
    /// provider change, and thread delete use this. [`Self::answer`] still answers it for a
    /// person. The question goes, and the task fails, when the requester goes.
    pub fn ask_for_person(
        thread_id: Option<&ThreadId>,
        requester: Option<Requester>,
        title: String,
        options: Vec<InteractionOption>,
        cx: &mut App,
    ) -> Task<Result<String>> {
        match thread_id {
            Some(thread_id) => Self::raise_question(thread_id, title, options, true, requester, cx),
            None => Self::standalone(title, options, requester, cx),
        }
    }

    fn raise_question(
        thread_id: &ThreadId,
        title: String,
        options: Vec<InteractionOption>,
        for_a_person: bool,
        requester: Option<Requester>,
        cx: &mut App,
    ) -> Task<Result<String>> {
        if requester.as_ref().is_some_and(Requester::is_gone) {
            return Task::ready(Err(Requester::stopped()));
        }
        let Some(global) = cx.try_global::<WorkspaceGlobal>() else {
            return Task::ready(Err(OpError::Failed("workspace is not open".into())));
        };
        let this = global.0.downgrade();
        let thread_id = thread_id.clone();
        cx.spawn(async move |cx| {
            let thread = crate::thread::thread(&this, &thread_id, cx)?;
            if thread.read_with(cx, |thread, _| thread.record.archived_at.is_some()) {
                return Err(OpError::Invalid(format!(
                    "thread {thread_id} is archived; ask from an active thread"
                )));
            }
            let (answer, answered) = async_channel::bounded(1);
            let raised = crate::thread::raise(
                &this,
                &thread,
                title,
                options,
                |workspace, thread_id, id| {
                    if for_a_person {
                        workspace.for_a_person.insert(id.clone());
                    }
                    crate::thread::insert_reply(workspace, thread_id.clone(), id, answer);
                },
                cx,
            )
            .await?;
            let Some(answer) = Requester::unless_gone(requester.as_ref(), answered.recv()).await
            else {
                crate::thread::withdraw(&this, raised, cx).await?;
                return Err(Requester::stopped());
            };
            answer.map_err(|_| {
                OpError::Failed(format!("the question on thread {thread_id} was cancelled"))
            })
        })
    }

    /// An install, a provider change, a thread delete, or a standalone approval.
    #[must_use]
    pub fn needs_a_person(&self, id: &PendingInteractionId) -> bool {
        self.for_a_person.contains(id) || self.approvals.contains_key(id)
    }

    #[must_use]
    pub fn sections(&self) -> &[kg_domain::Section] {
        &self.sections
    }

    #[must_use]
    pub fn sidebar(&self, cx: &App) -> kg_domain::Sidebar {
        let mut sidebar = kg_domain::Sidebar {
            projects: self
                .projects
                .values()
                .map(|project| project.read(cx).record().clone())
                .collect(),
            sections: self.sections.clone(),
            threads: self
                .threads
                .values()
                .map(|thread| thread.read(cx).record())
                .filter(|thread| {
                    thread.visibility == kg_domain::Visibility::Visible
                        && thread.archived_at.is_none()
                })
                .cloned()
                .collect(),
        };
        kg_domain::order_sidebar_projects(&mut sidebar.projects);
        sidebar.threads.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        sidebar
    }

    #[must_use]
    pub fn project(&self, id: &ProjectId) -> Option<&Entity<Project>> {
        self.projects.get(id)
    }
    #[must_use]
    pub fn thread(&self, id: &ThreadId) -> Option<&Entity<Thread>> {
        self.threads.get(id)
    }

    #[must_use]
    pub fn thread_records(&self, cx: &App) -> Vec<kg_domain::Thread> {
        self.threads
            .values()
            .map(|thread| thread.read(cx).record().clone())
            .collect()
    }
    #[must_use]
    pub fn environment(&self, id: &EnvironmentId) -> Option<&Entity<Environment>> {
        self.environments.get(id)
    }

    pub(crate) async fn refresh(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> Result<()> {
        let database = this
            .update(cx, |workspace, _| workspace.database.clone())
            .map_err(closed)?;
        let (projects, environments, threads, sections, endings, screens) = database
            .call(|store| {
                let projects = store.projects()?;
                let environments = projects
                    .iter()
                    .map(|project| store.environments(&project.id))
                    .collect::<kg_store::Result<Vec<_>>>()?
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>();
                Ok((
                    projects,
                    environments,
                    store.threads(None, None, None, None)?,
                    store.sections()?,
                    store.endings()?,
                    store.screens()?,
                ))
            })
            .await?;
        this.update(cx, |workspace, cx| {
            workspace.sections = sections;
            workspace
                .projects
                .retain(|id, _| projects.iter().any(|record| &record.id == id));
            workspace
                .environments
                .retain(|id, _| environments.iter().any(|record| &record.id == id));
            for record in projects {
                if let Some(entity) = workspace.projects.get(&record.id) {
                    entity.update(cx, |project, cx| {
                        project.record = record;
                        cx.notify();
                    });
                } else {
                    workspace
                        .projects
                        .insert(record.id.clone(), cx.new(|_| Project { record }));
                }
            }
            for record in environments {
                if let Some(entity) = workspace.environments.get(&record.id) {
                    entity.update(cx, |environment, cx| {
                        environment.record = record;
                        cx.notify();
                    });
                } else {
                    workspace
                        .environments
                        .insert(record.id.clone(), cx.new(|_| Environment { record }));
                }
            }
            workspace
                .threads
                .retain(|id, _| threads.iter().any(|record| &record.id == id));
            for record in threads {
                if let Some(entity) = workspace.threads.get(&record.id) {
                    entity.update(cx, |thread, cx| {
                        thread.record = record;
                        cx.notify();
                    });
                } else {
                    workspace
                        .threads
                        .insert(record.id.clone(), cx.new(|_| Thread::new(record)));
                }
            }
            workspace.ends.clear();
            for (id, kinds) in endings {
                if let Some(end) = crate::end::End::from_newest(&kinds) {
                    workspace.ends.insert(id, end);
                }
            }
            workspace.losses = remember(workspace, screens);
            cx.emit(WorkspaceEvent::Changed);
            cx.notify();
        })
        .map_err(closed)
    }

    #[must_use]
    pub fn end_line(&self, id: &ThreadId) -> Option<String> {
        self.ends.get(id).map(crate::end::End::line)
    }

    #[must_use]
    pub fn screen_note(&self, id: &ThreadId) -> Option<&'static str> {
        self.losses.get(id).copied().map(crate::end::Loss::note)
    }

    #[cfg(feature = "test-support")]
    pub fn set_snapshot_bound(&mut self, id: &ThreadId, bound: usize) -> bool {
        let Some(crate::thread::Shown::Running(terminal)) = self.terminals.get(id) else {
            return false;
        };
        terminal.set_snapshot_bound(bound);
        true
    }

    /// A stop or any exit code. A signal, a quit, a failure, or an interrupted start does
    /// not count.
    #[must_use]
    pub fn program_ended(&self, id: &ThreadId) -> bool {
        self.ends.get(id).is_some_and(crate::end::End::shows_mark)
    }

    /// A non-zero exit, a signal, a quit, a failure, an interrupted start, or a screen that was
    /// not kept in full.
    #[must_use]
    pub fn recovery(&self, id: &ThreadId) -> bool {
        self.screen_note(id).is_some()
            || self.ends.get(id).is_some_and(|end| {
                !matches!(end, crate::end::End::Stopped | crate::end::End::Exited(0))
            })
    }

    pub(crate) async fn events(
        this: &WeakEntity<Self>,
        id: ThreadId,
        kinds: Vec<kg_domain::EventKind>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        let database = this
            .update(cx, |workspace, _| workspace.database.clone())
            .map_err(closed)?;
        let at = time(this, cx)?;
        let started = kinds
            .iter()
            .any(|kind| matches!(kind, kg_domain::EventKind::Started));
        let thread_id = id.clone();
        let (project_id, events) = database
            .call(move |store| {
                let project_id = store
                    .thread(&id)?
                    .ok_or(kg_store::Error::NotFound {
                        what: "thread",
                        id: id.to_string(),
                    })?
                    .project_id;
                let events = store.append_events(&id, at, kinds)?;
                if started {
                    store.delete_screen(&id)?;
                }
                Ok((project_id, events))
            })
            .await?;
        this.update(cx, |workspace, cx| {
            let mut newest = Vec::new();
            let mut run = None;
            for event in &events {
                if matches!(event.kind, kg_domain::EventKind::Started) {
                    run = Some(event.seq);
                }
                if matches!(
                    event.kind,
                    kg_domain::EventKind::Stopped
                        | kg_domain::EventKind::Exited { .. }
                        | kg_domain::EventKind::Failed { .. }
                ) {
                    newest.push(event.kind.clone());
                }
            }
            newest.reverse();
            if let Some(seq) = run {
                workspace.runs.insert(thread_id.clone(), seq);
                workspace.saved.retain(|(thread, _)| thread != &thread_id);
                workspace.losses.remove(&thread_id);
            }
            if let Some(end) = crate::end::End::from_newest(&newest) {
                workspace.ends.insert(thread_id, end);
            }
            for event in events {
                cx.emit(WorkspaceEvent::Event {
                    event,
                    project_id: project_id.clone(),
                });
            }
            cx.notify();
        })
        .map_err(closed)
    }
}

fn interrupted(store: &mut Store) -> kg_store::Result<()> {
    let at = now();
    for mut record in store.threads(None, None, None, None)? {
        close_left_turn(store, &mut record, at)?;
        if !matches!(
            record.status,
            kg_domain::ThreadStatus::Starting | kg_domain::ThreadStatus::Active
        ) {
            clear_pending(store, &record.id)?;
        }
    }
    close_left_environments(store)
}

fn close_left_turn(
    store: &mut Store,
    record: &mut kg_domain::Thread,
    at: kg_domain::Timestamp,
) -> kg_store::Result<()> {
    match record.status {
        kg_domain::ThreadStatus::Starting
        | kg_domain::ThreadStatus::Active
        | kg_domain::ThreadStatus::Stopping => {
            let (error, before_turn) = if record.status == kg_domain::ThreadStatus::Starting {
                (crate::end::STARTING, true)
            } else {
                (crate::end::QUIT, false)
            };
            let own = vec![kg_domain::EventKind::Failed {
                error: Some(error.into()),
            }];
            record.status = kg_domain::ThreadStatus::Error;
            record.latest_attention_at = at;
            record.updated_at = at;
            let notice = parent_notice(store, record, error, before_turn)?;
            store.recover_thread(record, at, own, notice)
        }
        kg_domain::ThreadStatus::Pending
        | kg_domain::ThreadStatus::Idle
        | kg_domain::ThreadStatus::Error => Ok(()),
    }
}

fn clear_pending(store: &Store, id: &ThreadId) -> kg_store::Result<()> {
    for interaction in store.pending_interactions(id)? {
        store.remove_pending_interaction(&interaction.id)?;
    }
    Ok(())
}

fn close_left_environments(store: &mut Store) -> kg_store::Result<()> {
    let threads = store.threads(None, None, None, None)?;
    for project in store.projects()? {
        for mut environment in store.environments(&project.id)? {
            if matches!(
                environment.status,
                kg_domain::EnvironmentStatus::Creating | kg_domain::EnvironmentStatus::Provisioning
            ) {
                let used = threads
                    .iter()
                    .any(|thread| thread.environment_id.as_ref() == Some(&environment.id));
                environment.status = if environment.provider.managed() && !used {
                    kg_domain::EnvironmentStatus::Destroyed
                } else {
                    kg_domain::EnvironmentStatus::Error
                };
                store.put_environment(&environment)?;
            }
        }
    }
    Ok(())
}

/// The notice a parent gets when a child fails at startup, if it has a live parent.
fn parent_notice(
    store: &Store,
    child: &kg_domain::Thread,
    error: &str,
    before_turn: bool,
) -> kg_store::Result<Option<(ThreadId, kg_domain::EventKind)>> {
    let Some(parent_id) = &child.parent_thread_id else {
        return Ok(None);
    };
    let Some(parent) = store.thread(parent_id)? else {
        return Ok(None);
    };
    if parent.archived_at.is_some() {
        return Ok(None);
    }
    let text = crate::thread::child_notice(&child.title, Some(error), before_turn);
    Ok(Some((
        parent_id.clone(),
        kg_domain::EventKind::Notice {
            severity: kg_domain::NoticeSeverity::Info,
            text,
        },
    )))
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

pub(crate) fn now() -> kg_domain::Timestamp {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    kg_domain::Timestamp(i64::try_from(millis).unwrap_or(i64::MAX))
}

pub(crate) fn id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}
pub(crate) fn closed(error: impl std::fmt::Display) -> OpError {
    OpError::Failed(error.to_string())
}
pub(crate) fn missing(what: &str, id: &impl std::fmt::Display) -> OpError {
    OpError::NotFound(format!("{what} {id} not found"))
}

fn remember(
    workspace: &mut Workspace,
    screens: Vec<kg_store::ScreenRow>,
) -> BTreeMap<ThreadId, crate::end::Loss> {
    let mut losses = BTreeMap::new();
    for screen in screens {
        let id = screen.thread_id.clone();
        workspace.runs.insert(id.clone(), screen.started_seq);
        workspace.saved.insert((id.clone(), screen.started_seq));
        if let Some(loss) = crate::end::Loss::from_kept(screen.screen_kept, screen.history_dropped)
        {
            losses.insert(id.clone(), loss);
        }
        if screen.screen_kept && !workspace.terminals.contains_key(&id) {
            match kg_terminal::Finished::restore(screen.cols, screen.rows, &screen.replay) {
                Some(finished) => {
                    workspace
                        .terminals
                        .insert(id, crate::thread::Shown::Ended(finished));
                }
                None => {
                    eprintln!(
                        "kg: saved screen for {id} was outside the bound and was not restored"
                    );
                }
            }
        }
    }
    losses
}

pub(crate) fn time(
    this: &WeakEntity<Workspace>,
    cx: &mut AsyncApp,
) -> Result<kg_domain::Timestamp> {
    this.update(cx, |workspace, cx| {
        let elapsed = cx
            .background_executor()
            .now()
            .duration_since(workspace.clock.1)
            .as_millis();
        kg_domain::Timestamp(
            workspace
                .clock
                .0
                .0
                .saturating_add(i64::try_from(elapsed).unwrap_or(i64::MAX)),
        )
    })
    .map_err(closed)
}

fn write_plugin_config(
    store: &mut Store,
    id: &PluginId,
    key: &str,
    value: &Value,
    folder: Option<&str>,
    permit: &Mutex<Option<u64>>,
    activation: u64,
) -> kg_store::Result<bool> {
    let guard = permit
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *guard != Some(activation) {
        return Ok(false);
    }
    if let Some(path) = folder {
        let Some(mut row) = store.plugins()?.into_iter().find(|plugin| plugin.id == *id) else {
            return Ok(false);
        };
        if !patch_folder(&mut row, key, path) {
            return Ok(false);
        }
        store.set_plugin_approved(&row.id, &row.approved)?;
    }
    store.set_plugin_value(id, key, value)?;
    Ok(true)
}

fn copy_kept_paths(approved: &mut Value, existing: &Value) {
    let folder_keys: Vec<String> = approved
        .get("folders")
        .and_then(Value::as_object)
        .map(|folders| folders.keys().cloned().collect())
        .unwrap_or_default();
    let existing_paths = existing
        .get("paths")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let Some(approved) = approved.as_object_mut() else {
        return;
    };
    let mut paths = serde_json::Map::new();
    for key in folder_keys {
        if let Some(path) = existing_paths.get(&key) {
            paths.insert(key, path.clone());
        }
    }
    approved.insert("paths".to_owned(), Value::Object(paths));
}

fn patch_folder(plugin: &mut Plugin, key: &str, path: &str) -> bool {
    let Some(approved) = plugin.approved.as_object_mut() else {
        return false;
    };
    let still_folder = approved
        .get("folders")
        .and_then(Value::as_object)
        .is_some_and(|folders| folders.contains_key(key));
    if !still_folder {
        return false;
    }
    let paths = approved
        .entry("paths")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let Some(paths) = paths.as_object_mut() else {
        return false;
    };
    paths.insert(key.to_owned(), Value::String(path.to_owned()));
    true
}
