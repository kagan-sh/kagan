//! The hidden kagan a plugin preview runs in: its folder, its socket, and the processes that run it.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use std::collections::BTreeMap;

use kg_domain::{OpError, PluginId, PluginPreview, PreviewId, SOCKET_PATH_MAX, ThreadId};
use kg_process::Process;

use crate::Result;

const READY_WITHIN: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(200);
pub(crate) const APP_LOG: &str = "app.log";
pub(crate) const WATCH_LOG: &str = "watch.log";

/// A running preview. Dropping it kills both process trees.
pub(crate) struct Running {
    pub(crate) info: PluginPreview,
    pub(crate) folder: PathBuf,
    _app: Process,
    _watch: Process,
}

impl Running {
    pub(crate) fn new(info: PluginPreview, folder: PathBuf, app: Process, watch: Process) -> Self {
        Self {
            info,
            folder,
            _app: app,
            _watch: watch,
        }
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

/// Removes the folders of previews that are not running, such as a crash left behind.
pub(crate) fn sweep(data_dir: &Path, running: &[PreviewId]) -> std::io::Result<()> {
    let Ok(entries) = fs::read_dir(kg_core::previews_dir(data_dir)) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if is_preview_name(name) && !running.iter().any(|id| id.0 == name) {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

pub(crate) fn remove(folder: &Path) -> std::io::Result<()> {
    fs::remove_dir_all(folder)
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

// The environment every `kg` the preview runs gets: its own folder and socket, and no thread.
fn kg(exe: &Path, data_dir: &Path, socket: &Path) -> Command {
    let mut command = Command::new(exe);
    command
        .env("KG_DATA_DIR", data_dir)
        .env("KG_SOCKET", socket)
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

/// Blocks until the preview's socket answers `kg open`, or the time runs out.
pub(crate) fn wait_ready(exe: &Path, data_dir: &Path, socket: &Path, folder: &Path) -> Result<()> {
    let started = Instant::now();
    loop {
        let answered = kg(exe, data_dir, socket)
            .arg("open")
            .env("KG_NO_START", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if answered {
            return Ok(());
        }
        if started.elapsed() >= READY_WITHIN {
            return Err(OpError::Failed(format!(
                "the preview did not answer within {} seconds; its log is {}",
                READY_WITHIN.as_secs(),
                folder.join(APP_LOG).display()
            )));
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
    command
        .args(["plugin", "dev"])
        .arg(root)
        .env("KG_NO_START", "1");
    Ok(Process::spawn(
        command,
        Stdio::null(),
        Stdio::null(),
        Stdio::from(log),
    )?)
}

/// Takes the preview a thread started for this plugin, if it has one.
pub(crate) fn take_for(
    previews: &mut BTreeMap<PreviewId, Running>,
    thread_id: Option<&ThreadId>,
    plugin: &PluginId,
) -> Option<Running> {
    let id = previews
        .values()
        .find(|running| {
            running.info.thread_id.as_ref() == thread_id && running.info.plugin == *plugin
        })
        .map(|running| running.info.id.clone())?;
    previews.remove(&id)
}

/// Takes every preview a thread started.
pub(crate) fn take_for_thread(
    previews: &mut BTreeMap<PreviewId, Running>,
    thread_id: &ThreadId,
) -> Vec<Running> {
    let ids: Vec<PreviewId> = previews
        .values()
        .filter(|running| running.info.thread_id.as_ref() == Some(thread_id))
        .map(|running| running.info.id.clone())
        .collect();
    ids.iter().filter_map(|id| previews.remove(id)).collect()
}
