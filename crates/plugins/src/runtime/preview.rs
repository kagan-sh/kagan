//! The hidden kagan a plugin preview runs in: its folder, its socket, and the processes that run it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use async_channel::Receiver;
use gpui_kit::{BackgroundExecutor, Context, Entity, Subscription};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{
    EventKind, OpError, PluginId, PluginPreview, PreviewId, SOCKET_PATH_MAX, ThreadId,
};
use kg_process::Process;

use crate::{PluginHost, Result};

const READY_WITHIN: Duration = Duration::from_secs(20);
const QUIT_WITHIN: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(200);
pub(crate) const APP_LOG: &str = "app.log";
pub(crate) const WATCH_LOG: &str = "watch.log";
const LOG_TAIL_LINES: usize = 12;

pub(crate) struct Running {
    pub(crate) info: PluginPreview,
    pub(crate) folder: PathBuf,
    app: Process,
    watch: Process,
}

impl Running {
    pub(crate) fn new(info: PluginPreview, folder: PathBuf, app: Process, watch: Process) -> Self {
        Self {
            info,
            folder,
            app,
            watch,
        }
    }
}

// The previews this app runs, and the ids a start has reserved before its folder exists.
pub(crate) struct Previews {
    running: BTreeMap<PreviewId, Running>,
    starting: BTreeSet<PreviewId>,
    _events: Subscription,
    _quit: Subscription,
}

impl Previews {
    pub(crate) fn new(workspace: &Entity<Workspace>, cx: &mut Context<PluginHost>) -> Self {
        Self {
            running: BTreeMap::new(),
            starting: BTreeSet::new(),
            _events: cx.subscribe(workspace, |host, _, event: &WorkspaceEvent, cx| {
                end_on(&mut host.previews, event, cx);
            }),
            _quit: cx.on_app_quit(|host, cx| {
                let ended: Vec<_> = host
                    .previews
                    .take_all()
                    .into_iter()
                    .map(|running| shut_down_later(running, cx.background_executor()))
                    .collect();
                async move {
                    futures::future::join_all(
                        ended
                            .into_iter()
                            .map(|ended| async move { ended.recv().await }),
                    )
                    .await;
                }
            }),
        }
    }

    pub(crate) fn infos(&self) -> Vec<PluginPreview> {
        self.running
            .values()
            .map(|running| running.info.clone())
            .collect()
    }

    // Every id a folder may belong to: running previews and the starts still making theirs.
    pub(crate) fn ids(&self) -> Vec<PreviewId> {
        self.running.keys().chain(&self.starting).cloned().collect()
    }

    pub(crate) fn reserve(&mut self, id: PreviewId) {
        self.starting.insert(id);
    }

    pub(crate) fn release(&mut self, id: &PreviewId) {
        self.starting.remove(id);
    }

    pub(crate) fn insert(&mut self, running: Running) {
        self.running.insert(running.info.id.clone(), running);
    }

    pub(crate) fn remove(&mut self, id: &PreviewId) -> Option<Running> {
        self.running.remove(id)
    }

    // The preview a thread started for this plugin, which a new start replaces.
    pub(crate) fn take_for(
        &mut self,
        thread_id: Option<&ThreadId>,
        plugin: &PluginId,
    ) -> Option<Running> {
        take_matching(&mut self.running, thread_id, plugin)
    }

    fn take_for_thread(&mut self, thread_id: &ThreadId) -> Vec<Running> {
        let ids: Vec<PreviewId> = self
            .running
            .values()
            .filter(|running| running.info.thread_id.as_ref() == Some(thread_id))
            .map(|running| running.info.id.clone())
            .collect();
        ids.iter()
            .filter_map(|id| self.running.remove(id))
            .collect()
    }

    pub(crate) fn take_all(&mut self) -> Vec<Running> {
        std::mem::take(&mut self.running).into_values().collect()
    }

    // The previews whose app or reload watch has exited. Their processes are already gone.
    pub(crate) fn take_dead(&mut self) -> Vec<Running> {
        let dead: Vec<PreviewId> = self
            .running
            .iter_mut()
            .filter_map(|(id, running)| {
                let exited = running.app.child().try_status().ok().flatten().is_some()
                    || running.watch.child().try_status().ok().flatten().is_some();
                exited.then(|| id.clone())
            })
            .collect();
        dead.iter()
            .filter_map(|id| self.running.remove(id))
            .collect()
    }
}

