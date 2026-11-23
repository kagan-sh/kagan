use std::ffi::OsString;
use std::path::Path;

use crate::thread::terminal::{exit_end, utf8_locale, waiting};
use gpui_kit::{AppContext as _, AsyncApp, Entity, Task, WeakEntity};
use kg_domain::{
    Conversation, EventKind, Link, NoticeSeverity, OpError, OpResult, Role, ThreadId, ThreadStatus,
    TurnOutcome, acp_command,
};

use crate::Thread;
use crate::host::database::Result;
use crate::workspace::closed;
use crate::{
    Workspace,
    thread::Raised,
    thread::agent::{self, Start},
    thread::halt::status,
    thread::turn_state::{Signal, Source},
    thread::{finish, resolve_program},
};

mod client;
mod map;
mod permission;

use client::{AcpCommand, AcpEvent};

const NO_AGENT: &str = "the agent is not running; start the thread again";
const NOT_A_TERMINAL: &str =
    "this is an ACP thread, not a terminal; read the transcript with kg thread events";

/// A live ACP connection. Dropping it cancels the connection, and the process tree with it.
pub(crate) struct Acp {
    commands: async_channel::Sender<AcpCommand>,
    /// The questions this connection asked the person, so Cancel and the end of a turn can take
    /// back the ones it raised and leave the thread's others alone.
    pub(super) raised: Vec<Raised>,
    _connection: Task<()>,
}

impl Acp {
    fn send(&self, command: AcpCommand) -> bool {
        self.commands.try_send(command).is_ok()
    }

    /// Closes the commands, for a test that needs a prompt refused while the run still holds the
    /// connection.
    #[cfg(test)]
    pub(crate) fn close(&self) {
        self.commands.close();
    }
}

/// Starts the agent as its own ACP connection, then lets it name the session.
pub(super) async fn start(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    start: Start,
    cx: &mut AsyncApp,
) -> Result<()> {
    let record = thread.read_with(cx, |thread, _| thread.record.clone());
    let folder = this
        .update(cx, |workspace, cx| {
            record
                .environment_id
                .as_ref()
                .and_then(|id| workspace.environments.get(id))
                .map(|environment| environment.read(cx).record.path.clone())
        })
        .map_err(closed)?
        .ok_or_else(|| OpError::Invalid("the thread has no folder".into()))?;
    crate::workspace::hold(this, crate::workspace::Hold::Start, cx).await;
    // Taken before the program starts, so what it begins in that moment counts as after the start.
    let at = crate::workspace::time(this, cx)?;
    let launch = match launch(this, &record, &folder, start, cx).await {
        Ok(launch) => launch,
        Err(error) => return failed(this, thread, error, cx).await,
    };
    let (events, received) = async_channel::unbounded();
    let (commands, sent) = async_channel::unbounded();
    let executor = cx.background_executor().clone();
    let connection = cx.background_spawn(client::run(launch, folder, executor, events, sent));
    let id = record.id.clone();
    let consumer = {
        let this = this.clone();
        cx.spawn(async move |cx| {
            consume(&this, id, at, received, cx).await;
        })
    };
    this.update(cx, |workspace, cx| {
        crate::thread::terminal::begin_run(workspace, &record.id, Source::Acp, cx);
        let run = workspace.run_mut(&record.id);
        run.acp = Some(Acp {
            commands,
            raised: Vec::new(),
            _connection: connection,
        });
        run.acp_task = Some(consumer);
    })
    .map_err(closed)?;
    // Slice 1 starts a new session rather than loading the earlier one, so the person is told.
    if matches!(start, Start::Again) {
        let kinds = vec![EventKind::Notice {
            severity: NoticeSeverity::Info,
            text: "This is a new chat with the agent; the earlier conversation is not restored."
                .into(),
        }];
        Workspace::events(this, record.id, kinds, cx).await?;
    }
    Ok(())
}

async fn failed(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    error: OpError,
    cx: &mut AsyncApp,
) -> Result<()> {
    let id = thread.read_with(cx, |thread, _| thread.record.id.clone());
    let kinds = vec![EventKind::Failed {
        error: Some(error.to_string()),
    }];
    Workspace::events(this, id, kinds, cx).await?;
    status(this, thread, ThreadStatus::Error, cx).await?;
    Err(error)
}

