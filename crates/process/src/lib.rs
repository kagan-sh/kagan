//! An owned process tree, and the Windows job object that ends it.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use async_process::Child;

#[cfg(windows)]
pub use job::Job;

/// Owns a child and kills its entire tree on drop, including after the child exits.
pub struct Process {
    child: Child,
    killed: bool,
    #[cfg(windows)]
    job: Option<job::Job>,
}

impl Process {
    /// Starts `command` in its own process group or Windows Job Object.
    ///
    /// `stdin`, `stdout`, and `stderr` are the child's stdio. On Windows the child is created
    /// suspended and resumed only after it is in the job, so a grandchild cannot start outside it.
    ///
    /// # Errors
    /// If the child cannot start or its tree cannot be owned.
    pub fn spawn(
        mut command: std::process::Command,
        stdin: std::process::Stdio,
        stdout: std::process::Stdio,
        stderr: std::process::Stdio,
    ) -> io::Result<Self> {
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(
            &mut command,
            windows_sys::Win32::System::Threading::CREATE_NO_WINDOW
                | windows_sys::Win32::System::Threading::CREATE_SUSPENDED,
        );
        let child = async_process::Command::from(command)
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr)
            .kill_on_drop(true)
            .spawn()?;
        let process = Self {
            child,
            killed: false,
            #[cfg(windows)]
            job: None,
        };
        #[cfg(windows)]
        let mut process = process;
        #[cfg(windows)]
        {
            process.job = Some(job::Job::assign(process.child.id())?);
            job::resume(process.child.id())?;
        }
        Ok(process)
    }

    /// The owned child's pipes and exit status.
    pub fn child(&mut self) -> &mut Child {
        &mut self.child
    }

    /// Kills every process in the tree.
    pub fn kill(&mut self) {
        if std::mem::replace(&mut self.killed, true) {
            return;
        }
        #[cfg(unix)]
        if let Some(pid) = rustix::process::Pid::from_raw(self.child.id().cast_signed()) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        #[cfg(windows)]
        if let Some(job) = self.job.take() {
            job.terminate();
        }
        let _ = self.child.kill();
    }

    /// Waits up to `limit` for the child to exit, then kills the tree and returns `None`.
    ///
    /// # Errors
    /// When the child's status cannot be read.
    pub fn wait_within(&mut self, limit: Duration) -> io::Result<Option<ExitStatus>> {
        poll_child(
            self,
            |process| process.child.try_status(),
            Process::kill,
            limit,
        )
    }
}