pub(crate) fn take_matching(
    running: &mut BTreeMap<PreviewId, Running>,
    thread_id: Option<&ThreadId>,
    plugin: &PluginId,
) -> Option<Running> {
    let id = running
        .values()
        .find(|running| {
            running.info.thread_id.as_ref() == thread_id && running.info.plugin == *plugin
        })
        .map(|running| running.info.id.clone())?;
    running.remove(&id)
}

fn end_on(previews: &mut Previews, event: &WorkspaceEvent, cx: &mut Context<PluginHost>) {
    let WorkspaceEvent::Event { event, .. } = event else {
        return;
    };
    if !matches!(event.kind, EventKind::Archived | EventKind::Deleted) {
        return;
    }
    for running in previews.take_for_thread(&event.thread_id) {
        drop(shut_down_later(running, cx.background_executor()));
    }
}

pub(crate) fn new_id() -> PreviewId {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    PreviewId(format!("p-{}", &hex[..6]))
}

pub(crate) fn folder(data_dir: &Path, id: &PreviewId) -> PathBuf {
    kg_core::previews_dir(data_dir).join(&id.0)
}

// A folder this code made: `p-` and six hex digits.
fn is_preview_name(name: &str) -> bool {
    name.strip_prefix("p-")
        .is_some_and(|hex| hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

// The `kg` this app runs. Tests have no kg binary of their own, so they get a path that does not
// exist: a spawn fails at once instead of running the test binary as a kagan.
pub(crate) fn exe() -> io::Result<PathBuf> {
    #[cfg(test)]
    {
        Ok(PathBuf::from("/nonexistent/kagan-test-kg"))
    }
    #[cfg(not(test))]
    {
        std::env::current_exe()
    }
}

#[cfg(unix)]
pub(crate) fn socket_for(folder: &Path, _id: &PreviewId) -> Result<PathBuf> {
    let socket = folder.join("kg.sock");
    if socket.as_os_str().len() > SOCKET_PATH_MAX {
        return Err(OpError::Invalid(format!(
            "the preview folder {} makes a socket path longer than {SOCKET_PATH_MAX} bytes; move the kagan data folder somewhere shorter",
            folder.display()
        )));
    }
    Ok(socket)
}

#[cfg(windows)]
pub(crate) fn socket_for(_folder: &Path, id: &PreviewId) -> Result<PathBuf> {
    Ok(PathBuf::from(format!("kagan-preview-{id}")))
}

// A `kg` aimed at one preview. It never starts an app: a preview that is gone stays gone.
fn kg(exe: &Path, data_dir: &Path, socket: &Path) -> Command {
    let mut command = Command::new(exe);
    command
        .env("KG_DATA_DIR", data_dir)
        .env("KG_SOCKET", socket)
        .env("KG_NO_START", "1")
        .env_remove("KG_THREAD_ID")
        .env_remove("KG_PROJECT_ID")
        .env_remove("KG_ENVIRONMENT_ID");
    command
}

pub(crate) fn spawn_app(
    exe: &Path,
    folder: &Path,
    data_dir: &Path,
    socket: &Path,
) -> Result<Process> {
    let log = File::create(folder.join(APP_LOG))?;
    let mut command = kg(exe, data_dir, socket);
    command.arg("--offscreen");
    Ok(Process::spawn(
        command,
        Stdio::null(),
        Stdio::from(log.try_clone()?),
        Stdio::from(log),
    )?)
}

// Blocks until the app answers `kg open`. Gives up at once when the app has exited, and says what
// its log ends with, since the folder is removed after a failed start.
pub(crate) fn wait_ready(
    exe: &Path,
    data_dir: &Path,
    socket: &Path,
    folder: &Path,
    app: &mut Process,
) -> Result<()> {
    let started = Instant::now();
    loop {
        let answered = kg(exe, data_dir, socket)
            .arg("open")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if answered {
            return Ok(());
        }
        if app.child().try_status()?.is_some() {
            return Err(stopped_message(
                "the preview's kagan stopped before it answered",
                folder,
            ));
        }
        if started.elapsed() >= READY_WITHIN {
            return Err(stopped_message(
                &format!(
                    "the preview did not answer within {} seconds",
                    READY_WITHIN.as_secs()
                ),
                folder,
            ));
        }
        std::thread::sleep(POLL);
    }
}

pub(crate) fn spawn_watch(
    exe: &Path,
    root: &Path,
    folder: &Path,
    data_dir: &Path,
    socket: &Path,
) -> Result<Process> {
    let log = File::create(folder.join(WATCH_LOG))?;
    let mut command = kg(exe, data_dir, socket);
    command.args(["plugin", "dev"]).arg(root);
    Ok(Process::spawn(
        command,
        Stdio::null(),
        Stdio::null(),
        Stdio::from(log),
    )?)
}

// The error for a preview whose process stopped, with the end of its log, which says why.
pub(crate) fn stopped_message(what: &str, folder: &Path) -> OpError {
    let log = fs::read_to_string(folder.join(APP_LOG)).unwrap_or_default();
    let tail: Vec<&str> = log.lines().rev().take(LOG_TAIL_LINES).collect();
    let tail: Vec<&str> = tail.into_iter().rev().collect();
    OpError::Failed(format!("{what}; the end of its log: {}", tail.join(" | ")))
}

fn answers(exe: &Path, data_dir: &Path, socket: &Path) -> bool {
    kg(exe, data_dir, socket)
        .arg("status")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

// Asks the preview's kagan to quit, as its own window does. The reply is not needed: the wait
// after it says whether the app went.
fn ask_quit(exe: &Path, data_dir: &Path, socket: &Path) {
    let _ = kg(exe, data_dir, socket)
        .args(["window", "quit"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn wait_gone(exe: &Path, data_dir: &Path, socket: &Path, limit: Duration) -> bool {
    let started = Instant::now();
    while answers(exe, data_dir, socket) {
        if started.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(POLL);
    }
    true
}

// Ends everything a preview started. The app quits over its socket, so its own programs and
// terminals end, and the processes it still holds are killed after the wait. The folder goes last.
pub(crate) fn shut_down(mut running: Running, exe: &Path) -> io::Result<()> {
    let data_dir = running.info.data_dir.clone();
    let socket = running.info.socket.clone();
    let tree = kg_process::descendants(running.app.child().id());
    ask_quit(exe, &data_dir, &socket);
    // Stops at once when the app has exited, and kills the app's group when it has not.
    drop(running.app.wait_within(QUIT_WITHIN, || false));
    kg_process::kill_listed(&tree);
    let folder = running.folder.clone();
    drop(running);
    remove(&folder)
}

// Runs `shut_down` off the caller's thread. The receiver closes once the preview is gone.
pub(crate) fn shut_down_later(running: Running, executor: &BackgroundExecutor) -> Receiver<()> {
    let (done, ended) = async_channel::bounded::<()>(1);
    let exe = exe().unwrap_or_default();
    executor
        .spawn(async move {
            // A folder that stays is swept on the next start.
            drop(shut_down(running, &exe));
            drop(done);
        })
        .detach();
    ended
}

// Removes the folders of previews that are not kept. A folder whose app still answers gets a
// quit first, and is kept when the quit does not take.
pub(crate) fn sweep(exe: &Path, data_dir: &Path, keep: &[PreviewId]) -> io::Result<()> {
    let Ok(entries) = fs::read_dir(kg_core::previews_dir(data_dir)) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry?;
        let folder = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !is_preview_name(&name) || keep.iter().any(|id| id.0 == name) {
            continue;
        }
        if answers_from(exe, &folder, &PreviewId(name)) {
            continue;
        }
        fs::remove_dir_all(&folder)?;
    }
    Ok(())
}

// True when a leftover folder's app still answers after a quit, so the folder must stay.
fn answers_from(exe: &Path, folder: &Path, id: &PreviewId) -> bool {
    let Ok(socket) = socket_for(folder, id) else {
        return false;
    };
    let data_dir = folder.join("data");
    if !answers(exe, &data_dir, &socket) {
        return false;
    }
    ask_quit(exe, &data_dir, &socket);
    !wait_gone(exe, &data_dir, &socket, QUIT_WITHIN)
}

pub(crate) fn remove(folder: &Path) -> io::Result<()> {
    fs::remove_dir_all(folder)
}
