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
use kg_domain::{EventKind, OpError, PluginPreview, PreviewId};
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
    pub(crate) fn app_exited(&mut self) -> bool {
        self.app.child().try_status().ok().flatten().is_some()
    }

    pub(crate) fn watch_exited(&mut self) -> bool {
        self.watch.child().try_status().ok().flatten().is_some()
    }

    pub(crate) fn new(info: PluginPreview, folder: PathBuf, app: Process, watch: Process) -> Self {
        Self {
            info,
            folder,
            app,
            watch,
        }
    }
}

// The previews this app runs, the ids a start has reserved before its folder exists, and the
// `kg` the previews run as.
pub(crate) struct Previews {
    running: BTreeMap<PreviewId, Running>,
    starting: BTreeSet<PreviewId>,
    pub(crate) exe: PathBuf,
    _events: Subscription,
    _quit: Subscription,
}

impl Previews {
    pub(crate) fn new(workspace: &Entity<Workspace>, cx: &mut Context<PluginHost>) -> Self {
        Self {
            running: BTreeMap::new(),
            starting: BTreeSet::new(),
            exe: std::env::current_exe().unwrap_or_default(),
            _events: cx.subscribe(workspace, |host, _, event: &WorkspaceEvent, cx| {
                end_on(&mut host.previews, event, cx);
            }),
            // The app is quitting and cannot wait, so each preview is asked to quit and its
            // watch ends at once. Each preview's own quit ends its programs.
            _quit: cx.on_app_quit(|host, _| {
                host.previews.quit_all();
                std::future::ready(())
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

    pub(crate) fn remove(&mut self, id: &PreviewId) -> Option<Running> {
        self.running.remove(id)
    }

    // Takes every preview the test returns true for. It may look at a preview's processes, so it
    // gets them mutably.
    pub(crate) fn take_where(
        &mut self,
        mut take: impl FnMut(&mut Running) -> bool,
    ) -> Vec<Running> {
        let ids: Vec<PreviewId> = self
            .running
            .iter_mut()
            .filter_map(|(id, running)| take(running).then(|| id.clone()))
            .collect();
        ids.iter()
            .filter_map(|id| self.running.remove(id))
            .collect()
    }

    // Puts a started preview in place of the one its thread had for this plugin, and returns that
    // one. Taking and inserting happen in one call, so no other start sees the gap.
    pub(crate) fn replace(&mut self, running: Running) -> Option<Running> {
        let thread = running.info.thread_id.clone();
        let plugin = running.info.plugin.clone();
        let old =
            self.take_where(|other| other.info.thread_id == thread && other.info.plugin == plugin);
        self.running.insert(running.info.id.clone(), running);
        old.into_iter().next()
    }

    // Ends every preview's watch at once and asks each app to quit, without waiting for it.
    pub(crate) fn quit_all(&mut self) {
        let exe = self.exe.clone();
        for running in self.running.values_mut() {
            running.watch.kill();
            drop(ask_quit_detached(&exe, &running.info));
        }
    }
}

fn end_on(previews: &mut Previews, event: &WorkspaceEvent, cx: &mut Context<PluginHost>) {
    let WorkspaceEvent::Event { event, .. } = event else {
        return;
    };
    if !matches!(event.kind, EventKind::Archived | EventKind::Deleted) {
        return;
    }
    let exe = previews.exe.clone();
    let thread = event.thread_id.clone();
    for running in previews.take_where(|running| running.info.thread_id.as_ref() == Some(&thread)) {
        drop(shut_down_later(running, &exe, cx.background_executor()));
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

// The socket a preview listens on: a file in its folder on unix, and a pipe name on Windows.
#[cfg(unix)]
pub(crate) fn socket_for(folder: &Path) -> PathBuf {
    folder.join("kg.sock")
}

#[cfg(windows)]
pub(crate) fn socket_for(folder: &Path) -> PathBuf {
    let name = folder.file_name().unwrap_or_default().to_string_lossy();
    PathBuf::from(format!("kagan-preview-{name}"))
}

// A unix socket path has a length limit, so a long folder is refused before anyone is asked.
#[cfg(unix)]
pub(crate) fn check_socket_length(socket: &Path) -> Result<()> {
    use kg_domain::SOCKET_PATH_MAX;
    if socket.as_os_str().len() > SOCKET_PATH_MAX {
        return Err(OpError::Invalid(format!(
            "the preview folder {} makes a socket path longer than {SOCKET_PATH_MAX} bytes; move the kagan data folder somewhere shorter",
            socket.parent().unwrap_or(socket).display()
        )));
    }
    Ok(())
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

// Asks a preview's kagan to quit over its socket. The reply is not waited for.
fn ask_quit_detached(exe: &Path, info: &PluginPreview) -> io::Result<std::process::Child> {
    kg(exe, &info.data_dir, &info.socket)
        .args(["window", "quit"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

// Ends everything a preview started. The watch dies first, since it has nothing to save. The app
// is asked to quit over its socket, so its own programs end, and the app is killed when it is
// still running after the wait. The folder goes last.
pub(crate) fn shut_down(mut running: Running, exe: &Path) -> io::Result<()> {
    running.watch.kill();
    let data_dir = running.info.data_dir.clone();
    let socket = running.info.socket.clone();
    drop(
        kg(exe, &data_dir, &socket)
            .args(["window", "quit"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    );
    drop(running.app.wait_within(QUIT_WITHIN, || false));
    let folder = running.folder.clone();
    drop(running);
    remove(&folder)
}

// Runs `shut_down` off the caller's thread. The receiver closes once the preview is gone.
pub(crate) fn shut_down_later(
    running: Running,
    exe: &Path,
    executor: &BackgroundExecutor,
) -> Receiver<()> {
    let (done, ended) = async_channel::bounded::<()>(1);
    let exe = exe.to_path_buf();
    executor
        .spawn(async move {
            // A folder that stays is swept on the next start.
            drop(shut_down(running, &exe));
            drop(done);
        })
        .detach();
    ended
}

// Removes the folders of previews that are not kept. A folder whose app still answers is kept:
// it belongs to a kagan that is running, or that a person must end by hand.
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
        let answering = answers(exe, &folder.join("data"), &socket_for(&folder));
        if !answering {
            fs::remove_dir_all(&folder)?;
        }
    }
    Ok(())
}

pub(crate) fn remove(folder: &Path) -> io::Result<()> {
    fs::remove_dir_all(folder)
}
