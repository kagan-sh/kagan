use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{
    EnvironmentStatus, EventKind, OpError, OpResult, ThreadId, ThreadStatus, TurnOutcome,
};
use kg_terminal::Exit;

use crate::Workspace;
use crate::host::database::Result;
use crate::workspace::closed;

use super::agent::Start;
use super::screen::keep_screen;
use super::turn::done;
use super::{Shown, start};
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
    let Some(Shown::Running(terminal)) = &run.shown else {
        return false;
    };
    terminal.stop();
    run.stopping.get_or_insert(StopReason::Stop);
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
    start(this, &thread, Start::Again, cx).await?;
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
    let Ok(thread) = crate::thread::thread(this, &id, cx) else {
        return Ok(OpResult::Done);
    };
    let (current, turn_open) =
        thread.read_with(cx, |thread, _| (thread.record.status, thread.turn_open));
    if !matches!(current, ThreadStatus::Active | ThreadStatus::Stopping) {
        return Ok(OpResult::Done);
    }
    if turn_open {
        thread.update(cx, |thread, _| thread.turn_open = false);
    }
    crate::thread::halt::settle(this, &thread, cx).await?;
    this.update(cx, |workspace, cx| keep_screen(workspace, &id, cx))
        .map_err(closed)?;
    if let Some(reason) = reason {
        Workspace::events(
            this,
            id,
            close_turn(
                turn_open && reason != StopReason::Quit,
                TurnOutcome::Interrupted,
                None,
                vec![EventKind::Stopped, EventKind::Idle],
            ),
            cx,
        )
        .await?;
        status(this, &thread, ThreadStatus::Idle, cx).await?;
        return Ok(OpResult::Done);
    }
    let (events, next) = match exit {
        Exit::Code(0) => (
            close_turn(
                turn_open,
                TurnOutcome::Completed,
                None,
                vec![
                    EventKind::Exited {
                        code: Some(0),
                        signal: None,
                    },
                    EventKind::Idle,
                ],
            ),
            ThreadStatus::Idle,
        ),
        Exit::Code(code) => {
            let error = format!("the program exited with code {code}");
            (
                close_turn(
                    turn_open,
                    TurnOutcome::Failed,
                    Some(error.clone()),
                    vec![
                        EventKind::Exited {
                            code: Some(code),
                            signal: None,
                        },
                        EventKind::Failed { error: Some(error) },
                    ],
                ),
                ThreadStatus::Error,
            )
        }
        Exit::Signal(signal) => {
            let error = format!("the program was ended by signal {signal}");
            (
                close_turn(
                    turn_open,
                    TurnOutcome::Failed,
                    Some(error.clone()),
                    vec![
                        EventKind::Exited {
                            code: None,
                            signal: Some(signal),
                        },
                        EventKind::Failed { error: Some(error) },
                    ],
                ),
                ThreadStatus::Error,
            )
        }
    };
    Workspace::events(this, id, events, cx).await?;
    status(this, &thread, next, cx).await?;
    if next == ThreadStatus::Idle {
        done(this, &thread, cx).await?;
    }
    Ok(OpResult::Done)
}
// A recovered thread has the status of a running program and no terminal in this process.
fn program_is_live(workspace: &Workspace, id: &ThreadId, cx: &gpui_kit::App) -> bool {
    let shown = workspace.shown(id);
    if matches!(shown, Some(Shown::Running(_))) {
        return true;
    }
    let status = workspace
        .threads
        .get(id)
        .map(|thread| thread.read(cx).record.status);
    matches!(
        status,
        Some(ThreadStatus::Active | ThreadStatus::Starting | ThreadStatus::Stopping)
    ) && shown.is_some()
}

fn close_turn(
    open: bool,
    outcome: TurnOutcome,
    error: Option<String>,
    mut kinds: Vec<EventKind>,
) -> Vec<EventKind> {
    if open {
        kinds.insert(0, EventKind::TurnEnded { outcome, error });
    }
    kinds
}
