use std::ffi::OsStr;
use std::io::{Read as _, Seek as _};
use std::net::{TcpStream, ToSocketAddrs as _};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use futures::FutureExt as _;
use serde::Deserialize;

use kg_domain::{GIT_SOURCE, OpError, PluginId};
use kg_process::Process;
use tempfile::TempDir;

use crate::index::public;
use crate::manifest::inside;
use crate::{Discovered, Result, discover};

const STAGING: &str = ".staging-";

/// Where plugin code comes from: `git:<url>@<ref>`, else a local folder.
pub(crate) enum Source {
    Git { url: String, reference: String },
    Local(PathBuf),
}

impl Source {
    pub(crate) fn parse(text: &str) -> Result<Self> {
        match text.strip_prefix(GIT_SOURCE) {
            Some(rest) => {
                if rest.chars().any(char::is_control) {
                    return Err(OpError::Invalid(
                        "a git address cannot hold line breaks or control characters".into(),
                    ));
                }
                let shown = public(rest);
                let needs_ref = || OpError::Invalid(format!("\"git:{shown}\" needs a ref @<ref>"));
                let (url, reference) = rest.rsplit_once('@').ok_or_else(needs_ref)?;
                // A ref cannot hold ':', so `git@host:owner/repo` alone is a url without one.
                if reference.contains(':') {
                    return Err(needs_ref());
                }
                if url.is_empty() || reference.is_empty() {
                    return Err(OpError::Invalid(format!(
                        "\"git:{shown}\" needs a url and a ref: git:<url>@<ref>"
                    )));
                }
                if reference.starts_with('-') {
                    return Err(OpError::Invalid(format!(
                        "\"git:{shown}\" has a ref that starts with -"
                    )));
                }
                Ok(Self::Git {
                    url: url.to_owned(),
                    reference: reference.to_owned(),
                })
            }
            None => Ok(Self::Local(PathBuf::from(text))),
        }
    }
}

/// Plugin code that was read but not yet kept. A git checkout waits in a staging
/// folder until the person approves it.
pub(crate) struct Fetched {
    pub(crate) discovered: Discovered,
    pub(crate) commit: Option<String>,
    /// For local sources the plugin folder itself; the source string records it.
    pub(crate) root: PathBuf,
    staging: Option<(TempDir, PathBuf)>,
}

/// Reads the plugin at `source`, in the `plugin` subfolder if given. Git sources are
/// cloned under `plugins` and locked to the commit the ref names now.
pub(crate) fn fetch(
    source: &Source,
    plugin: Option<&Path>,
    plugins: &Path,
    data_dir: &Path,
    stop: &async_channel::Receiver<()>,
) -> Result<Fetched> {
    match source {
        Source::Local(path) => {
            let subfolder = select(path, plugin)?;
            let root = dunce::canonicalize(path.join(&subfolder))?;
            Ok(Fetched {
                discovered: discover(&root, data_dir)?,
                commit: None,
                root,
                staging: None,
            })
        }
        Source::Git { url, reference } => {
            std::fs::create_dir_all(plugins)?;
            let staging = tempfile::Builder::new()
                .prefix(STAGING)
                .tempdir_in(plugins)?;
            let checkout = staging.path();
            let commit = clone(url, reference, checkout, stop)?;
            git(
                Some(checkout),
                ["checkout", "--quiet", "--detach", commit.as_str()].map(OsStr::new),
                stop,
            )?;
            let subfolder = select(checkout, plugin)?;
            let root = checkout.join(&subfolder);
            Ok(Fetched {
                discovered: discover(&root, data_dir)?,
                commit: Some(commit),
                root,
                staging: Some((staging, subfolder)),
            })
        }
    }
}

/// Clones `url` into `checkout` without checking out, and returns the commit `reference` names.
pub(crate) fn clone(
    url: &str,
    reference: &str,
    checkout: &Path,
    stop: &async_channel::Receiver<()>,
) -> Result<String> {
    let ssh = url.starts_with("ssh://")
        || (!url.contains("://")
            && url
                .split_once(':')
                .is_some_and(|(host, _)| host.len() > 1 && !host.contains('/')));
    let proxy = [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ]
    .iter()
    .any(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()))
        || git_config("http.proxy");
    if !ssh {
        check_host(url, Duration::from_secs(15), proxy)?;
    }
    let ssh_timeout =
        ssh && std::env::var_os("GIT_SSH_COMMAND").is_none() && !git_config("core.sshCommand");
    git_with_ssh(
        None,
        [
            OsStr::new("clone"),
            OsStr::new("--quiet"),
            OsStr::new("--no-checkout"),
            OsStr::new("--"),
            OsStr::new(url),
            checkout.as_os_str(),
        ],
        stop,
        ssh_timeout,
    )?;
    for name in [
        format!("{reference}^{{commit}}"),
        format!("origin/{reference}^{{commit}}"),
    ] {
        let output = git_command(
            Some(checkout),
            ["rev-parse", "--verify", "--quiet", name.as_str()].map(OsStr::new),
            stop,
        )?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned());
        }
    }
    Err(OpError::Invalid(format!(
        "{} has no ref {reference}",
        public(url)
    )))
}

