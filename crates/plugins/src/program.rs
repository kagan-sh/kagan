use std::io::{Read as _, Seek as _};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use futures::FutureExt as _;
use kg_domain::OpError;
use kg_process::Process;

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
) -> Result<Output, RunError> {
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
) -> Result<std::process::ExitStatus, RunError> {
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
