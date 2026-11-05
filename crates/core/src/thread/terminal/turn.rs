use std::time::Instant;

use gpui_kit::{App, AsyncApp, Entity, WeakEntity};
use kg_domain::{EventKind, ThreadId, TurnOutcome};

use crate::database::Result;
use crate::workspace::closed;
use crate::{Thread, Workspace, WorkspaceEvent};

use super::{QUIET, StopReason};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Spin {
    Opened,
    /// Stop cleared `busy` while the title was still a spinner.
    Resumed,
    Closed,
}

pub(crate) fn follow(
    thread: &Entity<Thread>,
    title: Option<&str>,
    now: Instant,
    cx: &mut App,
) -> Option<Spin> {
    let busy_title = title.is_some_and(busy);
    thread.update(cx, |thread, cx| {
        let next = if busy_title {
            thread.plain_since = None;
            true
        } else if thread.busy {
            let since = *thread.plain_since.get_or_insert(now);
            now.duration_since(since) < QUIET
        } else {
            false
        };
        if next == thread.busy {
            return None;
        }
        let opened = next && !thread.turn_open;
        thread.busy = next;
        thread.turn_open = next;
        cx.notify();
        Some(if !next {
            Spin::Closed
        } else if opened {
            Spin::Opened
        } else {
            Spin::Resumed
        })
    })
}

pub(super) async fn waiting(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    cx: &mut AsyncApp,
) -> Result<()> {
    let reason = this
        .update(cx, |workspace, _| workspace.stopping.get(&id).copied())
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
    this.update(cx, |_, cx| cx.emit(WorkspaceEvent::Waiting(id)))
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
pub(crate) fn busy(title: &str) -> bool {
    title
        .chars()
        .next()
        .is_some_and(|first| matches!(first, '\u{2800}'..='\u{28FF}' | '\u{25D0}'..='\u{25D3}'))
}