fn check_host(source: &str, timeout: Duration, proxy: bool) -> Result<()> {
    if proxy {
        return Ok(());
    }
    // A source that is not a URL, such as an SSH address, has no host to reach here.
    let parsed = url::Url::parse(source).ok();
    let target = match parsed.as_ref() {
        Some(url) if url.scheme() == "https" => {
            url.host_str().map(|host| (host, url.port().unwrap_or(443)))
        }
        _ => None,
    };
    let Some((host, port)) = target else {
        return Ok(());
    };
    let reached = (host, port).to_socket_addrs().is_ok_and(|addresses| {
        let deadline = std::time::Instant::now() + timeout;
        addresses.into_iter().any(|address| {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            !remaining.is_zero() && TcpStream::connect_timeout(&address, remaining).is_ok()
        })
    });
    if reached {
        Ok(())
    } else {
        Err(OpError::Failed(format!(
            "could not reach {host}; check the address and your network"
        )))
    }
}

fn git_config(key: &str) -> bool {
    Command::new("git")
        .args(["config", "--get", key])
        .output()
        .is_ok_and(|output| {
            output.status.success() && !output.stdout.iter().all(u8::is_ascii_whitespace)
        })
}

impl Fetched {
    /// Keeps approved code: a git checkout moves to `<plugins>/<id>/<commit>`.
    /// Returns the plugin root and what was discovered there.
    pub(crate) fn keep(self, plugins: &Path) -> Result<(PathBuf, Discovered)> {
        let Self {
            discovered,
            commit,
            root,
            staging,
        } = self;
        let (Some((staging, subfolder)), Some(commit)) = (staging, commit) else {
            return Ok((root, discovered));
        };
        let id = &discovered.manifest.id;
        let kept = checkout(plugins, id, &commit);
        if !kept.exists() {
            std::fs::create_dir_all(plugins.join(&id.0))?;
            std::fs::rename(staging.keep(), &kept)?;
        }
        Ok((kept.join(subfolder), discovered))
    }
}

