use std::{
    io::{self, BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

use async_channel::Receiver;

use interprocess::local_socket::{Listener, ListenerOptions, Name, Stream, prelude::*};
use kg_domain::{Op, OpError, OpResult};
use serde::Deserialize;

const START_TIMEOUT: Duration = Duration::from_secs(10);

// `sockaddr_un.sun_path` holds the path and a trailing NUL. Linux gives the path 108 bytes;
// macOS gives it 104.
#[cfg(unix)]
const SOCKET_PATH_MAX: usize = if cfg!(target_os = "linux") { 107 } else { 103 };

pub(crate) fn non_empty(value: Option<std::ffi::OsString>) -> Option<std::ffi::OsString> {
    value.filter(|value| !value.is_empty())
}

fn socket_id(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    hex::encode(Sha256::digest(bytes))
}

#[cfg(unix)]
fn os_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes().to_vec()
}

#[cfg(windows)]
fn os_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().flat_map(u16::to_le_bytes).collect()
}

/// Where the socket is: `KG_SOCKET` when set, else beside `KG_DATA_DIR` when that is set, else
/// the default place for this user.
#[cfg(unix)]
pub(crate) fn location() -> io::Result<std::ffi::OsString> {
    use std::path::PathBuf;

    if let Some(socket) = non_empty(std::env::var_os("KG_SOCKET")) {
        return Ok(socket);
    }
    if let Some(data) = non_empty(std::env::var_os("KG_DATA_DIR")) {
        let data = std::path::absolute(data)?;
        return Ok(socket_beside(&data));
    }
    let dir = if let Some(runtime) = non_empty(std::env::var_os("XDG_RUNTIME_DIR")) {
        PathBuf::from(runtime).join("kagan")
    } else {
        let home = non_empty(std::env::var_os("HOME"))
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
        if cfg!(target_os = "macos") {
            home.join("Library/Application Support/kagan")
        } else {
            non_empty(std::env::var_os("XDG_DATA_HOME"))
                .map_or_else(|| home.join(".local/share"), PathBuf::from)
                .join("kagan")
        }
    };
    location_in(&dir)
}

#[cfg(unix)]
fn socket_beside(dir: &std::path::Path) -> std::ffi::OsString {
    let direct = dir.join("kg.sock");
    if direct.as_os_str().len() <= SOCKET_PATH_MAX {
        return direct.into_os_string();
    }
    let id = socket_id(&os_bytes(dir.as_os_str()));
    format!("/tmp/kagan-{id}/kg.sock").into()
}

#[cfg(unix)]
pub(crate) fn ensure_directory(dir: &std::path::Path) -> io::Result<()> {
    use std::{
        fs,
        os::unix::fs::{DirBuilderExt, PermissionsExt},
    };

    if dir.is_dir() {
        return Ok(());
    }
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(unix)]
pub(crate) fn location_in(dir: &std::path::Path) -> io::Result<std::ffi::OsString> {
    use std::{fs, os::unix::fs::PermissionsExt};

    ensure_directory(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    Ok(dir.join("kg.sock").into_os_string())
}

#[cfg(unix)]
pub(crate) fn name(location: &std::ffi::OsStr) -> io::Result<Name<'static>> {
    use interprocess::local_socket::{GenericFilePath, ToFsName};

    std::path::Path::new(location)
        .to_path_buf()
        .to_fs_name::<GenericFilePath>()
}

#[cfg(windows)]
pub(crate) fn location() -> io::Result<std::ffi::OsString> {
    if let Some(socket) = non_empty(std::env::var_os("KG_SOCKET")) {
        return Ok(socket);
    }
    if let Some(data) = non_empty(std::env::var_os("KG_DATA_DIR")) {
        let data = std::path::absolute(data)?;
        return Ok(format!("kagan-{}", socket_id(&os_bytes(data.as_os_str()))).into());
    }
    let user = std::env::var("USERNAME").unwrap_or_default();
    Ok(format!("kagan-{user}").into())
}

#[cfg(windows)]
pub(crate) fn name(location: &std::ffi::OsStr) -> io::Result<Name<'static>> {
    use interprocess::local_socket::{GenericNamespaced, ToNsName};

    location.to_os_string().to_ns_name::<GenericNamespaced>()
}