/// Builds the command that starts the agent's ACP server: the agent's own ACP subcommand, then
/// the person's arguments, in the thread's folder, with kagan's variables.
async fn launch(
    this: &WeakEntity<Workspace>,
    record: &kg_domain::Thread,
    folder: &Path,
    start: Start,
    cx: &mut AsyncApp,
) -> Result<client::Launch> {
    let environment = crate::host::program::program_environment_of(this, cx);
    let home = crate::host::program::program_home_of(this, cx);
    let program = resolve_program(&record.provider, environment.as_ref(), home.as_deref())
        .map_err(|error| OpError::Invalid(error.to_string()))?;
    let subcommand = acp_command(&record.provider)
        .ok_or_else(|| OpError::Invalid(format!("{} has no ACP command", record.provider)))?;
    let acp = acp_line(&program.path, subcommand, &program.args);
    let integration = agent::integrate(this, record, &acp, folder, start, cx).await?;
    let mut env = integration.env;
    if let Some(locale) = cx.background_spawn(async { utf8_locale() }).await {
        env.push(("LANG".into(), locale.into()));
    }
    let args = std::iter::once(OsString::from(subcommand))
        .chain(program.args)
        .collect();
    Ok(client::Launch {
        program: program.path,
        args,
        cwd: folder.to_owned(),
        env,
    })
}

// The same program, with the ACP subcommand among its arguments, so the agent integration sees
// the subcommand line and adds nothing of its own. The integration's flags are dropped: the ACP
// command is the agent's own line, and slice 5 settles the rest.
fn acp_line(
    path: &Path,
    subcommand: &str,
    args: &[OsString],
) -> crate::thread::run::ResolvedProgram {
    crate::thread::run::ResolvedProgram {
        path: path.to_owned(),
        args: std::iter::once(OsString::from(subcommand))
            .chain(args.iter().cloned())
            .collect(),
    }
}

/// Applies the connection's events in the order they happened.
async fn consume(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    at: kg_domain::Timestamp,
    events: async_channel::Receiver<AcpEvent>,
    cx: &mut AsyncApp,
) {
    let mut pending = None;
    loop {
        let event = match pending.take() {
            Some(event) => event,
            None => match events.recv().await {
                Ok(event) => event,
                Err(_) => break,
            },
        };
        let event = match event {
            AcpEvent::Chunk {
                thought,
                text,
                message_id,
            } => AcpEvent::Chunk {
                thought,
                text: joined(thought, text, message_id.as_deref(), &events, &mut pending),
                message_id,
            },
            other => other,
        };
        let outcome = match event {
            AcpEvent::Session(session) => named(this, &id, &session, at, cx).await,
            AcpEvent::Chunk {
                thought: false,
                text,
                message_id,
            } => {
                let kinds = vec![EventKind::Message {
                    role: Role::Agent,
                    text,
                    message_id,
                    attachments: Vec::new(),
                }];
                Workspace::events(this, id.clone(), kinds, cx).await
            }
            AcpEvent::Chunk {
                thought: true,
                text,
                message_id,
            } => {
                let kinds = vec![EventKind::Thought { text, message_id }];
                Workspace::events(this, id.clone(), kinds, cx).await
            }
            AcpEvent::Tool(tool) => tool_event(this, &id, tool, None, cx).await,
            AcpEvent::Plan(entries) => {
                Workspace::events(this, id.clone(), vec![EventKind::Plan { entries }], cx).await
            }
            AcpEvent::Usage { used, size } => {
                Workspace::events(this, id.clone(), vec![EventKind::Usage { used, size }], cx).await
            }
            AcpEvent::Permission {
                tool,
                options,
                answer,
            } => permission(this, &id, tool, options, answer, cx).await,
            AcpEvent::TurnDone { outcome, error } => turn_done(this, &id, outcome, error, cx).await,
            AcpEvent::Ended { exit, error } => {
                // The agent is gone: the next prompt starts it again rather than writing to a
                // connection that has ended.
                let cleared = this
                    .update(cx, |workspace, _| workspace.run_mut(&id).acp = None)
                    .map_err(|error| OpError::Invalid(error.to_string()));
                let recorded = ended(this, id.clone(), exit, error, cx).await;
                cleared.and(recorded)
            }
        };
        if let Err(error) = outcome {
            eprintln!("kg: could not record what the agent said: {error}");
        }
    }
}

/// Joins the chunks the agent already said for the same message behind this one, so a burst is one
/// write and one event row. Anything else stays queued for the next step.
fn joined(
    thought: bool,
    mut text: String,
    message_id: Option<&str>,
    events: &async_channel::Receiver<AcpEvent>,
    pending: &mut Option<AcpEvent>,
) -> String {
    loop {
        match events.try_recv() {
            Ok(AcpEvent::Chunk {
                thought: same_kind,
                text: more,
                message_id: same,
            }) => {
                if same_kind != thought || same.as_deref() != message_id {
                    *pending = Some(AcpEvent::Chunk {
                        thought: same_kind,
                        text: more,
                        message_id: same,
                    });
                    break;
                }
                text.push_str(&more);
            }
            Ok(other) => {
                *pending = Some(other);
                break;
            }
            Err(_) => break,
        }
    }
    text
}