pub(crate) fn clear_staging(plugins: &Path) {
    let Ok(entries) = std::fs::read_dir(plugins) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(STAGING) {
            // Staging that cannot be removed now is cleared at the next start.
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

pub(crate) fn checkout(plugins: &Path, id: &PluginId, commit: &str) -> PathBuf {
    plugins.join(&id.0).join(commit)
}

pub(crate) fn git<'a>(
    cwd: Option<&Path>,
    args: impl IntoIterator<Item = &'a OsStr>,
    stop: &async_channel::Receiver<()>,
) -> Result<String> {
    git_with_ssh(cwd, args, stop, false)
}

fn git_with_ssh<'a>(
    cwd: Option<&Path>,
    args: impl IntoIterator<Item = &'a OsStr>,
    stop: &async_channel::Receiver<()>,
    ssh_timeout: bool,
) -> Result<String> {
    let output = git_command_inner(cwd, args, stop, ssh_timeout)?;
    if !output.status.success() {
        return Err(OpError::Failed(format!(
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git_command<'a>(
    cwd: Option<&Path>,
    args: impl IntoIterator<Item = &'a OsStr>,
    stop: &async_channel::Receiver<()>,
) -> Result<std::process::Output> {
    git_command_inner(cwd, args, stop, false)
}

fn git_command_inner<'a>(
    cwd: Option<&Path>,
    args: impl IntoIterator<Item = &'a OsStr>,
    stop: &async_channel::Receiver<()>,
    ssh_timeout: bool,
) -> Result<std::process::Output> {
    let mut command = Command::new("git");
    if ssh_timeout {
        command.env("GIT_SSH_COMMAND", "ssh -o ConnectTimeout=15");
    }
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ALLOW_PROTOCOL", "https:ssh:git:file");
    run_git(command, Duration::from_mins(2), stop)
}

pub(crate) fn run_git(
    command: Command,
    limit: Duration,
    stop: &async_channel::Receiver<()>,
) -> Result<std::process::Output> {
    run(command, limit, stop).map_err(|error| match error {
        RunError::Timeout => {
            OpError::Failed("git took longer than 2 minutes; check the host and try again".into())
        }
        RunError::Stopped => OpError::Failed("the install was stopped".into()),
        RunError::Other(error) => OpError::Failed(format!("cannot run git: {error}")),
    })
}

#[derive(Debug)]
pub(crate) enum RunError {
    Timeout,
    Stopped,
    Other(OpError),
}

impl From<std::io::Error> for RunError {
    fn from(error: std::io::Error) -> Self {
        Self::Other(error.into())
    }
}

pub(crate) fn run(
    command: Command,
    timeout: Duration,
    stop: &async_channel::Receiver<()>,
) -> std::result::Result<Output, RunError> {
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let mut process = Process::spawn(
        command,
        Stdio::null(),
        stdout.try_clone()?.into(),
        stderr.try_clone()?.into(),
    )?;
    let status = wait(&mut process, timeout, stop)?;
    let mut output = Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    stdout.rewind()?;
    stderr.rewind()?;
    stdout.read_to_end(&mut output.stdout)?;
    stderr.read_to_end(&mut output.stderr)?;
    Ok(output)
}

/// Waits for the program to end. Its time limit, or the stop being sent or dropped, ends the wait
/// first; the caller's `Process` then ends the program.
fn wait(
    process: &mut Process,
    timeout: Duration,
    stop: &async_channel::Receiver<()>,
) -> std::result::Result<std::process::ExitStatus, RunError> {
    let status = process.child().status().fuse();
    let timer = async_io::Timer::after(timeout).fuse();
    let stopped = stop.recv().fuse();
    async_io::block_on(async move {
        futures::pin_mut!(status, timer, stopped);
        futures::select_biased! {
            status = status => status.map_err(RunError::from),
            _ = timer => Err(RunError::Timeout),
            _ = stopped => Err(RunError::Stopped),
        }
    })
}

/// The shape `.kagan/plugins.json` and a plugin index share; `E` is one plugin.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Index<E> {
    #[serde(rename = "$schema")]
    _schema: Option<String>,
    pub(crate) schema_version: u32,
    pub(crate) name: String,
    pub(crate) plugins: Vec<E>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    name: String,
    source: PathBuf,
}

fn same_relative(selected: &Path, source: &Path) -> bool {
    fn parts(path: &Path) -> Option<Vec<&OsStr>> {
        let mut parts = Vec::new();
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::Normal(part) => parts.push(part),
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
            }
        }
        Some(parts)
    }
    match (parts(selected), parts(source)) {
        (Some(selected), Some(source)) => selected == source,
        _ => false,
    }
}

fn select(root: &Path, selected: Option<&Path>) -> Result<PathBuf> {
    let path = root.join(".kagan/plugins.json");
    let folder = match std::fs::read_to_string(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            selected.unwrap_or(Path::new(".")).to_owned()
        }
        Err(error) => return Err(error.into()),
        Ok(text) => {
            let index: Index<Entry> = serde_json::from_str(&text)
                .map_err(|error| OpError::Invalid(format!("{}: {error}", path.display())))?;
            if index.schema_version != 1 || index.name.trim().is_empty() || index.plugins.is_empty()
            {
                return Err(OpError::Invalid(format!(
                    "{} needs schemaVersion 1, a name and plugins",
                    path.display()
                )));
            }
            let mut names = std::collections::BTreeSet::new();
            for entry in &index.plugins {
                if entry.name.trim().is_empty()
                    || !names.insert(&entry.name)
                    || inside(root, &entry.source).is_none()
                {
                    return Err(OpError::Invalid(format!(
                        "{} has invalid or repeated plugin {}",
                        path.display(),
                        entry.name
                    )));
                }
            }
            let entry = match selected {
                Some(selected) => index.plugins.iter().find(|entry| {
                    selected == Path::new(&entry.name) || same_relative(selected, &entry.source)
                }),
                None if index.plugins.len() == 1 => index.plugins.first(),
                None => None,
            }
            .ok_or_else(|| {
                OpError::Invalid(format!(
                    "{}: choose --plugin {}",
                    path.display(),
                    index
                        .plugins
                        .iter()
                        .map(|entry| entry.name.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ")
                ))
            })?;
            entry.source.clone()
        }
    };
    let joined = inside(root, &folder).ok_or_else(|| {
        OpError::Invalid("plugin source must be a folder inside the repository".into())
    })?;
    let canonical = dunce::canonicalize(&joined)?;
    let root = dunce::canonicalize(root)?;
    if !canonical.starts_with(&root) {
        return Err(OpError::Invalid(
            "plugin source escapes the repository".into(),
        ));
    }
    canonical
        .strip_prefix(root)
        .map(Path::to_path_buf)
        .map_err(|error| OpError::Invalid(error.to_string()))
}
