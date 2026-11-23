use std::ffi::OsString;
use std::fs::File;
use std::future::Future;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::v1::schema::{
    CancelNotification, InitializeRequest, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SessionNotification,
};
use agent_client_protocol::v1::{SessionMessage, SessionSetup};
use agent_client_protocol::{Agent, Channel, Client, Dispatch, TransportFrame};
use gpui_kit::BackgroundExecutor;
use kg_domain::TurnOutcome;
use kg_process::Process;

use super::map;

/// How long the agent is given to exit on its own once its output has ended.
const EXIT_GRACE: Duration = Duration::from_secs(2);
/// How long to wait between two looks at the agent's exit.
const EXIT_POLL: Duration = Duration::from_millis(20);
/// The last lines the agent wrote to stderr, kept for a failure's worth of explanation.
const TAIL_LINES: usize = 20;
/// How often the connection looks at what the agent's reader thread has, and the thread at what
/// the connection said. A thread that reads a pipe must not wake a task on GPUI's executor: the
/// test scheduler refuses a wake from another thread, and the watcher has always polled.
const TICK: Duration = Duration::from_millis(20);

/// What the connection reports to the thread, in the order it happened.
#[derive(Debug)]
pub(super) enum AcpEvent {
    /// The agent named its session. It is the thread's conversation.
    Session(String),
    Message {
        text: String,
        message_id: Option<String>,
    },
    /// The agent answered the prompt: its turn is over.
    TurnDone {
        outcome: TurnOutcome,
        error: Option<String>,
    },
    /// The agent's connection is over. `code` is set when it exited on its own.
    Ended {
        code: Option<u32>,
        error: Option<String>,
    },
}

/// What the thread asks the connection to do.
#[derive(Debug)]
pub(super) enum AcpCommand {
    Prompt(String),
    Cancel,
}

pub(super) struct Launch {
    pub(super) program: PathBuf,
    pub(super) args: Vec<OsString>,
    pub(super) cwd: PathBuf,
    pub(super) env: Vec<(OsString, OsString)>,
}

/// Runs one agent for the life of its thread: spawn it, talk ACP, and report how it ended.
pub(super) async fn run(
    launch: Launch,
    folder: PathBuf,
    executor: BackgroundExecutor,
    events: async_channel::Sender<AcpEvent>,
    commands: async_channel::Receiver<AcpCommand>,
) {
    let mut process = match spawn(&launch) {
        Ok(process) => process,
        Err(error) => {
            send(
                &events,
                AcpEvent::Ended {
                    code: None,
                    error: Some(error),
                },
            )
            .await;
            return;
        }
    };
    let tail = Arc::new(Mutex::new(Vec::new()));
    let transport = match carry(&mut process, &tail) {
        Ok(transport) => transport,
        Err(error) => {
            send(
                &events,
                AcpEvent::Ended {
                    code: None,
                    error: Some(error),
                },
            )
            .await;
            return;
        }
    };
    let Transport {
        lines,
        frames,
        channel,
    } = transport;
    let failure = pump(
        lines,
        frames,
        connect(channel, folder, &events, commands),
        &executor,
    )
    .await
    .err();
    let code = exit_status(&mut process, &executor).await;
    let error = failure.map(|error| match tail_text(&tail) {
        Some(tail) => format!("{error}\n{tail}"),
        None => error.to_string(),
    });
    send(&events, AcpEvent::Ended { code, error }).await;
}

async fn send(events: &async_channel::Sender<AcpEvent>, event: AcpEvent) {
    let _ = events.send(event).await;
}

fn spawn(launch: &Launch) -> Result<Process, String> {
    let mut command = std::process::Command::new(&launch.program);
    command.args(&launch.args).current_dir(&launch.cwd).envs(
        launch
            .env
            .iter()
            .map(|(key, value)| (key.as_os_str(), value.as_os_str())),
    );
    Process::spawn(command, Stdio::piped(), Stdio::piped(), Stdio::piped()).map_err(|error| {
        format!(
            "could not start {} in {}: {error}",
            launch.program.display(),
            launch.cwd.display()
        )
    })
}