/// Polls `try_wait` every 20 ms until the child exits or `limit` passes, then calls `kill`.
fn poll_child<T>(
    child: &mut T,
    mut try_wait: impl FnMut(&mut T) -> io::Result<Option<ExitStatus>>,
    mut kill: impl FnMut(&mut T),
    limit: Duration,
) -> io::Result<Option<ExitStatus>> {
    let started = Instant::now();
    loop {
        match try_wait(child)? {
            Some(status) => return Ok(Some(status)),
            None if started.elapsed() > limit => {
                kill(child);
                return Ok(None);
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Output of a command that ran to completion.
pub struct Captured {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// Runs `command` to completion and captures both pipes.
///
/// # Errors
/// When the program cannot start.
pub fn capture(command: &mut Command) -> io::Result<Captured> {
    let output = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    Ok(Captured {
        code: output.status.code().unwrap_or(1),
        stdout: output.stdout,
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

/// Runs `command` to completion and captures both pipes, killing it after `limit`.
///
/// # Errors
/// When the program cannot start, or runs past `limit` with [`io::ErrorKind::TimedOut`].
pub fn capture_with_limit(command: &mut Command, limit: Duration) -> io::Result<Captured> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .map(|mut pipe| std::thread::spawn(move || read_pipe(&mut pipe)));
    let stderr = child
        .stderr
        .take()
        .map(|mut pipe| std::thread::spawn(move || read_pipe(&mut pipe)));
    let status = poll_child(
        &mut child,
        std::process::Child::try_wait,
        |child| {
            let _ = child.kill();
            let _ = child.wait();
        },
        limit,
    )?;
    let Some(status) = status else {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "the program ran too long",
        ));
    };
    Ok(Captured {
        code: status.code().unwrap_or(1),
        stdout: join_pipe(stdout),
        stderr: String::from_utf8_lossy(&join_pipe(stderr))
            .trim()
            .to_owned(),
    })
}

fn read_pipe(pipe: &mut impl io::Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = pipe.read_to_end(&mut bytes);
    bytes
}

fn join_pipe(handle: Option<std::thread::JoinHandle<Vec<u8>>>) -> Vec<u8> {
    handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default()
}

/// The `PATH` of the user's login shell, which an app started from the desktop does not inherit,
/// else this process's own. Resolved once; the first call waits for the shell (at most
/// `LOGIN_SHELL_TIMEOUT`), so call [`warm_user_path`] at startup.
#[must_use]
pub fn user_path() -> Option<OsString> {
    USER_PATH
        .get_or_init(|| login_path().or_else(|| std::env::var_os("PATH")))
        .clone()
}

/// Resolves [`user_path`] on its own thread so no later caller waits on the login shell.
pub fn warm_user_path() {
    std::thread::spawn(|| {
        let _ = user_path();
    });
}

static USER_PATH: std::sync::OnceLock<Option<OsString>> = std::sync::OnceLock::new();

#[cfg(unix)]
const LOGIN_SHELL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

#[cfg(unix)]
fn login_path() -> Option<OsString> {
    use std::io::Read as _;
    use std::os::unix::ffi::OsStringExt as _;
    let shell = std::env::var_os("SHELL")?;
    let mut child = std::process::Command::new(shell)
        .args(["-l", "-c", "printf %s \"$PATH\""])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let status = poll_child(
        &mut child,
        std::process::Child::try_wait,
        |child| {
            let _ = child.kill();
            let _ = child.wait();
        },
        LOGIN_SHELL_TIMEOUT,
    )
    .ok()??;
    let mut path = Vec::new();
    child.stdout.take()?.read_to_end(&mut path).ok()?;
    (status.success() && !path.is_empty()).then(|| OsString::from_vec(path))
}

#[cfg(windows)]
fn login_path() -> Option<OsString> {
    None
}

#[must_use]
pub fn pathext() -> Option<OsString> {
    if cfg!(windows) {
        Some(std::env::var_os("PATHEXT").unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into()))
    } else {
        None
    }
}

/// Resolves `program` the way a shell would. With `pathext` (Windows), only files ending in one of
/// its extensions count, so `npx` finds `npx.cmd` and not the extensionless shell script beside it.
#[must_use]
pub fn find_program(
    program: &Path,
    path: Option<&OsStr>,
    pathext: Option<&OsStr>,
) -> Option<PathBuf> {
    let extensions: Vec<String> = pathext
        .map(|pathext| {
            pathext
                .to_string_lossy()
                .split(';')
                .filter(|extension| !extension.is_empty())
                .map(str::to_ascii_lowercase)
                .collect()
        })
        .unwrap_or_default();
    let candidates = |base: PathBuf| -> Vec<PathBuf> {
        if pathext.is_none() {
            return vec![base];
        }
        let has_extension = base.extension().is_some_and(|extension| {
            let extension = format!(".{}", extension.to_string_lossy()).to_ascii_lowercase();
            extensions.contains(&extension)
        });
        let mut with_extensions: Vec<PathBuf> = extensions
            .iter()
            .map(|extension| {
                let mut name = base.clone().into_os_string();
                name.push(extension);
                PathBuf::from(name)
            })
            .collect();
        if has_extension {
            with_extensions.insert(0, base);
        }
        with_extensions
    };

    if program.is_absolute() || program.components().count() > 1 {
        return candidates(program.to_path_buf())
            .into_iter()
            .find(|candidate| is_executable(candidate));
    }
    std::env::split_paths(path?)
        .flat_map(|dir| candidates(dir.join(program)))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(windows)]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(windows)]
mod job;

#[cfg(test)]
mod tests {
    use std::io::Read as _;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    use super::Process;

    #[test]
    fn spawn_keeps_the_stdio_the_caller_set() -> std::io::Result<()> {
        let path = TempPath::new("stdio");
        let mut process = Process::spawn(
            echo_hello(),
            Stdio::null(),
            std::fs::File::create(&path.0)?.into(),
            Stdio::null(),
        )?;
        let status = wait(&mut process)?;
        if !status.success() {
            return Err(std::io::Error::other(format!("echo exited {status}")));
        }
        let mut output = String::new();
        std::fs::File::open(&path.0)?.read_to_string(&mut output)?;
        if !output.contains("hello") {
            return Err(std::io::Error::other(format!("stdout file was {output:?}")));
        }
        Ok(())
    }

    #[test]
    fn killing_the_process_kills_a_background_grandchild() -> std::io::Result<()> {
        let path = TempPath::new("grandchild");
        let mut process = Process::spawn(
            tree_command(&path.0),
            Stdio::null(),
            Stdio::null(),
            Stdio::null(),
        )?;
        let pid = read_pid(&path.0)?;
        let _reap = Reap(pid);
        if !alive(pid) {
            return Err(std::io::Error::other("grandchild was not alive"));
        }
        process.kill();
        let deadline = Instant::now() + Duration::from_secs(2);
        while alive(pid) {
            if Instant::now() >= deadline {
                return Err(std::io::Error::other(format!(
                    "grandchild {pid} survived kill"
                )));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }

    #[test]
    fn waiting_past_the_limit_kills_the_child() -> std::io::Result<()> {
        let mut process =
            Process::spawn(long_command(), Stdio::null(), Stdio::null(), Stdio::null())?;
        assert!(process.wait_within(Duration::from_millis(50))?.is_none());
        assert!(!wait(&mut process)?.success());
        Ok(())
    }

    #[cfg(unix)]
    fn long_command() -> std::process::Command {
        let mut command = std::process::Command::new("sh");
        command.args(["-c", "sleep 30"]);
        command
    }

    #[cfg(windows)]
    fn long_command() -> std::process::Command {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "ping -n 30 127.0.0.1 >NUL"]);
        command
    }

    #[cfg(unix)]
    fn echo_hello() -> std::process::Command {
        let mut command = std::process::Command::new("sh");
        command.args(["-c", "printf hello"]);
        command
    }

    #[cfg(windows)]
    fn echo_hello() -> std::process::Command {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "echo hello"]);
        command
    }

    #[cfg(unix)]
    fn tree_command(pid_file: &Path) -> std::process::Command {
        let mut command = std::process::Command::new("sh");
        command.args(["-c", "sleep 300 & echo $! > \"$1\"; wait", "sh"]);
        command.arg(pid_file);
        command
    }

    // `WScript.Shell.Exec` inherits the parent's job. A second PowerShell does not
    // finish starting on a busy Windows runner before the pid file is due.
    #[cfg(windows)]
    fn tree_command(pid_file: &Path) -> std::process::Command {
        let script = pid_file.with_extension("js");
        std::fs::write(
            &script,
            "var child = new ActiveXObject('WScript.Shell').Exec('ping.exe -n 300 127.0.0.1');\
             var file = new ActiveXObject('Scripting.FileSystemObject').CreateTextFile(WScript.Arguments(0), true);\
             file.Write(child.ProcessID);\
             file.Close();\
             while (child.Status == 0) WScript.Sleep(200);\n",
        )
        .unwrap();
        let mut command = std::process::Command::new("cscript.exe");
        command.args(["//Nologo", "//E:jscript"]);
        command.arg(&script);
        command.arg(pid_file);
        command
    }

    fn wait(process: &mut Process) -> std::io::Result<std::process::ExitStatus> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = process.child().try_status()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::other("child did not exit"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn read_pid(path: &Path) -> std::io::Result<u32> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(text) = std::fs::read_to_string(path)
                && let Ok(pid) = text.trim().parse::<u32>()
                && pid > 0
            {
                return Ok(pid);
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::other("grandchild did not report its pid"));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn alive(pid: u32) -> bool {
        #[cfg(unix)]
        {
            std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        }
        #[cfg(windows)]
        {
            std::process::Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}"), "/NH"])
                .output()
                .is_ok_and(|output| {
                    String::from_utf8_lossy(&output.stdout)
                        .split_whitespace()
                        .any(|word| word == pid.to_string())
                })
        }
    }

    struct TempPath(PathBuf);

    impl TempPath {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "kg-process-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |duration| duration.as_nanos())
            ));
            Self(path)
        }
    }

    impl Drop for TempPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
            let _ = std::fs::remove_file(self.0.with_extension("js"));
        }
    }

    struct Reap(u32);

    impl Drop for Reap {
        fn drop(&mut self) {
            if !alive(self.0) {
                return;
            }
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("kill")
                    .args(["-9", &self.0.to_string()])
                    .status();
            }
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/PID", &self.0.to_string()])
                    .status();
            }
        }
    }
}