/// One tool delta as the log keeps it, with the approval that links it to a question when there is
/// one.
async fn tool_event(
    this: &WeakEntity<Workspace>,
    id: &ThreadId,
    tool: map::ToolUpdate,
    approval: Option<kg_domain::PendingInteractionId>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let kinds = vec![EventKind::Tool {
        id: tool.id,
        name: tool.name,
        kind: tool.kind,
        title: tool.title,
        status: tool.status,
        input: tool.input,
        output: tool.output,
        diff: tool.diff,
        locations: tool.locations,
        approval,
    }];
    Workspace::events(this, id.clone(), kinds, cx).await
}

/// The agent wants permission: raise the one question the card and the Questions tab share, then
/// record the tool row it links to. A question that cannot be raised is not the agent's problem:
/// the row still draws, and the dropped answer channel answers the agent `cancelled`.
async fn permission(
    this: &WeakEntity<Workspace>,
    id: &ThreadId,
    tool: map::ToolUpdate,
    options: Vec<kg_domain::InteractionOption>,
    answer: async_channel::Sender<String>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let thread = crate::thread::thread(this, id, cx)?;
    let provider = thread.read_with(cx, |thread, _| {
        kg_domain::program_label(&thread.record.provider).clone()
    });
    let title = format!("{provider} wants to {}", map::phrase(tool.kind));
    let waiting = crate::workspace::Waiting {
        thread: None,
        answer,
        for_a_person: true,
        approval: None,
        ask: None,
    };
    let raised = crate::thread::raise(this, &thread, title, options, None, waiting, cx).await;
    let approval = match raised {
        Ok(raised) => {
            let asked = raised.id.clone();
            // The connection keeps what it asked, so Cancel and the end of the turn take back
            // only its own questions. A connection that is already gone leaves the question to
            // the end that settles the thread.
            this.update(cx, |workspace, _| {
                let acp = workspace.runs.get_mut(id)?.acp.as_mut()?;
                acp.raised.push(raised);
                Some(asked)
            })
            .map_err(closed)?
        }
        Err(error) => {
            eprintln!("kg: could not ask about the agent's tool call: {error}");
            None
        }
    };
    let pending = map::ToolUpdate {
        status: Some(kg_domain::ToolStatus::Pending),
        ..tool
    };
    tool_event(this, id, pending, approval, cx).await
}

/// The agent named its session: that id is the thread's conversation, and the thread is running.
async fn named(
    this: &WeakEntity<Workspace>,
    id: &ThreadId,
    session: &str,
    at: kg_domain::Timestamp,
    cx: &mut AsyncApp,
) -> Result<()> {
    let thread = crate::thread::thread(this, id, cx)?;
    Workspace::events_at(this, id.clone(), at, vec![EventKind::Started], cx).await?;
    status(this, &thread, ThreadStatus::Active, cx).await?;
    let database = crate::workspace::database(this, cx)?;
    let conversation = match session.parse::<Conversation>() {
        Ok(conversation) => conversation,
        Err(error) => {
            let kinds = vec![EventKind::Notice {
                severity: NoticeSeverity::Warning,
                text: format!("the agent named a session kagan cannot store: {error}"),
            }];
            return Workspace::events(this, id.clone(), kinds, cx).await;
        }
    };
    let thread_id = id.clone();
    let previous = database
        .call({
            let thread_id = thread_id.clone();
            move |store| store.conversation(&thread_id)
        })
        .await?;
    let claimed = conversation.clone();
    // One claim for both starts: an id another thread holds is never stored, and a late answer
    // from a start the thread has left is not stored either.
    let saved = database
        .call(move |store| store.claim_conversation(&thread_id, &claimed, previous, at))
        .await?;
    if saved {
        return Ok(());
    }
    let kinds = vec![EventKind::Notice {
        severity: NoticeSeverity::Warning,
        text: "the agent named a session another thread holds, so kagan did not store it".into(),
    }];
    Workspace::events(this, id.clone(), kinds, cx).await
}

/// The agent answered the prompt, so its turn is over.
async fn turn_done(
    this: &WeakEntity<Workspace>,
    id: &ThreadId,
    outcome: TurnOutcome,
    error: Option<String>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let thread = crate::thread::thread(this, id, cx)?;
    // A response that arrives after Stop or the end must not write a turn or a wait on a stopped
    // thread.
    if thread.read_with(cx, |thread, _| thread.record.status) != ThreadStatus::Active {
        return Ok(());
    }
    let now = cx.background_executor().now();
    thread.update(cx, |thread, cx| {
        thread.turn.apply(Signal::PromptDone, now);
        thread.busy = thread.turn.open();
        cx.notify();
    });
    // A question the agent asked and left behind is about a turn that is over: the card goes.
    withdraw_questions(this, id, cx).await?;
    waiting(this, id.clone(), Some((outcome, error)), cx).await
}