/// Checks a `/tmp/kagan-<id>` folder before a client connects to it.
pub(crate) fn before_connect(location: &std::ffi::OsStr) -> io::Result<()> {
    #[cfg(unix)]
    trust_fallback(std::path::Path::new(location))?;
    #[cfg(not(unix))]
    let _ = location;
    Ok(())
}

/// Listens on `location`. A leftover socket file is removed only when it is a socket we own,
/// inside a folder we own. Anything else is left in place.
pub(crate) fn open(location: &std::ffi::OsStr) -> io::Result<Option<Listener>> {
    #[cfg(unix)]
    {
        open_unix(location)
    }
    #[cfg(not(unix))]
    bind(name(location)?)
}

#[cfg(unix)]
fn open_unix(location: &std::ffi::OsStr) -> io::Result<Option<Listener>> {
    let path = std::path::Path::new(location);
    if fallback_socket(path) {
        trust_fallback(path)?;
    } else if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        ensure_directory(parent)?;
    }
    let bound = name(location)?;
    if Stream::connect(bound.borrow()).is_ok() {
        return Ok(None);
    }
    reclaim_stale(path)?;
    bind(bound)
}

#[cfg(unix)]
fn fallback_socket(path: &std::path::Path) -> bool {
    path.file_name().is_some_and(|name| name == "kg.sock")
        && path.parent().is_some_and(fallback_dir)
}

#[cfg(unix)]
fn fallback_dir(dir: &std::path::Path) -> bool {
    let Some(name) = dir.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    dir.parent() == Some(std::path::Path::new("/tmp"))
        && name.starts_with("kagan-")
        && name.len() == "kagan-".len() + 64
}

#[cfg(unix)]
fn trust_fallback(path: &std::path::Path) -> io::Result<()> {
    if !fallback_socket(path) {
        return Ok(());
    }
    let Some(dir) = path.parent() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "socket has no folder",
        ));
    };
    match std::fs::symlink_metadata(dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => create_private(dir),
        Err(error) => Err(error),
        Ok(meta) => private_dir(&meta),
    }
}

#[cfg(unix)]
fn create_private(dir: &std::path::Path) -> io::Result<()> {
    use std::{
        fs,
        os::unix::fs::{DirBuilderExt, PermissionsExt},
    };

    let created = {
        let _umask = Umask::owner_only();
        fs::DirBuilder::new().mode(0o700).create(dir)
    };
    match created {
        Ok(()) => {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
            private_dir(&fs::symlink_metadata(dir)?)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            private_dir(&fs::symlink_metadata(dir)?)
        }
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn private_dir(metadata: &std::fs::Metadata) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != effective_uid()
        || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the socket folder is not private",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn reclaim_stale(path: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    let meta = match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
        Ok(meta) => meta,
    };
    let kind = meta.file_type();
    if kind.is_symlink() || !kind.is_socket() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "kg.sock is not a socket",
        ));
    }
    if meta.uid() != effective_uid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "kg.sock is owned by someone else",
        ));
    }
    // A bare `kg.sock` has an empty parent. The working directory is that folder.
    let dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let dir_meta = std::fs::symlink_metadata(dir)?;
    if dir_meta.file_type().is_symlink() || !dir_meta.is_dir() || dir_meta.uid() != effective_uid()
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the socket folder is not ours",
        ));
    }
    if fallback_dir(dir) {
        private_dir(&dir_meta)?;
    }
    std::fs::remove_file(path)
}

#[cfg(unix)]
fn effective_uid() -> u32 {
    // SAFETY: `geteuid` only reads this process's credential and cannot fail.
    unsafe { libc::geteuid() }
}

#[cfg(unix)]
struct Umask(libc::mode_t);

#[cfg(unix)]
impl Umask {
    fn owner_only() -> Self {
        // SAFETY: `umask` is process-wide. This sets and returns the previous mask, and Drop restores it before anything else is created.
        Self(unsafe { libc::umask(0o077) })
    }
}

#[cfg(unix)]
impl Drop for Umask {
    fn drop(&mut self) {
        // SAFETY: restores the mask that the matching constructor replaced.
        unsafe { libc::umask(self.0) };
    }
}

