use std::collections::{HashSet, VecDeque};
use std::ffi::OsString;
#[cfg(unix)]
use std::fs::File;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
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
use kg_domain::{TEXT_MAX, TurnOutcome};
use kg_process::Process;
use kg_terminal::Exit;

use super::map;

/// How long the agent is given to answer `initialize` and `session/new`.
const STARTUP: Duration = Duration::from_secs(30);
/// How long the agent is given to exit on its own once its output has ended.
const EXIT_GRACE: Duration = Duration::from_secs(2);
/// How long to wait between two looks at the agent's exit.
const EXIT_POLL: Duration = Duration::from_millis(20);
/// The last lines the agent wrote to stderr, kept for a failure's worth of explanation.
const TAIL_LINES: usize = 20;
/// The most of one line kagan keeps, the same cap a prompt has. An agent that sends more is cut.
const LINE_MAX: usize = TEXT_MAX;
/// How often the connection looks at what the agent's reader thread has while it is answering.
const TICK: Duration = Duration::from_millis(20);
/// How often it looks once the agent has said nothing for a while. Idle threads cost 4 wakes a
/// second, not 50; the cost is written in decision 279.
const IDLE_TICK: Duration = Duration::from_millis(250);
/// How long after the last line the connection keeps the short tick.
const BUSY: Duration = Duration::from_secs(1);
/// How many lines the reader thread may queue before it waits for the connection to take them.
const READ_BACKLOG: usize = 64;

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
    /// The agent's connection is over. `exit` is how its process ended when kagan saw it, and
    /// `error` is what failed when it did not.
    Ended {
        exit: Option<Exit>,
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
                    exit: None,
                    error: Some(error),
                },
            )
            .await;
            return;
        }
    };
    let tail = Arc::new(Mutex::new(VecDeque::new()));
    let transport = match carry(&mut process, &tail) {
        Ok(transport) => transport,
        Err(error) => {
            send(
                &events,
                AcpEvent::Ended {
                    exit: None,
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
    let connection = connect(channel, folder, &events, commands, &executor);
    let failure = pump(lines, frames, connection, &executor).await.err();
    let exit = exit_status(&mut process, &executor).await;
    // A clean exit needs no explanation; anything else gets the agent's own last words.
    let tail = tail_text(&tail).filter(|_| exit != Some(Exit::Code(0)));
    send(
        &events,
        AcpEvent::Ended {
            exit,
            error: join(failure, tail),
        },
    )
    .await;
}

/// What the agent's pipes are read and written through, and the channel the connection takes.
struct Transport {
    /// The lines the agent's reader thread has read, in order.
    lines: std::sync::mpsc::Receiver<String>,
    /// Where the pump turns a line into a frame for the connection.
    frames: futures::channel::mpsc::UnboundedSender<TransportFrame>,
    channel: Channel,
}

async fn send(events: &async_channel::Sender<AcpEvent>, event: AcpEvent) {
    let _ = events.send(event).await;
}

/// What kagan knows about the end in words: why the connection failed, then the agent's own words.
fn join(failure: Option<String>, tail: Option<String>) -> Option<String> {
    match (failure, tail) {
        (Some(failure), Some(tail)) => Some(format!("{failure}\n{tail}")),
        (failure, tail) => failure.or(tail),
    }
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

fn remember(tail: &Arc<Mutex<VecDeque<String>>>, line: String) {
    let Ok(mut tail) = tail.lock() else {
        return;
    };
    if tail.len() == TAIL_LINES {
        tail.pop_front();
    }
    tail.push_back(line);
}

fn tail_text(tail: &Arc<Mutex<VecDeque<String>>>) -> Option<String> {
    let tail = tail.lock().ok()?;
    (!tail.is_empty()).then(|| {
        format!(
            "the agent said: {}",
            tail.iter().cloned().collect::<Vec<_>>().join("\n")
        )
    })
}

/// A pipe the agent writes on, read from one thread on either platform.
trait Output: Send {
    /// The next line, or None when the agent's output ended.
    fn next_line(&mut self) -> Option<Vec<u8>>;
}

/// A pipe the agent's connection writes on.
trait Input: Send {
    /// Writes one frame's line. False when the agent's input is gone.
    fn write_line(&mut self, line: &[u8]) -> bool;
}

#[cfg(unix)]
impl Output for std::io::BufReader<File> {
    fn next_line(&mut self) -> Option<Vec<u8>> {
        use std::io::BufRead as _;
        let mut line = Vec::new();
        match self.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line),
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
    fn next_line(&mut self) -> Option<Vec<u8>> {
        use futures::AsyncBufReadExt as _;
        futures::executor::block_on(async {
            let mut line = Vec::new();
            match self.0.read_until(b'\n', &mut line).await {
                Ok(0) | Err(_) => None,
                Ok(_) => Some(line),
            }
        })
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

/// Carries the connection's frames through three std threads: one writes lines to the agent, one
/// reads its lines, one keeps its stderr. The reader thread cannot wake the connection itself
/// (GPUI's test scheduler refuses a wake from another thread), so the pump hands its lines over
/// on the executor's timer.
fn carry(process: &mut Process, tail: &Arc<Mutex<VecDeque<String>>>) -> Result<Transport, String> {
    let (input, output, errors) = pipes(process)?;
    let reader = Arc::clone(tail);
    std::thread::Builder::new()
        .name("kg-acp-stderr".into())
        .spawn(move || read_lines(errors, |line| remember(&reader, line)))
        .map_err(|error| error.to_string())?;
    // Bounded, so an agent that floods its output waits for kagan instead of growing the queue.
    let (lines, read) = std::sync::mpsc::sync_channel(READ_BACKLOG);
    std::thread::Builder::new()
        .name("kg-acp-read".into())
        .spawn(move || {
            read_lines(output, |line| {
                let _ = lines.send(line);
            });
        })
        .map_err(|error| error.to_string())?;
    let (frames, inbound) = futures::channel::mpsc::unbounded::<TransportFrame>();
    let (outbound, written) = futures::channel::mpsc::unbounded::<TransportFrame>();
    std::thread::Builder::new()
        .name("kg-acp-write".into())
        .spawn(move || write_frames(input, written))
        .map_err(|error| error.to_string())?;
    Ok(Transport {
        lines: read,
        frames,
        channel: Channel::new(inbound, outbound),
    })
}

/// The reader body both platforms share. A line is capped, and its bytes that are not UTF-8 are
/// replaced, so an agent's output is never dropped without a trace.
fn read_lines(mut output: impl Output, mut on_line: impl FnMut(String)) {
    while let Some(line) = output.next_line() {
        let line = &line[..line.len().min(LINE_MAX)];
        on_line(
            String::from_utf8_lossy(line)
                .trim_end_matches(['\n', '\r'])
                .to_owned(),
        );
    }
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

/// The agent's pipes: two it writes on, one it reads, blocking on the calling thread.
#[cfg(unix)]
fn pipes(
    process: &mut Process,
) -> Result<
    (
        impl Input + 'static,
        impl Output + 'static,
        impl Output + 'static,
    ),
    String,
> {
    let child = process.child();
    let (Some(input), Some(output), Some(errors)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err("the agent has no pipes".into());
    };
    Ok((
        pipe(input)?,
        std::io::BufReader::new(pipe(output)?),
        std::io::BufReader::new(pipe(errors)?),
    ))
}

/// A pipe the agent's own process gave kagan, as a file this thread blocks on.
#[cfg(unix)]
fn pipe<P>(from: P) -> Result<File, String>
where
    P: TryInto<std::os::fd::OwnedFd>,
    P::Error: std::fmt::Display,
{
    blocking(from.try_into().map_err(|error| error.to_string())?)
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

/// Windows pipes answer through async-process's blocking thread pool, so the same three threads
/// wait on them through it, and one reader body still serves both platforms.
#[cfg(windows)]
fn pipes(
    process: &mut Process,
) -> Result<
    (
        impl Input + 'static,
        impl Output + 'static,
        impl Output + 'static,
    ),
    String,
> {
    let child = process.child();
    let (Some(input), Some(output), Some(errors)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err("the agent has no pipes".into());
    };
    Ok((
        Write(input),
        Read(futures::io::BufReader::new(output)),
        Read(futures::io::BufReader::new(errors)),
    ))
}

/// Hands the reader thread's lines to the connection every tick, and lets the connection run in
/// between. Nothing here wakes a task from another thread.
async fn pump(
    lines: std::sync::mpsc::Receiver<String>,
    frames: futures::channel::mpsc::UnboundedSender<TransportFrame>,
    connection: impl Future<Output = Result<(), String>>,
    executor: &BackgroundExecutor,
) -> Result<(), String> {
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
                    // The agent stopped writing: the connection sees its input end, and the run
                    // reports the exit now instead of waiting out a grace it no longer needs.
                    frames.close_channel();
                    return Ok(());
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

/// Talks ACP until the agent's output ends or the connection fails. `initialize` and
/// `session/new` share one limit: an agent that never answers is reported, not waited on.
async fn connect(
    transport: Channel,
    folder: PathBuf,
    events: &async_channel::Sender<AcpEvent>,
    commands: async_channel::Receiver<AcpCommand>,
    executor: &BackgroundExecutor,
) -> Result<(), String> {
    let result = Client
        .v1()
        .name("kagan")
        .connect_with(transport, async |cx| {
            let handshake = async {
                cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                cx.build_session(&folder).block_task().start_session().await
            };
            let setup = match futures::future::select(
                Box::pin(handshake),
                Box::pin(executor.timer(STARTUP)),
            )
            .await
            {
                futures::future::Either::Left((Ok(setup), _)) => setup,
                futures::future::Either::Left((Err(error), _)) => return Err(error),
                futures::future::Either::Right(_) => {
                    return Ok(Err(format!(
                        "the agent did not answer in {} s",
                        STARTUP.as_secs()
                    )));
                }
            };
            let SessionSetup { mut session, .. } = setup;
            let id = session.session_id().to_string();
            if events.send(AcpEvent::Session(id)).await.is_err() {
                return Ok(Err("the thread ended".to_owned()));
            }
            Ok(serve(&mut session, &commands, events).await)
        })
        .await;
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(failure)) => Err(failure),
        Err(error) => Err(error.to_string()),
    }
}

/// What the agent sent that slice 1 does not read, so each kind is logged once per connection
/// rather than once per chunk.
#[derive(Default)]
struct Unread(HashSet<String>);

impl Unread {
    fn update(&mut self, update: &agent_client_protocol::v1::schema::SessionUpdate) {
        let name = map::name(update);
        if self.0.insert(name.clone()) {
            eprintln!("kg: the agent sent {name}, which kagan does not draw yet");
        }
    }

    fn request(&mut self, method: &str) {
        if self.0.insert(method.to_owned()) {
            eprintln!("kg: the agent asked for {method}, which kagan does not serve yet");
        }
    }
}

/// Serves the thread's commands and the agent's messages until one side is gone: a reply chunk
/// becomes an event, and the prompt's response ends its turn.
async fn serve(
    session: &mut agent_client_protocol::v1::ActiveSession<'_, Agent>,
    commands: &async_channel::Receiver<AcpCommand>,
    events: &async_channel::Sender<AcpEvent>,
) -> Result<(), String> {
    let mut unread = Unread::default();
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
            Step::Update(Ok(message)) => heard(*message, events, &mut unread).await,
            Step::Update(Err(error)) => return Err(error.to_string()),
            Step::Command(AcpCommand::Prompt(text)) => session
                .send_prompt(text)
                .map_err(|error| error.to_string())?,
            Step::Command(AcpCommand::Cancel) => session
                .connection()
                .send_notification_to(Agent, CancelNotification::new(session.session_id().clone()))
                .map_err(|error| error.to_string())?,
            Step::Closed => return Ok(()),
        }
    }
}

/// One thing the agent said, as far as slice 1 reads it: an answer chunk, its turn's end, or a
/// permission request.
async fn heard(
    message: SessionMessage,
    events: &async_channel::Sender<AcpEvent>,
    unread: &mut Unread,
) {
    match message {
        SessionMessage::PromptResponse(response) => {
            let (outcome, error) = match response {
                Ok(response) => map::outcome(response.stop_reason),
                Err(error) => (TurnOutcome::Failed, Some(error.to_string())),
            };
            send(events, AcpEvent::TurnDone { outcome, error }).await;
        }
        SessionMessage::Message(dispatch) => answer(dispatch.into_raw(), events, unread).await,
        // Slice 2 reads the rest; until then a kind kagan does not draw is not an answer.
        _ => eprintln!("kg: the agent sent something kagan does not read yet"),
    }
}

enum Step {
    Update(Result<Box<SessionMessage>, agent_client_protocol::Error>),
    Command(AcpCommand),
    Closed,
}

/// One message from the agent for this session: a reply chunk, or a request that slice 1 does
/// not serve. A request kagan drops is answered by the protocol with an error, so the agent's
/// call cannot wait forever.
async fn answer(dispatch: Dispatch, events: &async_channel::Sender<AcpEvent>, unread: &mut Unread) {
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
            } else {
                unread.update(&notification.update);
            }
            return;
        }
        Ok(Err(dispatch)) => dispatch,
        Err(_) => {
            eprintln!("kg: the agent sent a message kagan could not read");
            return;
        }
    };
    match dispatch.into_request::<RequestPermissionRequest>() {
        Ok(Ok((_request, responder))) => {
            let _ = responder.respond(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ));
        }
        // The protocol answers a request kagan drops with an error, so its caller never waits; the
        // log says which one, once, so a dialect kagan does not know is visible.
        Ok(Err(dropped)) => unread.request(dropped.method()),
        Err(_) => eprintln!("kg: the agent sent a request kagan could not read"),
    }
}

/// Waits a moment for the agent to exit on its own, then ends its tree and reports how it ended.
async fn exit_status(process: &mut Process, executor: &BackgroundExecutor) -> Option<Exit> {
    let started = Instant::now();
    loop {
        match process.child().try_status() {
            Ok(Some(status)) => return exit_of(status),
            Ok(None) if started.elapsed() < EXIT_GRACE => executor.timer(EXIT_POLL).await,
            Ok(None) => {
                process.kill();
                return killed(process, executor).await;
            }
            Err(_) => return None,
        }
    }
}

/// The status kagan's own kill leaves. It arrives at once on both platforms, so a moment is
/// enough to read it.
async fn killed(process: &mut Process, executor: &BackgroundExecutor) -> Option<Exit> {
    for _ in 0..10 {
        if let Ok(Some(status)) = process.child().try_status() {
            return exit_of(status);
        }
        executor.timer(EXIT_POLL).await;
    }
    None
}

/// How the process ended: its code, or the signal that ended it.
fn exit_of(status: ExitStatus) -> Option<Exit> {
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        return Some(Exit::Signal(signal));
    }
    status
        .code()
        .and_then(|code| u32::try_from(code).ok())
        .map(Exit::Code)
}