fn remember(tail: &Arc<Mutex<Vec<String>>>, line: String) {
    let Ok(mut tail) = tail.lock() else {
        return;
    };
    tail.push(line);
    if tail.len() > TAIL_LINES {
        tail.remove(0);
    }
}

fn tail_text(tail: &Arc<Mutex<Vec<String>>>) -> Option<String> {
    let tail = tail.lock().ok()?;
    (!tail.is_empty()).then(|| format!("the agent said: {}", tail.join("\n")))
}

/// What the agent's pipes are read and written through, and the channel the connection takes.
struct Transport {
    /// The lines the agent's reader thread has read, in order.
    lines: std::sync::mpsc::Receiver<String>,
    /// Where the pump turns a line into a frame for the connection.
    frames: futures::channel::mpsc::UnboundedSender<TransportFrame>,
    channel: Channel,
}

/// Carries the connection's frames through two std threads: one writes lines to the agent, one
/// reads its lines. The connection itself never touches a pipe, and neither thread wakes it: the
/// pump hands the frames over on the executor's timer.
#[cfg(unix)]
fn carry(process: &mut Process, tail: &Arc<Mutex<Vec<String>>>) -> Result<Transport, String> {
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::os::fd::OwnedFd;

    let child = process.child();
    let (Some(input), Some(output), Some(errors)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err("the agent has no pipes".into());
    };
    let errors = blocking(OwnedFd::try_from(errors).map_err(|error| error.to_string())?)?;
    let reader = Arc::clone(tail);
    std::thread::Builder::new()
        .name("kg-acp-stderr".into())
        .spawn(move || {
            for line in BufReader::new(errors).lines().map_while(Result::ok) {
                remember(&reader, line);
            }
        })
        .map_err(|error| error.to_string())?;
    let (lines, read) = std::sync::mpsc::channel();
    let output = blocking(OwnedFd::try_from(output).map_err(|error| error.to_string())?)?;
    std::thread::Builder::new()
        .name("kg-acp-read".into())
        .spawn(move || {
            for line in BufReader::new(output).lines().map_while(Result::ok) {
                if lines.send(line).is_err() {
                    return;
                }
            }
        })
        .map_err(|error| error.to_string())?;
    let (frames, inbound) = futures::channel::mpsc::unbounded::<TransportFrame>();
    let (outbound, written) = futures::channel::mpsc::unbounded::<TransportFrame>();
    let stdin = blocking(OwnedFd::try_from(input).map_err(|error| error.to_string())?)?;
    std::thread::Builder::new()
        .name("kg-acp-write".into())
        .spawn(move || {
            use futures::StreamExt as _;
            let mut stdin = stdin;
            let mut written = written;
            while let Some(frame) = futures::executor::block_on(written.next()) {
                let Ok(mut line) = frame.to_json() else {
                    continue;
                };
                line.push('\n');
                if stdin.write_all(line.as_bytes()).is_err() || stdin.flush().is_err() {
                    return;
                }
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(Transport {
        lines: read,
        frames,
        channel: Channel::new(inbound, outbound),
    })
}

/// A pipe the agent's own process gave kagan. async-process makes it non-blocking for its own
/// reactor; these threads read and write it like any other file.
#[cfg(unix)]
fn blocking(fd: std::os::fd::OwnedFd) -> Result<File, String> {
    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
    let flags = fcntl_getfl(&fd).map_err(|error| error.to_string())?;
    fcntl_setfl(&fd, flags & !OFlags::NONBLOCK).map_err(|error| error.to_string())?;
    Ok(File::from(fd))
}

/// Windows pipes answer through async-process's blocking thread pool, so each thread parks on
/// its own read or write, and a line goes through a channel of its own.
#[cfg(windows)]
fn carry(process: &mut Process, tail: &Arc<Mutex<Vec<String>>>) -> Result<Transport, String> {
    use futures::{AsyncBufReadExt as _, AsyncWriteExt as _, StreamExt as _};

    let child = process.child();
    let (Some(input), Some(output), Some(errors)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err("the agent has no pipes".into());
    };
    let reader = Arc::clone(tail);
    std::thread::Builder::new()
        .name("kg-acp-stderr".into())
        .spawn(move || {
            futures::executor::block_on(async move {
                let mut errors = futures::io::BufReader::new(errors);
                let mut line = String::new();
                while errors.read_line(&mut line).await.unwrap_or_default() > 0 {
                    remember(&reader, std::mem::take(&mut line));
                }
            });
        })
        .map_err(|error| error.to_string())?;
    let (lines, read) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("kg-acp-read".into())
        .spawn(move || {
            futures::executor::block_on(async move {
                let mut output = futures::io::BufReader::new(output);
                let mut line = String::new();
                while output.read_line(&mut line).await.unwrap_or_default() > 0 {
                    if lines.send(std::mem::take(&mut line)).is_err() {
                        return;
                    }
                }
            });
        })
        .map_err(|error| error.to_string())?;
    let (frames, inbound) = futures::channel::mpsc::unbounded::<TransportFrame>();
    let (outbound, written) = futures::channel::mpsc::unbounded::<TransportFrame>();
    std::thread::Builder::new()
        .name("kg-acp-write".into())
        .spawn(move || {
            futures::executor::block_on(async move {
                let mut input = input;
                let mut written = written;
                while let Some(frame) = written.next().await {
                    let Ok(mut line) = frame.to_json() else {
                        continue;
                    };
                    line.push('\n');
                    if input.write_all(line.as_bytes()).await.is_err() {
                        return;
                    }
                }
            });
        })
        .map_err(|error| error.to_string())?;
    Ok(Transport {
        lines: read,
        frames,
        channel: Channel::new(inbound, outbound),
    })
}

/// Hands the reader thread's lines to the connection every tick, and lets the connection run in
/// between. Nothing here wakes a task from another thread.
async fn pump(
    lines: std::sync::mpsc::Receiver<String>,
    frames: futures::channel::mpsc::UnboundedSender<TransportFrame>,
    connection: impl Future<Output = Result<(), agent_client_protocol::Error>>,
    executor: &BackgroundExecutor,
) -> Result<(), agent_client_protocol::Error> {
    use std::sync::mpsc::TryRecvError;
    let mut connection = Box::pin(connection);
    let mut closed_at = None;
    loop {
        let mut open = true;
        loop {
            match lines.try_recv() {
                Ok(line) => {
                    if frames
                        .unbounded_send(TransportFrame::parse_json(&line))
                        .is_err()
                    {
                        return Ok(());
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    open = false;
                    break;
                }
            }
        }
        // The agent stopped writing: the connection sees its input end, and is given a moment to
        // report what it made of the last lines.
        if !open {
            closed_at.get_or_insert_with(Instant::now);
        }
        if closed_at.is_some_and(|at| at.elapsed() > EXIT_GRACE) {
            return Ok(());
        }
        let tick = executor.timer(TICK);
        if let futures::future::Either::Left((result, _)) =
            futures::future::select(connection.as_mut(), tick).await
        {
            return result;
        }
    }
}

/// Talks ACP until the agent's output ends or the connection fails.
async fn connect(
    transport: Channel,
    folder: PathBuf,
    events: &async_channel::Sender<AcpEvent>,
    commands: async_channel::Receiver<AcpCommand>,
) -> Result<(), agent_client_protocol::Error> {
    Client
        .v1()
        .name("kagan")
        .connect_with(transport, async |cx| {
            cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;
            cx.build_session(&folder)
                .block_task()
                .run_until(async |SessionSetup { mut session, .. }| {
                    let id = session.session_id().to_string();
                    events
                        .send(AcpEvent::Session(id))
                        .await
                        .map_err(|_| agent_client_protocol::Error::internal_error())?;
                    serve(&mut session, &commands, events).await
                })
                .await
        })
        .await
}

/// Serves the thread's commands and the agent's messages until one side is gone: a reply chunk
/// becomes an event, and the prompt's response ends its turn.
async fn serve(
    session: &mut agent_client_protocol::v1::ActiveSession<'_, Agent>,
    commands: &async_channel::Receiver<AcpCommand>,
    events: &async_channel::Sender<AcpEvent>,
) -> Result<(), agent_client_protocol::Error> {
    loop {
        let step = {
            let update = Box::pin(session.read_update());
            match futures::future::select(update, Box::pin(commands.recv())).await {
                futures::future::Either::Left((update, _)) => Step::Update(update.map(Box::new)),
                futures::future::Either::Right((Ok(command), _)) => Step::Command(command),
                futures::future::Either::Right((Err(_), _)) => Step::Closed,
            }
        };
        match step {
            Step::Update(Ok(message)) => heard(*message, events).await,
            Step::Update(Err(error)) => return Err(error),
            Step::Command(AcpCommand::Prompt(text)) => session.send_prompt(text)?,
            Step::Command(AcpCommand::Cancel) => session.connection().send_notification_to(
                Agent,
                CancelNotification::new(session.session_id().clone()),
            )?,
            Step::Closed => return Ok(()),
        }
    }
}

/// One thing the agent said, as far as slice 1 reads it: an answer chunk, its turn's end, or a
/// permission request.
async fn heard(message: SessionMessage, events: &async_channel::Sender<AcpEvent>) {
    match message {
        SessionMessage::PromptResponse(response) => {
            let (outcome, error) = match response {
                Ok(response) => map::outcome(response.stop_reason),
                Err(error) => (TurnOutcome::Failed, Some(error.to_string())),
            };
            send(events, AcpEvent::TurnDone { outcome, error }).await;
        }
        SessionMessage::Message(dispatch) => answer(dispatch.into_raw(), events).await,
        _ => {}
    }
}

enum Step {
    Update(Result<Box<SessionMessage>, agent_client_protocol::Error>),
    Command(AcpCommand),
    Closed,
}

/// One message from the agent for this session: a reply chunk, or a permission request that
/// slice 1 answers with `cancelled`.
async fn answer(dispatch: Dispatch, events: &async_channel::Sender<AcpEvent>) {
    let dispatch = match dispatch.into_notification::<SessionNotification>() {
        Ok(Ok(notification)) => {
            if let Some(chunk) = map::chunk(&notification.update) {
                send(
                    events,
                    AcpEvent::Message {
                        text: chunk.text,
                        message_id: chunk.message_id,
                    },
                )
                .await;
            }
            return;
        }
        Ok(Err(dispatch)) => dispatch,
        Err(_) => return,
    };
    if let Ok(Ok((_request, responder))) = dispatch.into_request::<RequestPermissionRequest>() {
        let _ = responder.respond(RequestPermissionResponse::new(
            RequestPermissionOutcome::Cancelled,
        ));
    }
}

/// Waits a moment for the agent to exit on its own, then ends its tree and reports its code.
async fn exit_status(process: &mut Process, executor: &BackgroundExecutor) -> Option<u32> {
    let started = Instant::now();
    loop {
        match process.child().try_status() {
            Ok(Some(status)) => return status.code().and_then(|code| u32::try_from(code).ok()),
            Ok(None) if started.elapsed() < EXIT_GRACE => executor.timer(EXIT_POLL).await,
            Ok(None) => {
                process.kill();
                return None;
            }
            Err(_) => return None,
        }
    }
}
