use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::v1::schema::{
    CancelNotification, InitializeRequest, LoadSessionRequest, McpServer, NewSessionRequest,
    RequestPermissionRequest, SessionNotification,
};
use agent_client_protocol::v1::{ActiveSession, SessionMessage, SessionSetup};
use agent_client_protocol::{Agent, Channel, Client, Dispatch};
use gpui_kit::BackgroundExecutor;
use kg_domain::{ToolStatus, TurnOutcome};
use kg_process::Process;
use kg_terminal::Exit;

use super::map;
use super::permission::Asking;
use super::transport::{EXIT_GRACE, Transport, carry, pump, spawn, tail_text};

/// How long the agent is given to answer `initialize` and `session/new`.
const STARTUP: Duration = Duration::from_secs(30);
/// How long to wait between two looks at the agent's exit.
const EXIT_POLL: Duration = Duration::from_millis(20);

/// What the connection reports to the thread, in the order it happened.
#[derive(Debug)]
pub(super) enum AcpEvent {
    /// The agent named its session. It is the thread's conversation, and `restored` says whether
    /// it is the one the start asked for.
    Session { id: String, restored: Restored },
    /// One chunk of the agent's reply or its reasoning.
    Chunk {
        thought: bool,
        text: String,
        message_id: Option<String>,
    },
    /// One tool call, or an update to one with the same id.
    Tool(map::ToolUpdate),
    /// The agent's whole plan.
    Plan(Vec<kg_domain::PlanEntry>),
    /// Tokens in context.
    Usage { used: u64, size: u64 },
    /// The agent wants permission; `answer` carries the person's chosen option id.
    Permission {
        tool: map::ToolUpdate,
        options: Vec<kg_domain::InteractionOption>,
        answer: async_channel::Sender<String>,
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

/// What Start again did with the conversation the thread stored, so the thread can say it.
#[derive(Debug)]
pub(super) enum Restored {
    /// The agent loaded the earlier conversation: the thread is the same session as before.
    Loaded,
    /// A new session, because there was nothing to load or the agent cannot load.
    New,
    /// The agent was asked for the earlier conversation and refused: its own words.
    Refused(String),
}

/// What the thread asks the agent for besides running it: the conversation to continue, and the
/// MCP server kagan offers it.
#[derive(Debug)]
pub(super) struct Session {
    pub(super) resume: Option<kg_domain::Conversation>,
    pub(super) servers: Vec<McpServer>,
}

pub(super) struct Launch {
    pub(super) program: PathBuf,
    pub(super) args: Vec<OsString>,
    pub(super) cwd: PathBuf,
    pub(super) env: Vec<(OsString, OsString)>,
    pub(super) session: Session,
}

/// Runs one agent for the life of its thread: spawn it, talk ACP, and report how it ended.
pub(super) async fn run(
    launch: Launch,
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
    let connection = connect(
        channel,
        launch.cwd.clone(),
        launch.session,
        &events,
        commands,
        &executor,
    );
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

/// Talks ACP until the agent's output ends or the connection fails. `initialize` and the session
/// share one limit: an agent that never answers is reported, not waited on.
async fn connect(
    transport: Channel,
    folder: PathBuf,
    session: Session,
    events: &async_channel::Sender<AcpEvent>,
    commands: async_channel::Receiver<AcpCommand>,
    executor: &BackgroundExecutor,
) -> Result<(), String> {
    let Session { resume, servers } = session;
    let result = Client
        .v1()
        .name("kagan")
        .connect_with(transport, async |cx| {
            let handshake = async {
                let initialized = cx
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                // A conversation is loaded only when the agent says it can load one: the answer,
                // never the agent's name, decides.
                let resume = resume.filter(|_| initialized.agent_capabilities.load_session);
                let new = async {
                    let request = NewSessionRequest::new(&folder).mcp_servers(servers.clone());
                    let SessionSetup { session, .. } = cx
                        .build_session_from(request)
                        .block_task()
                        .start_session()
                        .await?;
                    Ok::<_, agent_client_protocol::Error>(session)
                };
                let (session, restored): (_, Restored) = match resume {
                    Some(conversation) => {
                        let request = LoadSessionRequest::new(conversation.to_string(), &folder)
                            .mcp_servers(servers.clone());
                        match cx
                            .load_session_from(request)
                            .block_task()
                            .start_session()
                            .await
                        {
                            Ok(SessionSetup { mut session, .. }) => {
                                forget(&mut session);
                                (session, Restored::Loaded)
                            }
                            // A refused load is an agent that cannot continue this conversation
                            // after all: the person gets a new chat, and the agent's words.
                            Err(error) => (new.await?, Restored::Refused(error.to_string())),
                        }
                    }
                    None => (new.await?, Restored::New),
                };
                Ok::<_, agent_client_protocol::Error>((session, restored))
            };
            let setup = match futures::future::select(
                Box::pin(handshake),
                Box::pin(executor.timer(STARTUP)),
            )
            .await
            {
                futures::future::Either::Left((result, _)) => result,
                futures::future::Either::Right(_) => {
                    return Ok(Err(format!(
                        "the agent did not answer in {} s",
                        STARTUP.as_secs()
                    )));
                }
            };
            let (mut session, restored) = match setup {
                Ok(setup) => setup,
                Err(error) => return Err(error),
            };
            let id = session.session_id().to_string();
            if events
                .send(AcpEvent::Session { id, restored })
                .await
                .is_err()
            {
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

/// Drops the updates a load replays before the thread reads the session again. The replay is the
/// conversation kagan already stores, so writing it would double the transcript.
fn forget(session: &mut ActiveSession<'_, Agent>) {
    use futures::FutureExt as _;
    // The SDK queues every replay update before it resolves the load response, so this sees them all.
    while matches!(session.read_update().now_or_never(), Some(Ok(_))) {}
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

    /// A message kind this build's protocol version cannot name.
    fn other(&mut self) {
        if self.0.insert("another kind of message".to_owned()) {
            eprintln!("kg: the agent sent something kagan does not read yet");
        }
    }
}

/// The output each tool call last reported. ACP re-sends the whole output on every update, so an
/// update that says nothing but one the log already holds is not written again. A call's entry goes
/// when the call completes or fails, so a long session holds only the calls still running.
#[derive(Default)]
struct Repeats(HashMap<String, String>);

impl Repeats {
    fn repeats(&mut self, tool: &map::ToolUpdate) -> bool {
        let last = self.0.remove(&tool.id);
        let Some(output) = &tool.output else {
            return false;
        };
        if matches!(
            tool.status,
            Some(ToolStatus::Completed | ToolStatus::Failed)
        ) {
            return false;
        }
        let only_output = tool.name.is_none()
            && tool.kind.is_none()
            && tool.title.is_none()
            && tool.status.is_none()
            && tool.input.is_none()
            && tool.diff.is_empty()
            && tool.locations.is_empty();
        self.0.insert(tool.id.clone(), output.clone());
        only_output && last.is_some_and(|last| last == *output)
    }
}

/// Serves the thread's commands, the agent's messages, and any permission answer until one side is
/// gone: an update becomes an event, and the prompt's response ends its turn.
async fn serve(
    session: &mut agent_client_protocol::v1::ActiveSession<'_, Agent>,
    commands: &async_channel::Receiver<AcpCommand>,
    events: &async_channel::Sender<AcpEvent>,
) -> Result<(), String> {
    let mut unread = Unread::default();
    let mut repeats = Repeats::default();
    let mut asking = Asking::default();
    loop {
        let step = {
            let update = Box::pin(session.read_update());
            let command = Box::pin(commands.recv());
            let heard = async {
                match futures::future::select(update, command).await {
                    futures::future::Either::Left((update, _)) => {
                        Step::Update(update.map(Box::new))
                    }
                    futures::future::Either::Right((Ok(command), _)) => Step::Command(command),
                    futures::future::Either::Right((Err(_), _)) => Step::Closed,
                }
            };
            match asking.opening() {
                Some(answer) => {
                    let heard = Box::pin(heard);
                    let answer = Box::pin(answer.recv());
                    match futures::future::select(heard, answer).await {
                        futures::future::Either::Left((step, _)) => step,
                        futures::future::Either::Right((answer, _)) => Step::Answer(answer),
                    }
                }
                None => heard.await,
            }
        };
        match step {
            Step::Update(Ok(message)) => {
                heard(*message, events, &mut unread, &mut repeats, &mut asking).await;
            }
            Step::Answer(answer) => asking.answered(answer),
            Step::Command(AcpCommand::Prompt(text)) => session
                .send_prompt(text)
                .map_err(|error| error.to_string())?,
            Step::Command(AcpCommand::Cancel) => {
                session
                    .connection()
                    .send_notification_to(
                        Agent,
                        CancelNotification::new(session.session_id().clone()),
                    )
                    .map_err(|error| error.to_string())?;
                asking.cancel();
            }
            // The update stream closes when the agent's output ends, and the commands when the
            // thread does. Either way the connection is over, not failed: the exit and the tail
            // say how it ended.
            Step::Update(Err(_)) | Step::Closed => return Ok(()),
        }
    }
}

enum Step {
    Update(Result<Box<SessionMessage>, agent_client_protocol::Error>),
    Command(AcpCommand),
    Answer(Result<String, async_channel::RecvError>),
    Closed,
}

/// One thing the agent said: an update, its turn's end, or a permission request.
async fn heard(
    message: SessionMessage,
    events: &async_channel::Sender<AcpEvent>,
    unread: &mut Unread,
    repeats: &mut Repeats,
    asking: &mut Asking,
) {
    match message {
        SessionMessage::PromptResponse(response) => {
            let (outcome, error) = match response {
                Ok(response) => map::outcome(response.stop_reason),
                Err(error) => (TurnOutcome::Failed, Some(error.to_string())),
            };
            send(events, AcpEvent::TurnDone { outcome, error }).await;
        }
        SessionMessage::Message(dispatch) => {
            answer(dispatch.into_raw(), events, unread, repeats, asking).await;
        }
        // A kind this build does not name, logged once rather than on every chunk.
        _ => unread.other(),
    }
}

/// One message from the agent for this session: an update to draw, or a permission request.
async fn answer(
    dispatch: Dispatch,
    events: &async_channel::Sender<AcpEvent>,
    unread: &mut Unread,
    repeats: &mut Repeats,
    asking: &mut Asking,
) {
    let dispatch = match dispatch.into_notification::<SessionNotification>() {
        Ok(Ok(notification)) => {
            match map::read(&notification.update) {
                Some(map::Update::Message(chunk)) => {
                    send(
                        events,
                        AcpEvent::Chunk {
                            thought: false,
                            text: chunk.text,
                            message_id: chunk.message_id,
                        },
                    )
                    .await;
                }
                Some(map::Update::Thought(chunk)) => {
                    send(
                        events,
                        AcpEvent::Chunk {
                            thought: true,
                            text: chunk.text,
                            message_id: chunk.message_id,
                        },
                    )
                    .await;
                }
                Some(map::Update::Tool(tool)) => {
                    if !repeats.repeats(&tool) {
                        send(events, AcpEvent::Tool(tool)).await;
                    }
                }
                Some(map::Update::Plan(entries)) => send(events, AcpEvent::Plan(entries)).await,
                Some(map::Update::Usage { used, size }) => {
                    send(events, AcpEvent::Usage { used, size }).await;
                }
                None => unread.update(&notification.update),
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
        Ok(Ok((request, responder))) => {
            asking.ask(request, responder, events).await;
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
    status.code().map(exit_code)
}

/// How Windows reports a code above `i32::MAX`: as a negative number. The sign is cast away, not
/// refused, so a thread says the same code a terminal does.
fn exit_code(code: i32) -> Exit {
    Exit::Code(code.cast_unsigned())
}

#[cfg(test)]
#[path = "client_test.rs"]
mod tests;
