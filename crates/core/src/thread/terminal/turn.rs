use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::Instant;

use gpui_kit::{App, AsyncApp, Entity, WeakEntity};
use kg_domain::{EventKind, KNOWN_PROGRAMS, OpResult, ThreadId, TurnOp, TurnOutcome};
use kg_terminal::Grid;

use crate::host::database::Result;
use crate::thread::turn_state::{Change, Signal, Source, TurnState};
use crate::workspace::closed;
use crate::{Thread, Workspace, WorkspaceEvent};

use super::StopReason;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Spin {
    Opened,
    /// The turn was already open, and `busy` came back after Stop cleared it.
    Resumed,
    Closed,
}

/// One watch tick. `changed` is the terminal's cheap redraw flag; `screen` reads the visible text,
/// and is called only when a quiet turn or a submit is waiting on it.
pub(crate) fn follow(
    thread: &Entity<Thread>,
    title: Option<&str>,
    changed: bool,
    screen: impl FnOnce() -> u64,
    now: Instant,
    cx: &mut App,
) -> Option<Spin> {
    thread.update(cx, |thread, cx| {
        let busy_before = thread.busy;
        let mut change = None;
        if changed {
            let signal = if thread.turn.wants_text() {
                Signal::Screen(screen())
            } else {
                Signal::Output
            };
            change = step(thread, signal, now).or(change);
        }
        let title = Signal::Title {
            busy: title.is_some_and(busy),
        };
        change = step(thread, title, now).or(change);
        change = step(thread, Signal::Tick, now).or(change);
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

/// A hash of the visible rows. A repaint with the same text hashes the same.
pub(crate) fn screen_hash(grid: &Grid) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for line in &grid.lines {
        let text: String = line.iter().map(|cell| cell.c).collect();
        text.trim_end().hash(&mut hasher);
    }
    hasher.finish()
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

fn step(thread: &mut Thread, signal: Signal, now: Instant) -> Option<Change> {
    let change = thread.turn.apply(signal, now);
    thread.busy = thread.turn.open();
    change
}

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
    });
}

// Only a known agent's line can start a quiet turn, so a shell never reports one.
pub(crate) fn note_submit(workspace: &Workspace, id: &ThreadId, now: Instant, cx: &mut App) {
    let Some(thread) = workspace.threads.get(id) else {
        return;
    };
    if !known_agent(&thread.read(cx).record.provider) {
        return;
    }
    let screen = workspace
        .shown(id)
        .and_then(|shown| shown.running())
        .map_or(0, |terminal| screen_hash(&terminal.view().grid()));
    thread.update(cx, |thread, _| {
        thread.turn.apply(Signal::Submit { screen }, now);
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
