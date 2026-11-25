use std::collections::VecDeque;
#[cfg(unix)]
use std::fs::File;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::{Channel, TransportFrame};
use gpui_kit::BackgroundExecutor;
use kg_domain::TEXT_MAX;
use kg_process::Process;

use super::client::{AcpError, Launch};

pub(super) const EXIT_GRACE: Duration = Duration::from_secs(2);
const TAIL_LINES: usize = 20;
const LINE_MAX: usize = TEXT_MAX;
const TICK: Duration = Duration::from_millis(20);
// Idle threads cost 4 wakes a second, not 50, so the first chunk after an idle prompt can wait 250 ms (decision 279).
const IDLE_TICK: Duration = Duration::from_millis(250);
const BUSY: Duration = Duration::from_secs(1);
const READ_BACKLOG: usize = 64;

pub(super) struct Transport {
    pub(super) lines: std::sync::mpsc::Receiver<String>,
    pub(super) frames: futures::channel::mpsc::UnboundedSender<TransportFrame>,
    pub(super) channel: Channel,
}

pub(super) fn spawn(launch: &Launch) -> Result<Process, AcpError> {
    let mut command = std::process::Command::new(&launch.program);
    command.args(&launch.args).current_dir(&launch.cwd).envs(
        launch
            .env
            .iter()
            .map(|(key, value)| (key.as_os_str(), value.as_os_str())),
    );
    Process::spawn(command, Stdio::piped(), Stdio::piped(), Stdio::piped()).map_err(|source| {
        AcpError::Spawn {
            program: launch.program.display().to_string(),
            cwd: launch.cwd.display().to_string(),
            source,
        }
    })
}

fn remember(tail: &Arc<Mutex<VecDeque<String>>>, line: String) {
    let Ok(mut tail) = tail.lock() else {
        return;
    };
    if tail.len() == TAIL_LINES {
        tail.pop_front();
    }
    tail.push_back(line);
}

pub(super) fn tail_text(tail: &Arc<Mutex<VecDeque<String>>>) -> Option<String> {
    let tail = tail.lock().ok()?;
    (!tail.is_empty()).then(|| {
        format!(
            "the agent said: {}",
            tail.iter().cloned().collect::<Vec<_>>().join("\n")
        )
    })
}

pub(super) trait Output: Send {
    fn fill(&mut self, into: &mut [u8]) -> Option<usize>;
}

trait Input: Send {
    fn write_line(&mut self, line: &[u8]) -> bool;
}

#[cfg(unix)]
impl Output for std::io::BufReader<File> {
    fn fill(&mut self, into: &mut [u8]) -> Option<usize> {
        match std::io::Read::read(self, into) {
            Ok(0) | Err(_) => None,
            Ok(read) => Some(read),
        }
    }
}

#[cfg(unix)]
impl Input for File {
    fn write_line(&mut self, line: &[u8]) -> bool {
        use std::io::Write as _;
        self.write_all(line).is_ok() && self.flush().is_ok()
    }
}

#[cfg(windows)]
struct Read<R>(futures::io::BufReader<R>);

#[cfg(windows)]
impl<R> Output for Read<R>
where
    R: futures::AsyncRead + Unpin + Send,
{
    fn fill(&mut self, into: &mut [u8]) -> Option<usize> {
        use futures::AsyncReadExt as _;
        match futures::executor::block_on(self.0.read(into)) {
            Ok(0) | Err(_) => None,
            Ok(read) => Some(read),
        }
    }
}

#[cfg(windows)]
struct Write<W>(W);

#[cfg(windows)]
impl<W> Input for Write<W>
where
    W: futures::AsyncWrite + Unpin + Send,
{
    fn write_line(&mut self, line: &[u8]) -> bool {
        use futures::AsyncWriteExt as _;
        futures::executor::block_on(self.0.write_all(line)).is_ok()
    }
}

pub(super) fn carry(
    process: &mut Process,
    tail: &Arc<Mutex<VecDeque<String>>>,
) -> Result<Transport, AcpError> {
    let (input, output, errors) = pipes(process)?;
    let reader = Arc::clone(tail);
    worker("kg-acp-stderr", move || {
        read_lines(errors, |line| remember(&reader, line));
    })?;
    // Bounded, so an agent that floods its output waits for kagan instead of growing the queue.
    let (lines, read) = std::sync::mpsc::sync_channel(READ_BACKLOG);
    worker("kg-acp-read", move || {
        read_lines(output, |line| {
            let _ = lines.send(line);
        });
    })?;
    let (frames, inbound) = futures::channel::mpsc::unbounded::<TransportFrame>();
    let (outbound, written) = futures::channel::mpsc::unbounded::<TransportFrame>();
    worker("kg-acp-write", move || write_frames(input, written))?;
    Ok(Transport {
        lines: read,
        frames,
        channel: Channel::new(inbound, outbound),
    })
}

fn worker(name: &str, body: impl FnOnce() + Send + 'static) -> Result<(), AcpError> {
    std::thread::Builder::new()
        .name(name.into())
        .spawn(body)
        .map(|_| ())
        .map_err(AcpError::Pipes)
}

fn read_lines(output: impl Output, mut on_line: impl FnMut(String)) {
    let mut lines = Lines::new(output);
    while let Some(line) = lines.next() {
        on_line(line);
    }
}

