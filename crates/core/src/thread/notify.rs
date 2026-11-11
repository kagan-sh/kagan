use std::time::{Duration, Instant};

use gpui_kit::Context;
use kg_domain::{Event, EventKind, NotifyReason, OpResult, ThreadId, TurnOutcome, WaitOutcome};

use super::terminal::{self, Shown};
use crate::{Workspace, WorkspaceEvent};

// A parent that just got a line may still be starting its turn; hold the next line this long.
const SETTLE: Duration = Duration::from_secs(2);
// Text the person has not sent yet may still be a draft; wait this long after its last key.
const DRAFT_HOLD: Duration = Duration::from_secs(30);

enum Report {
    Finished,
    Exited {
        code: Option<u32>,
        signal: Option<i32>,
    },
    Failed(Option<String>),
}

pub(crate) fn after(
    workspace: &mut Workspace,
    id: &ThreadId,
    events: &[Event],
    cx: &mut Context<Workspace>,
) {
    let kinds: Vec<&EventKind> = events.iter().map(|event| &event.kind).collect();
    if let Some(report) = report(&kinds) {
        if !matches!(report, Report::Finished) {
            cx.emit(WorkspaceEvent::Notice(id.clone(), NotifyReason::Failed));
        }
        tell(workspace, id, &sentence(&report), cx);
    }
    schedule(workspace, id, cx);
}

pub(crate) fn asked(workspace: &mut Workspace, id: &ThreadId, cx: &mut Context<Workspace>) {
    tell(workspace, id, "needs you", cx);
}

pub(crate) fn typed(workspace: &mut Workspace, id: &ThreadId, text: &str, now: Instant) {
    if text.ends_with('\r') {
        workspace.run_mut(id).held_until = Some(now + crate::TYPING_QUIET);
    } else {
        hold(workspace, id, now + DRAFT_HOLD);
    }
}

pub(crate) fn reports_end(result: &OpResult) -> bool {
    match result {
        OpResult::Wait(outcome) => *outcome != WaitOutcome::Timeout,
        OpResult::Event(Some(event)) => report(&[&event.kind]).is_some(),
        _ => false,
    }
}

pub(crate) fn heard(workspace: &mut Workspace, parent: &ThreadId, child: &ThreadId) {
    if let Some(run) = workspace.runs.get_mut(parent) {
        run.notes.remove(child);
    }
}

fn report(kinds: &[&EventKind]) -> Option<Report> {
    let stopped = kinds.iter().any(|kind| {
        matches!(
            kind,
            EventKind::Stopped
                | EventKind::TurnEnded {
                    outcome: TurnOutcome::Interrupted,
                    ..
                }
        )
    });
    if stopped {
        return None;
    }
    for kind in kinds {
        match kind {
            EventKind::Exited { code, signal } => {
                return Some(Report::Exited {
                    code: *code,
                    signal: *signal,
                });
            }
            EventKind::Failed { error } => return Some(Report::Failed(error.clone())),
            _ => {}
        }
    }
    kinds
        .iter()
        .any(|kind| {
            matches!(
                kind,
                EventKind::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    ..
                }
            )
        })
        .then_some(Report::Finished)
}

fn sentence(report: &Report) -> String {
    match report {
        Report::Finished => "finished its turn".into(),
        Report::Exited {
            code: Some(code), ..
        } => format!("exited with code {code}"),
        Report::Exited {
            signal: Some(signal),
            ..
        } => format!("exited on signal {signal}"),
        Report::Exited { .. } => "exited".into(),
        Report::Failed(Some(reason)) => {
            let reason = reason.split_whitespace().collect::<Vec<_>>().join(" ");
            format!("failed: {reason}")
        }
        Report::Failed(None) => "failed".into(),
    }
}

// The line is typed into a parent's program, so only plain words of a title get through: letters,
// digits, spaces and `-_.`, then at most 40 characters. The id always follows, for `kg thread`.
const TITLE_WORDS_MAX: usize = 40;

fn named(child: &ThreadId, title: &str) -> String {
    let plain: String = title
        .chars()
        .filter(|ch| ch.is_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.'))
        .collect();
    let words = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    let short: String = words.chars().take(TITLE_WORDS_MAX).collect();
    if short.is_empty() {
        child.to_string()
    } else {
        format!("{short} ({child})")
    }
}

fn tell(workspace: &mut Workspace, child: &ThreadId, what: &str, cx: &mut Context<Workspace>) {
    let Some(record) = workspace
        .threads
        .get(child)
        .map(|thread| thread.read(cx).record.clone())
    else {
        return;
    };
    let parent = if record.quiet {
        None
    } else {
        record.parent_thread_id.clone()
    };
    let Some(parent) = parent else {
        return;
    };
    let line = format!(
        "[kagan] child {} {what}. Read it: kg thread screen {child}",
        named(child, &record.title)
    );
    let Some(run) = workspace
        .runs
        .get_mut(&parent)
        .filter(|run| matches!(run.shown, Some(Shown::Running(_))))
    else {
        return;
    };
    run.notes.insert(child.clone(), line);
    schedule(workspace, &parent, cx);
}

fn hold(workspace: &mut Workspace, id: &ThreadId, until: Instant) {
    let run = workspace.run_mut(id);
    run.held_until = Some(run.held_until.map_or(until, |held| held.max(until)));
}

// Types the queued lines into the parent as one message once it is idle and the person is not typing.
fn schedule(workspace: &mut Workspace, parent: &ThreadId, cx: &mut Context<Workspace>) {
    let Some(run) = workspace.runs.get_mut(parent) else {
        return;
    };
    if run.notes.is_empty() {
        return;
    }
    if !matches!(run.shown, Some(Shown::Running(_))) {
        run.notes.clear();
        return;
    }
    let busy = workspace.threads.get(parent).is_some_and(|thread| {
        let thread = thread.read(cx);
        thread.busy || thread.turn.open()
    });
    if busy {
        return;
    }
    let now = cx.background_executor().now();
    if let Some(until) = run.held_until
        && until > now
    {
        let wait = until - now;
        let parent = parent.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            _ = this.update(cx, |workspace, cx| schedule(workspace, &parent, cx));
        })
        .detach();
        return;
    }
    let text = std::mem::take(&mut run.notes)
        .into_values()
        .collect::<Vec<_>>()
        .join(" ");
    run.held_until = Some(now + SETTLE);
    let parent = parent.clone();
    cx.spawn(async move |this, cx| {
        if let Err(error) = terminal::send(&this, parent, &text, cx).await {
            eprintln!("kg: could not tell a parent about its child: {error}");
        }
    })
    .detach();
}
