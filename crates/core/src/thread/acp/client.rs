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

const STARTUP: Duration = Duration::from_secs(30);
const EXIT_POLL: Duration = Duration::from_millis(20);

#[derive(Debug, thiserror::Error)]
pub(super) enum AcpError {
    #[error("could not start {program} in {cwd}: {source}")]
    Spawn {
        program: String,
        cwd: String,
        source: std::io::Error,
    },
    #[error("the agent gave no pipes")]
    NoPipes,
    #[error("could not set up the agent's pipes: {0}")]
    Pipes(#[source] std::io::Error),
    #[error("the agent did not answer in {} s", .0.as_secs())]
    Handshake(Duration),
    #[error(transparent)]
    Protocol(#[from] agent_client_protocol::Error),
    #[error("the thread ended")]
    Ended,
}

impl From<AcpError> for agent_client_protocol::Error {
    fn from(error: AcpError) -> Self {
        // The SDK's closure carries its own error type, so kagan's failure becomes its message.
        Self::new(
            agent_client_protocol::ErrorCode::INTERNAL_ERROR,
            error.to_string(),
        )
    }
}

#[derive(Debug)]
pub(super) enum AcpEvent {
    Session {
        id: String,
        restored: Restored,
    },
    Chunk {
        thought: bool,
        text: String,
        message_id: Option<String>,
    },
    Tool(map::ToolUpdate),
    Plan(Vec<kg_domain::PlanEntry>),
    Usage {
        used: u64,
        size: u64,
    },
    Permission {
        tool: map::ToolUpdate,
        options: Vec<kg_domain::InteractionOption>,
        answer: async_channel::Sender<String>,
    },
    TurnDone {
        outcome: TurnOutcome,
        error: Option<String>,
    },
    Ended {
        exit: Option<Exit>,
        error: Option<String>,
    },
}

#[derive(Debug)]
pub(super) enum AcpCommand {
    Prompt(String),
    Cancel,
}

#[derive(Debug)]
pub(super) enum Restored {
    Loaded,
    New,
    Refused(String),
}

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
                    error: Some(error.to_string()),
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
                    error: Some(error.to_string()),
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
    let failure = pump(lines, frames, connection, &executor)
        .await
        .err()
        .map(|error| error.to_string());
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

fn join(failure: Option<String>, tail: Option<String>) -> Option<String> {
    match (failure, tail) {
        (Some(failure), Some(tail)) => Some(format!("{failure}\n{tail}")),
        (failure, tail) => failure.or(tail),
    }
}

async fn connect(
    transport: Channel,
    folder: PathBuf,
    session: Session,
    events: &async_channel::Sender<AcpEvent>,
    commands: async_channel::Receiver<AcpCommand>,
    executor: &BackgroundExecutor,
) -> Result<(), AcpError> {
    // The SDK's closure carries its own error type, so kagan's failure becomes its message there.
    let failed = Client
        .v1()
        .name("kagan")
        .connect_with(transport, async |cx| {
            talk(cx, folder, session, events, commands, executor)
                .await
                .map_err(agent_client_protocol::Error::from)
        })
        .await;
    failed.map_err(AcpError::from)
}

async fn talk(
    cx: agent_client_protocol::v1::ConnectionTo<Agent>,
    folder: PathBuf,
    session: Session,
    events: &async_channel::Sender<AcpEvent>,
    commands: async_channel::Receiver<AcpCommand>,
    executor: &BackgroundExecutor,
) -> Result<(), AcpError> {
    let Session { resume, servers } = session;
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
            Ok::<_, AcpError>(session)
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
                    // A refused load is an agent that cannot continue this conversation after
                    // all: the person gets a new chat, and the agent's words.
                    Err(error) => (new.await?, Restored::Refused(error.to_string())),
                }
            }
            None => (new.await?, Restored::New),
        };
        Ok::<_, AcpError>((session, restored))
    };
    let setup =
        match futures::future::select(Box::pin(handshake), Box::pin(executor.timer(STARTUP))).await
        {
            futures::future::Either::Left((result, _)) => result?,
            futures::future::Either::Right(_) => return Err(AcpError::Handshake(STARTUP)),
        };
    let (mut session, restored) = setup;
    let id = session.session_id().to_string();
    if events
        .send(AcpEvent::Session { id, restored })
        .await
        .is_err()
    {
        return Err(AcpError::Ended);
    }
    serve(&mut session, &commands, events).await
}

// The replay is the conversation kagan already stores; writing it would double the transcript.
fn forget(session: &mut ActiveSession<'_, Agent>) {
    use futures::FutureExt as _;
    // The SDK queues every replay update before it resolves the load response, so this sees them all.
    while matches!(session.read_update().now_or_never(), Some(Ok(_))) {}
}

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

    fn other(&mut self) {
        if self.0.insert("another kind of message".to_owned()) {
            eprintln!("kg: the agent sent something kagan does not read yet");
        }
    }
}

// ACP re-sends the whole output on every update; one the log already holds is not written again.
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

async fn serve(
    session: &mut agent_client_protocol::v1::ActiveSession<'_, Agent>,
    commands: &async_channel::Receiver<AcpCommand>,
    events: &async_channel::Sender<AcpEvent>,
) -> Result<(), AcpError> {
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
            Step::Command(AcpCommand::Prompt(text)) => session.send_prompt(text)?,
            Step::Command(AcpCommand::Cancel) => {
                session.connection().send_notification_to(
                    Agent,
                    CancelNotification::new(session.session_id().clone()),
                )?;
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

async fn killed(process: &mut Process, executor: &BackgroundExecutor) -> Option<Exit> {
    for _ in 0..10 {
        if let Ok(Some(status)) = process.child().try_status() {
            return exit_of(status);
        }
        executor.timer(EXIT_POLL).await;
    }
    None
}

fn exit_of(status: ExitStatus) -> Option<Exit> {
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        return Some(Exit::Signal(signal));
    }
    status.code().map(exit_code)
}

// Windows reports a code above `i32::MAX` as a negative number, so the sign is cast away, not refused.
fn exit_code(code: i32) -> Exit {
    Exit::Code(code.cast_unsigned())
}

#[cfg(test)]
#[path = "client_test.rs"]
mod tests;
