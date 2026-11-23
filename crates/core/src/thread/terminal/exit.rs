use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{
    EnvironmentStatus, EventKind, OpError, OpResult, ThreadId, ThreadStatus, TurnOutcome,
};
use kg_terminal::Exit;

use crate::Workspace;
use crate::host::database::Result;
use crate::thread::finish::{End, ended};
use crate::workspace::closed;

use super::Run;
use super::Shown;
use super::agent::Start;
use super::screen::keep_screen;
use crate::thread::halt::status;

/// Why a program was asked to stop. Quit still records `stopped` and `idle`, and skips `turn.ended`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StopReason {
    Stop,
    Quit,
}

/// Asks a running program to end and marks the thread stopping. Its exit records the stop and keeps
/// the screen. Returns whether there was a program to end.
pub(crate) fn request_stop(workspace: &mut Workspace, id: &ThreadId) -> bool {
    let Some(run) = workspace.runs.get_mut(id) else {
        return false;
    };
    if !release(run) {
        return false;
    }
    run.stopping.get_or_insert(StopReason::Stop);
    true
}

/// Ends what a run owns: the agent's connection and the task that reads it, or its terminal.
/// Returns whether there was a program to end.
fn release(run: &mut Run) -> bool {
    if run.acp.take().is_some() {
        run.acp_task = None;
        return true;
    }
    let Some(Shown::Running(terminal)) = &run.shown else {
        return false;
    };
    terminal.stop();
    true
}
/// Runs a stopped or recovered thread's program again, in the folder it had.
/// Opening the app calls this for a thread the last process left running.
pub(crate) async fn start_again(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let thread = crate::thread::thread(this, &id, cx)?;
    let record = thread.read_with(cx, |thread, _| thread.record.clone());
    if record.archived_at.is_some() {
        return Err(OpError::Invalid(
            "restore the thread before starting it again".into(),
        ));
    }
    let blocked = this
        .update(cx, |workspace, cx| program_is_live(workspace, &id, cx))
        .map_err(closed)?;
    if blocked {
        return Err(OpError::Invalid("the program is still running".into()));
    }
    let database = crate::workspace::database(this, cx)?;
    let folder = record.environment_id.clone();
    let ready = match folder {
        Some(folder) => database
            .call(move |store| store.environment(&folder))
            .await?
            .is_some_and(|environment| environment.status == EnvironmentStatus::Ready),
        None => false,
    };
    if !ready {
        return Err(OpError::Invalid(
            "its folder is not ready; restore it with kg environment restore first".into(),
        ));
    }
    status(this, &thread, ThreadStatus::Starting, cx).await?;
    crate::thread::start_program(this, &thread, Start::Again, cx).await?;
    Ok(OpResult::Thread(
        thread.read_with(cx, |thread, _| thread.record.clone()),
    ))
}

/// Records how the program ended. It runs in the request loop, after the start that made it, so Active is set first.
pub(crate) async fn exited(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    exit: Exit,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let reason = this
        .update(cx, |workspace, _| {
            workspace.runs.get_mut(&id).and_then(|run| {
                run.exiting = false;
                run.stopping.take()
            })
        })
        .map_err(closed)?;
    this.update(cx, |workspace, cx| keep_screen(workspace, &id, cx))
        .map_err(closed)?;
    let end = match reason {
        Some(reason) => End {
            turn: (reason != StopReason::Quit).then_some((TurnOutcome::Interrupted, None)),
            events: vec![EventKind::Stopped, EventKind::Idle],
            status: ThreadStatus::Idle,
        },
        None => exit_end(Some(exit), None),
    };
    ended(this, id, end, cx).await
}

/// What a program's exit leaves behind. `exit` is None when kagan could not read one at all, and
/// `reason` is what it knows beyond the exit, such as an agent's own last words. A clean exit
/// ignores the reason.
pub(crate) fn exit_end(exit: Option<Exit>, reason: Option<String>) -> End {
    let Some(exit) = exit else {
        let error = reason.unwrap_or_else(|| "the program ended without an exit code".into());
        return End {
            turn: Some((TurnOutcome::Failed, Some(error.clone()))),
            events: vec![EventKind::Failed { error: Some(error) }],
            status: ThreadStatus::Error,
        };
    };
    let (code, signal) = match exit {
        Exit::Code(code) => (Some(code), None),
        Exit::Signal(signal) => (None, Some(signal)),
    };
    let exited = EventKind::Exited { code, signal };
    if exit == Exit::Code(0) && reason.is_none() {
        return End {
            turn: Some((TurnOutcome::Completed, None)),
            events: vec![exited, EventKind::Idle],
            status: ThreadStatus::Idle,
        };
    }
    let default = match exit {
        Exit::Code(code) => format!("the program exited with code {code}"),
        Exit::Signal(signal) => format!("the program was ended by signal {signal}"),
    };
    let error = match reason {
        Some(reason) => format!("{default}\n{reason}"),
        None => default,
    };
    End {
        turn: Some((TurnOutcome::Failed, Some(error.clone()))),
        events: vec![exited, EventKind::Failed { error: Some(error) }],
        status: ThreadStatus::Error,
    }
}
// A recovered thread has the status of a running program and no terminal in this process.
fn program_is_live(workspace: &Workspace, id: &ThreadId, cx: &gpui_kit::App) -> bool {
    let shown = workspace.shown(id);
    if matches!(shown, Some(Shown::Running(_))) {
        return true;
    }
    if workspace.runs.get(id).is_some_and(|run| run.acp.is_some()) {
        return true;
    }
    let status = workspace
        .threads
        .get(id)
        .map(|thread| thread.read(cx).record.status);
    status.is_some_and(ThreadStatus::running) && shown.is_some()
}
