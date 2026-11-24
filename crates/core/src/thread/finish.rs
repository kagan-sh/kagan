use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{EventKind, OpResult, ThreadId, ThreadStatus, TurnOutcome};

use crate::Workspace;
use crate::host::database::Result;
use crate::thread::halt::{settle, status};
use crate::thread::terminal::done;
use crate::thread::turn_state::Signal;

/// Why a program was asked to stop. Quit still records `stopped` and `idle`, and skips `turn.ended`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StopReason {
    Stop,
    Quit,
}

/// What a stop leaves behind. Quit leaves the open turn alone, so the next open resumes it.
pub(crate) fn stopped(reason: StopReason) -> End {
    End {
        turn: (reason != StopReason::Quit).then_some((TurnOutcome::Interrupted, None)),
        events: vec![EventKind::Stopped, EventKind::Idle],
        status: ThreadStatus::Idle,
    }
}

/// What ended a thread's program: the turn it leaves open, the events it records, and the status
/// it leaves behind.
pub(crate) struct End {
    /// The end of the open turn. None leaves it alone, as quitting does.
    pub(crate) turn: Option<(TurnOutcome, Option<String>)>,
    pub(crate) events: Vec<EventKind>,
    pub(crate) status: ThreadStatus,
}

/// Records a program's end, once, for a link of either kind. It runs in the request loop, after
/// the start that made it, so Active is set first.
pub(crate) async fn ended(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    end: End,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let Ok(thread) = crate::thread::thread(this, &id, cx) else {
        return Ok(OpResult::Done);
    };
    let (current, turn_open) =
        thread.read_with(cx, |thread, _| (thread.record.status, thread.turn.open()));
    // An agent can end before it names its session, while the thread is still Starting; a
    // terminal is Active before its watcher can report an exit.
    if !matches!(
        current,
        ThreadStatus::Starting | ThreadStatus::Active | ThreadStatus::Stopping
    ) {
        return Ok(OpResult::Done);
    }
    let now = cx.background_executor().now();
    thread.update(cx, |thread, _| thread.turn.apply(Signal::Ended, now));
    settle(this, &thread, cx).await?;
    let mut kinds = end.events;
    if turn_open && let Some((outcome, error)) = end.turn {
        kinds.insert(0, EventKind::TurnEnded { outcome, error });
    }
    Workspace::events(this, id, kinds, cx).await?;
    status(this, &thread, end.status, cx).await?;
    if end.status == ThreadStatus::Idle {
        done(this, &thread, cx).await?;
    }
    Ok(OpResult::Done)
}
