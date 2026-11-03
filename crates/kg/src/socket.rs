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

/// `sockaddr_un.sun_path` holds the path and a trailing NUL. Linux gives the path 108 bytes;
/// macOS gives it 104.
#[cfg(unix)]
const SOCKET_PATH_MAX: usize = if cfg!(target_os = "linux") { 107 } else { 103 };

fn non_empty(key: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(key).filter(|value| !value.is_empty())
}

fn socket_id(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
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

    if let Some(socket) = non_empty("KG_SOCKET") {
        return Ok(socket);
    }
    if let Some(data) = non_empty("KG_DATA_DIR") {
        return Ok(socket_beside(&PathBuf::from(data)));
    }
    let dir = if let Some(runtime) = non_empty("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime).join("kagan")
    } else {
        let home = non_empty("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
        if cfg!(target_os = "macos") {
            home.join("Library/Application Support/kagan")
        } else {
            non_empty("XDG_DATA_HOME")
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
    if let Some(socket) = non_empty("KG_SOCKET") {
        return Ok(socket);
    }
    if let Some(data) = non_empty("KG_DATA_DIR") {
        return Ok(format!("kagan-{}", socket_id(&os_bytes(&data))).into());
    }
    let user = std::env::var("USERNAME").unwrap_or_default();
    Ok(format!("kagan-{user}").into())
}

#[cfg(windows)]
pub(crate) fn name(location: &std::ffi::OsStr) -> io::Result<Name<'static>> {
    use interprocess::local_socket::{GenericNamespaced, ToNsName};

    location.to_os_string().to_ns_name::<GenericNamespaced>()
}

pub(crate) fn bind(name: Name<'_>) -> io::Result<Option<Listener>> {
    if Stream::connect(name.borrow()).is_ok() {
        return Ok(None);
    }
    // Nobody answers, so a socket file still there was left by a crash.
    let options = ListenerOptions::new().name(name).try_overwrite(true);
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
    let (receive, mut send) = stream.split();
    let (lines, incoming) = mpsc::channel();
    let (present, gone) = async_channel::bounded::<()>(1);
    thread::spawn(move || {
        let _present = present;
        for line in BufReader::new(receive).lines() {
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

    pub(crate) fn connect_or_start() -> io::Result<Self> {
        let location = location()?;
        if let Ok(client) = Self::connect(name(&location)?) {
            return Ok(client);
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
        match Client::connect(name(location)?) {
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
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::os::windows::process::CommandExt::creation_flags(
            &mut command,
            DETACHED_PROCESS | CREATE_NO_WINDOW,
        );
    }
    command.spawn().map(drop)
}