/// The agent's connection ended. The process is gone, whatever it said.
async fn ended(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    exit: Option<kg_terminal::Exit>,
    error: Option<String>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let end = exit_end(exit, error);
    finish::ended(this, id, end, cx).await?;
    Ok(())
}

/// Sends text as the thread's next prompt. One prompt at a time: the turn it opens ends with the
/// agent's answer.
pub(super) async fn prompt(
    this: &WeakEntity<Workspace>,
    id: &ThreadId,
    text: &str,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let thread = crate::thread::thread(this, id, cx)?;
    if !running(this, id, cx)? {
        return Err(OpError::Invalid(NO_AGENT.into()));
    }
    if thread.read_with(cx, |thread, _| thread.turn.open()) {
        return Err(OpError::Invalid(
            "the agent is answering; cancel its reply first".into(),
        ));
    }
    // The send is synchronous, so a refused one leaves no message and no turn behind.
    send(this, id, AcpCommand::Prompt(text.to_owned()), cx)?;
    let kinds = vec![
        EventKind::Message {
            role: Role::User,
            text: text.to_owned(),
            message_id: None,
            attachments: Vec::new(),
        },
        EventKind::TurnStarted,
    ];
    Workspace::events(this, id.clone(), kinds, cx).await?;
    let now = cx.background_executor().now();
    thread.update(cx, |thread, cx| {
        thread.turn.apply(Signal::Prompt, now);
        thread.busy = thread.turn.open();
        cx.notify();
    });
    Ok(OpResult::Done)
}

/// Asks the agent to end its reply; the agent itself keeps running.
pub(super) async fn cancel(
    this: &WeakEntity<Workspace>,
    id: &ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let thread = crate::thread::thread(this, id, cx)?;
    if thread.read_with(cx, |thread, _| thread.record.link) != Link::Acp {
        return Err(OpError::Invalid(
            "this is a terminal thread; stop it instead".into(),
        ));
    }
    if !thread.read_with(cx, |thread, _| thread.turn.open()) {
        return Err(OpError::Invalid("the agent is not answering".into()));
    }
    send(this, id, AcpCommand::Cancel, cx)?;
    // The agent's approval goes the moment the person cancels; the agent's own prompt response
    // would otherwise leave the card up. A question the agent raised through `kg thread ask` is
    // not this connection's, so it stays.
    withdraw_questions(this, id, cx).await?;
    Ok(OpResult::Done)
}

/// Withdraws the questions this connection raised that the person has not answered: the card goes,
/// the attention it raised goes, and an answered question is left alone.
async fn withdraw_questions(
    this: &WeakEntity<Workspace>,
    id: &ThreadId,
    cx: &mut AsyncApp,
) -> Result<()> {
    let raised = this
        .update(cx, |workspace, _| {
            let open: std::collections::BTreeSet<_> = workspace.pending.keys().cloned().collect();
            workspace
                .runs
                .get_mut(id)
                .and_then(|run| run.acp.as_mut())
                .map(|acp| std::mem::take(&mut acp.raised))
                .unwrap_or_default()
                .into_iter()
                .filter(|raised| open.contains(&raised.id))
                .collect::<Vec<_>>()
        })
        .map_err(closed)?;
    for raised in raised {
        crate::thread::withdraw(this, raised, cx).await?;
    }
    Ok(())
}

fn running(this: &WeakEntity<Workspace>, id: &ThreadId, cx: &mut AsyncApp) -> Result<bool> {
    this.update(cx, |workspace, _| {
        workspace.runs.get(id).is_some_and(|run| run.acp.is_some())
    })
    .map_err(closed)
}

fn send(
    this: &WeakEntity<Workspace>,
    id: &ThreadId,
    command: AcpCommand,
    cx: &mut AsyncApp,
) -> Result<()> {
    let sent = this
        .update(cx, |workspace, _| {
            workspace
                .runs
                .get(id)
                .and_then(|run| run.acp.as_ref())
                .is_some_and(|acp| acp.send(command))
        })
        .map_err(closed)?;
    if sent {
        Ok(())
    } else {
        Err(OpError::Invalid(NO_AGENT.into()))
    }
}

/// The refusal every terminal operation gets on an ACP thread that names no extra terminal.
pub(crate) fn not_a_terminal() -> OpError {
    OpError::Invalid(NOT_A_TERMINAL.into())
}
