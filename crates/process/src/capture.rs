use std::io;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Output of a command that ran to completion.
#[derive(Debug)]
pub struct Captured {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// Runs `command` in an owned process tree, killing it after `limit`.
///
/// # Errors
/// When the program cannot start, output cannot be read, or it runs past `limit`.
pub fn capture_with_limit(command: Command, limit: Duration) -> io::Result<Captured> {
    use io::{Read as _, Seek as _};

    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let mut process = crate::Process::spawn(
        command,
        Stdio::null(),
        stdout.try_clone()?.into(),
        stderr.try_clone()?.into(),
    )?;
    let status = process
        .wait_within(limit)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "the program ran too long"))?;
    process.kill();
    stdout.rewind()?;
    stderr.rewind()?;
    let mut out = Vec::new();
    stdout.read_to_end(&mut out)?;
    let mut bytes = Vec::new();
    stderr.read_to_end(&mut bytes)?;
    Ok(Captured {
        code: status.code().unwrap_or(1),
        stdout: out,
        stderr: String::from_utf8_lossy(&bytes).trim().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::time::Duration;

    use super::capture_with_limit;

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
        let command = shell("printf out; printf err >&2; exit 3");
        #[cfg(windows)]
        let command = shell("echo out & echo err 1>&2 & exit /b 3");
        let output = capture_with_limit(command, Duration::from_secs(5)).unwrap();
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
        let command = shell("printf fast");
        #[cfg(windows)]
        let command = shell("echo fast");
        let output = capture_with_limit(command, Duration::from_secs(5)).unwrap();
        assert_eq!(output.code, 0);
        assert!(String::from_utf8_lossy(&output.stdout).contains("fast"));
    }

    #[test]
    fn capture_with_limit_reports_a_timeout_and_kills_the_command() {
        let error =
            capture_with_limit(slow(), Duration::from_millis(50)).expect_err("slow command");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    }
}
