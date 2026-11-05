use std::io;
use std::process::{Command, Stdio};
use std::time::Duration;

use super::process::poll_child;

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