pub(crate) fn bind(name: Name<'_>) -> io::Result<Option<Listener>> {
    if Stream::connect(name.borrow()).is_ok() {
        return Ok(None);
    }
    let options = ListenerOptions::new().name(name).try_overwrite(false);
    #[cfg(windows)]
    let options = interprocess::os::windows::local_socket::ListenerOptionsExt::security_descriptor(
        options,
        crate::pipe::current_user_only()?,
    );
    options.create_sync().map(Some)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Reply {
    Failed(OpError),
    Done(Box<OpResult>),
}

/// `handle` gets each op and a channel that closes when its connection does.
pub(crate) fn serve(
    listener: Listener,
    handle: impl Fn(Op, &Receiver<()>) -> Result<OpResult, OpError> + Send + Sync + 'static,
) {
    let handle = Arc::new(handle);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let handle = handle.clone();
            match stream {
                Ok(stream) => {
                    thread::spawn(move || {
                        if let Err(error) = answer(stream, &*handle) {
                            eprintln!("kg: connection failed: {error}");
                        }
                    });
                }
                Err(error) => eprintln!("kg: accept failed: {error}"),
            }
        }
    });
}

// A reader thread sees the client leave while an op still waits for a person.
fn answer(
    stream: Stream,
    handle: &impl Fn(Op, &Receiver<()>) -> Result<OpResult, OpError>,
) -> io::Result<()> {
    let stream = Arc::new(stream);
    let receive = Arc::clone(&stream);
    let mut send = &*stream;
    let (lines, incoming) = mpsc::channel();
    let (present, gone) = async_channel::bounded::<()>(1);
    thread::spawn(move || {
        let _present = present;
        for line in BufReader::new(&*receive).lines() {
            if lines.send(line).is_err() {
                return;
            }
        }
    });
    for line in incoming {
        let reply = match serde_json::from_str::<Op>(&line?) {
            Ok(op) => handle(op, &gone),
            Err(error) => Err(OpError::Invalid(error.to_string())),
        };
        let mut out = match reply {
            Ok(result) => serde_json::to_vec(&result),
            Err(error) => serde_json::to_vec(&error),
        }?;
        out.push(b'\n');
        send.write_all(&out)?;
    }
    Ok(())
}

pub(crate) struct Client(BufReader<Stream>);

impl Client {
    pub(crate) fn connect(name: Name<'_>) -> io::Result<Self> {
        Stream::connect(name).map(|stream| Self(BufReader::new(stream)))
    }

    /// Connects to the app listening at `location`, after checking a fallback folder there.
    pub(crate) fn open(location: &std::ffi::OsStr) -> io::Result<Self> {
        before_connect(location)?;
        Self::connect(name(location)?)
    }

    pub(crate) fn connect_or_start() -> io::Result<Self> {
        let location = location()?;
        if let Ok(client) = Self::open(&location) {
            return Ok(client);
        }
        if non_empty(std::env::var_os("KG_NO_START")).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "no kagan app is listening on {}; start it first",
                    location.to_string_lossy()
                ),
            ));
        }
        start_headless(&location)?;
        wait_for_socket(&location, START_TIMEOUT)
    }

    pub(crate) fn call(&mut self, op: &Op) -> Result<OpResult, OpError> {
        let mut out = serde_json::to_vec(op)?;
        out.push(b'\n');
        self.0.get_mut().write_all(&out)?;
        let mut line = String::new();
        if self.0.read_line(&mut line)? == 0 {
            return Err(OpError::Failed("kagan closed the connection".into()));
        }
        match serde_json::from_str(&line)? {
            Reply::Done(result) => Ok(*result),
            Reply::Failed(error) => Err(error),
        }
    }
}

pub(crate) fn wait_for_socket(location: &std::ffi::OsStr, timeout: Duration) -> io::Result<Client> {
    let deadline = Instant::now() + timeout;
    loop {
        match Client::open(location) {
            Ok(client) => return Ok(client),
            Err(error) if Instant::now() > deadline => return Err(error),
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn start_headless(location: &std::ffi::OsStr) -> io::Result<()> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--headless")
        .env("KG_SOCKET", location)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, DETACHED_PROCESS};
        std::os::windows::process::CommandExt::creation_flags(
            &mut command,
            DETACHED_PROCESS | CREATE_NO_WINDOW,
        );
    }
    command.spawn().map(drop)
}
