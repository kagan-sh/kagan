use std::io;
use std::process::{Command, Stdio};
use std::time::Duration;

use super::process::poll_child;

/// Output of a command that ran to completion.
#[derive(Debug)]
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

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::time::Duration;

    use super::{capture, capture_with_limit};

    #[cfg(unix)]
    fn shell(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    #[cfg(windows)]
    fn shell(script: &str) -> Command {
        let mut command = Command::new("cmd");
        command.args(["/C", script]);
        command
    }

    #[cfg(unix)]
    fn slow() -> Command {
        shell("sleep 10")
    }

    #[cfg(windows)]
    fn slow() -> Command {
        shell("ping -n 10 127.0.0.1 >NUL")
    }

    #[test]
    fn capture_keeps_stdout_stderr_and_the_code() {
        #[cfg(unix)]
        let mut command = shell("printf out; printf err >&2; exit 3");
        #[cfg(windows)]
        let mut command = shell("echo out & echo err 1>&2 & exit /b 3");
        let output = capture(&mut command).unwrap();
        assert_eq!(output.code, 3);
        #[cfg(unix)]
        {
            assert_eq!(output.stdout, b"out");
            assert_eq!(output.stderr, "err");
        }
        #[cfg(windows)]
        {
            assert!(String::from_utf8_lossy(&output.stdout).contains("out"));
            assert!(output.stderr.contains("err"));
        }
    }

    #[test]
    fn capture_with_limit_returns_a_fast_command() {
        #[cfg(unix)]
        let mut command = shell("printf fast");
        #[cfg(windows)]
        let mut command = shell("echo fast");
        let output = capture_with_limit(&mut command, Duration::from_secs(5)).unwrap();
        assert_eq!(output.code, 0);
        assert!(String::from_utf8_lossy(&output.stdout).contains("fast"));
    }

    #[test]
    fn capture_with_limit_reports_a_timeout_and_kills_the_command() {
        let error =
            capture_with_limit(&mut slow(), Duration::from_millis(50)).expect_err("slow command");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    }
}
