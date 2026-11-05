use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::Process;

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
    let mut process = Process::spawn(long_command(), Stdio::null(), Stdio::null(), Stdio::null())?;
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

#[test]
fn dropping_the_process_kills_a_background_grandchild() -> std::io::Result<()> {
    let path = TempPath::new("drop-grandchild");
    let process = Process::spawn(
        tree_command(&path.0),
        Stdio::null(),
        Stdio::null(),
        Stdio::null(),
    )?;
    let pid = read_pid(&path.0)?;
    let _reap = Reap(pid);
    drop(process);
    let deadline = Instant::now() + Duration::from_secs(2);
    while alive(pid) {
        if Instant::now() >= deadline {
            return Err(std::io::Error::other(format!(
                "grandchild {pid} survived drop"
            )));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn capture_closes_descendant_output_after_the_parent_exits() -> std::io::Result<()> {
    let path = TempPath::new("capture-grandchild");
    let mut command = std::process::Command::new("sh");
    command.args([
        "-c",
        "sleep 3 & echo $! > \"$1\"; printf out; printf err >&2; exit 7",
        "sh",
    ]);
    command.arg(&path.0);
    let started = Instant::now();
    let output = crate::capture_with_limit(command, Duration::from_secs(1))?;
    let pid = read_pid(&path.0)?;
    let _reap = Reap(pid);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(output.code, 7);
    assert_eq!(output.stdout, b"out");
    assert_eq!(output.stderr, "err");
    let deadline = Instant::now() + Duration::from_secs(2);
    while alive(pid) {
        assert!(Instant::now() < deadline, "captured descendant survived");
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

#[test]
fn capture_timeout_kills_the_entire_tree() -> std::io::Result<()> {
    let path = TempPath::new("capture-timeout");
    let error = crate::capture_with_limit(tree_command(&path.0), Duration::from_secs(1))
        .expect_err("the tree waits for its child");
    let pid = read_pid(&path.0)?;
    let _reap = Reap(pid);
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    let deadline = Instant::now() + Duration::from_secs(2);
    while alive(pid) {
        assert!(Instant::now() < deadline, "timed out descendant survived");
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}