pub(super) struct Lines<O> {
    output: O,
    ahead: Vec<u8>,
    noted: bool,
}

impl<O: Output> Lines<O> {
    fn new(output: O) -> Self {
        Self {
            output,
            ahead: Vec::new(),
            noted: false,
        }
    }

    // The rest of a too-long line is read and dropped, so a sender that never ends a line cannot grow kagan's memory.
    fn next(&mut self) -> Option<String> {
        let mut line = Vec::new();
        loop {
            if self.ahead.is_empty() {
                let mut chunk = [0; 512];
                let Some(read) = self.output.fill(&mut chunk) else {
                    return (!line.is_empty()).then(|| text(&line));
                };
                self.ahead.extend_from_slice(&chunk[..read]);
            }
            let end = self.ahead.iter().position(|&byte| byte == b'\n');
            let taken = end.map_or(self.ahead.len(), |at| at + 1);
            let room = LINE_MAX.saturating_sub(line.len());
            let kept = taken.min(room);
            if kept < taken && !self.noted {
                self.noted = true;
                eprintln!(
                    "kg: the agent sent more than {} KiB on one line, so kagan cut it",
                    LINE_MAX / 1024
                );
            }
            line.extend_from_slice(&self.ahead[..kept]);
            self.ahead.drain(..taken);
            if end.is_some() {
                return Some(text(&line));
            }
        }
    }
}

fn text(line: &[u8]) -> String {
    String::from_utf8_lossy(line)
        .trim_end_matches(['\n', '\r'])
        .to_owned()
}

fn write_frames(
    mut input: impl Input,
    mut written: futures::channel::mpsc::UnboundedReceiver<TransportFrame>,
) {
    use futures::StreamExt as _;
    while let Some(frame) = futures::executor::block_on(written.next()) {
        let Ok(mut line) = frame.to_json() else {
            continue;
        };
        line.push('\n');
        if !input.write_line(line.as_bytes()) {
            return;
        }
    }
}

#[cfg(unix)]
fn pipes(
    process: &mut Process,
) -> Result<
    (
        impl Input + 'static,
        impl Output + 'static,
        impl Output + 'static,
    ),
    AcpError,
> {
    let child = process.child();
    let (Some(input), Some(output), Some(errors)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err(AcpError::NoPipes);
    };
    Ok((
        pipe(input)?,
        std::io::BufReader::new(pipe(output)?),
        std::io::BufReader::new(pipe(errors)?),
    ))
}

#[cfg(unix)]
fn pipe<P>(from: P) -> Result<File, AcpError>
where
    P: TryInto<std::os::fd::OwnedFd>,
    P::Error: std::fmt::Display,
{
    let fd = from
        .try_into()
        .map_err(|error: P::Error| AcpError::Pipes(std::io::Error::other(error.to_string())))?;
    blocking(fd)
}

#[cfg(unix)]
fn blocking(fd: std::os::fd::OwnedFd) -> Result<File, AcpError> {
    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
    let flags = fcntl_getfl(&fd).map_err(|error| AcpError::Pipes(error.into()))?;
    fcntl_setfl(&fd, flags & !OFlags::NONBLOCK).map_err(|error| AcpError::Pipes(error.into()))?;
    Ok(File::from(fd))
}

#[cfg(windows)]
fn pipes(
    process: &mut Process,
) -> Result<
    (
        impl Input + 'static,
        impl Output + 'static,
        impl Output + 'static,
    ),
    AcpError,
> {
    let child = process.child();
    let (Some(input), Some(output), Some(errors)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err(AcpError::NoPipes);
    };
    Ok((
        Write(input),
        Read(futures::io::BufReader::new(output)),
        Read(futures::io::BufReader::new(errors)),
    ))
}

pub(super) async fn pump(
    lines: std::sync::mpsc::Receiver<String>,
    frames: futures::channel::mpsc::UnboundedSender<TransportFrame>,
    connection: impl Future<Output = Result<(), AcpError>>,
    executor: &BackgroundExecutor,
) -> Result<(), AcpError> {
    use std::sync::mpsc::TryRecvError;
    let mut connection = Box::pin(connection);
    let mut heard = Instant::now();
    loop {
        loop {
            match lines.try_recv() {
                Ok(line) => {
                    heard = Instant::now();
                    if frames
                        .unbounded_send(TransportFrame::parse_json(&line))
                        .is_err()
                    {
                        return Ok(());
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // The agent stopped writing. The connection still reads the frames queued
                    // ahead of that end, so it is polled until it has answered them.
                    frames.close_channel();
                    return match futures::future::select(
                        connection.as_mut(),
                        executor.timer(EXIT_GRACE),
                    )
                    .await
                    {
                        futures::future::Either::Left((result, _)) => result,
                        futures::future::Either::Right(_) => Ok(()),
                    };
                }
            }
        }
        let tick = if heard.elapsed() < BUSY {
            TICK
        } else {
            IDLE_TICK
        };
        if let futures::future::Either::Left((result, _)) =
            futures::future::select(connection.as_mut(), executor.timer(tick)).await
        {
            return result;
        }
    }
}

#[cfg(test)]
#[path = "transport_test.rs"]
mod tests;
