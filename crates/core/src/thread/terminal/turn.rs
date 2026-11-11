use std::path::Path;
use std::time::{Duration, Instant};

use gpui_kit::{App, AsyncApp, Entity, WeakEntity};
use kg_domain::{EventKind, KNOWN_PROGRAMS, OpResult, ThreadId, TurnOp, TurnOutcome};

use crate::host::database::Result;
use crate::thread::turn_state::{Change, Signal, Source, TurnState};
use crate::workspace::closed;
use crate::{Thread, Workspace, WorkspaceEvent};

use super::{QUIET, StopReason};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Spin {
    Opened,
    /// The turn was already open, and `busy` came back after Stop cleared it.
    Resumed,
    Closed,
}

/// One watch tick: the window title, and whether the program's output changed since the last tick.
pub(crate) fn follow(
    thread: &Entity<Thread>,
    title: Option<&str>,
    changed: bool,
    now: Instant,
    cx: &mut App,
) -> Option<Spin> {
    thread.update(cx, |thread, cx| {
        let busy_before = thread.busy;
        if changed {
            thread.output_at = Some(now);
        }
        let mut change = None;
        if std::mem::take(&mut thread.submitted) {
            change = step(thread, Signal::Submit, now).or(change);
        }
        if title.is_some_and(busy) {
            thread.plain_since = None;
            change = step(thread, Signal::Title { busy: true }, now).or(change);
        } else if thread.turn.open() {
            let since = *thread.plain_since.get_or_insert(now);
            if now.duration_since(since) >= QUIET {
                change = step(thread, Signal::Title { busy: false }, now).or(change);
            }
        }
        let silent = thread
            .output_at
            .map_or(Duration::ZERO, |at| now.duration_since(at));
        change = step(thread, Signal::Silence(silent), now).or(change);
        let spin = match change {
            Some(Change::Opened) => Spin::Opened,
            Some(Change::Closed) => Spin::Closed,
            None if thread.busy != busy_before => Spin::Resumed,
            None => return None,
        };
        cx.notify();
        Some(spin)
    })
}

/// An agent's own hook reports a turn boundary. A start from a program that is not running is ignored.
pub(crate) async fn hook(
    this: &WeakEntity<Workspace>,
    op: TurnOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let (id, signal) = match op {
        TurnOp::Start { id } => (id, Signal::HookStart),
        TurnOp::End { id } => (id, Signal::HookEnd),
    };
    let thread = crate::thread::thread(this, &id, cx)?;
    let now = cx.background_executor().now();
    let change = thread.update(cx, |thread, cx| {
        if signal == Signal::HookStart && !thread.running() {
            return None;
        }
        let change = step(thread, signal, now);
        cx.notify();
        change
    });
    match change {
        Some(Change::Opened) => {
            Workspace::events(this, id, vec![EventKind::TurnStarted], cx).await?;
        }
        Some(Change::Closed) => waiting(this, id, cx).await?,
        None => {}
    }
    Ok(OpResult::Done)
}

/// Applies one signal to a thread's turn, and keeps `busy` in step with it.
fn step(thread: &mut Thread, signal: Signal, now: Instant) -> Option<Change> {
    let change = thread.turn.apply(signal);
    if change == Some(Change::Opened) {
        thread.output_at = Some(now);
    }
    thread.busy = thread.turn.open();
    change
}

/// A new run starts with its source already known when kagan set a hook; otherwise it is decided by
/// the first signal it sees.
pub(crate) fn begin_run(workspace: &Workspace, id: &ThreadId, hooks: bool, cx: &mut App) {
    let Some(thread) = workspace.threads.get(id) else {
        return;
    };
    let source = if hooks {
        Source::Hook
    } else {
        Source::Undecided
    };
    thread.update(cx, |thread, _| {
        thread.turn = TurnState::new(source);
        thread.busy = false;
        thread.output_at = None;
        thread.plain_since = None;
        thread.submitted = false;
    });
}

/// The person sent a line to the program. Only a known agent gets a quiet turn from it, so a shell
/// never reports a turn it cannot end.
pub(crate) fn note_submit(workspace: &Workspace, id: &ThreadId, cx: &mut App) {
    let Some(thread) = workspace.threads.get(id) else {
        return;
    };
    thread.update(cx, |thread, _| {
        if known_agent(&thread.record.provider) {
            thread.submitted = true;
        }
    });
}

fn known_agent(provider: &str) -> bool {
    let Ok((name, _)) = kg_domain::split_program(provider) else {
        return false;
    };
    Path::new(&name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| KNOWN_PROGRAMS.iter().any(|(known, _)| *known == stem))
}

pub(crate) fn busy(title: &str) -> bool {
    title
        .chars()
        .next()
        .is_some_and(|first| matches!(first, '\u{2800}'..='\u{28FF}' | '\u{25D0}'..='\u{25D3}'))
}

pub(super) async fn waiting(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    cx: &mut AsyncApp,
) -> Result<()> {
    let reason = this
        .update(cx, |workspace, _| {
            workspace.runs.get(&id).and_then(|run| run.stopping)
        })
        .map_err(closed)?;
    let thread = crate::thread::thread(this, &id, cx)?;
    done(this, &thread, cx).await?;
    let mut kinds = vec![EventKind::Waiting];
    if reason != Some(StopReason::Quit) {
        kinds.insert(
            0,
            EventKind::TurnEnded {
                outcome: if reason.is_some() {
                    TurnOutcome::Interrupted
                } else {
                    TurnOutcome::Completed
                },
                error: None,
            },
        );
    }
    Workspace::events(this, id.clone(), kinds, cx).await?;
    this.update(cx, |_, cx| {
        cx.emit(WorkspaceEvent::Notice(
            id.clone(),
            kg_domain::NotifyReason::TurnEnded,
        ));
        cx.emit(WorkspaceEvent::Waiting(id));
    })
    .map_err(closed)
}

pub(super) async fn done(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let database = crate::workspace::database(this, cx)?;
    let id = thread.read_with(cx, |thread, _| thread.record.id.clone());
    let saved = id.clone();
    database.call(move |store| store.mark_done(&saved)).await?;
    thread.update(cx, |thread, cx| {
        thread.record.mark = kg_domain::ThreadMark::Done;
        cx.notify();
    });
    this.update(cx, |_, cx| {
        cx.emit(WorkspaceEvent::Finished(id));
        cx.emit(WorkspaceEvent::Changed);
    })
    .map_err(closed)
}
