use gpui_kit::{App, AppContext as _, AsyncApp, Task, WeakEntity};
use kg_domain::ThreadId;
use kg_store::ScreenRow;

use crate::host::database::Result;
use crate::workspace::clock::{closed, missing};

use super::{Workspace, WorkspaceEvent};

impl Workspace {
    fn end(&self, id: &ThreadId) -> Option<&crate::end::End> {
        self.runs.get(id)?.end.as_ref()
    }
    #[must_use]
    pub fn end_line(&self, id: &ThreadId) -> Option<String> {
        self.end(id).map(crate::end::End::line)
    }
    #[must_use]
    pub fn screen_note(&self, id: &ThreadId) -> Option<&'static str> {
        self.runs.get(id)?.loss.map(crate::end::Loss::note)
    }
    #[cfg(any(test, feature = "test"))]
    pub fn set_snapshot_bound(&mut self, id: &ThreadId, bound: usize) -> bool {
        let Some(crate::thread::Shown::Running(terminal)) = self.shown(id) else {
            return false;
        };
        terminal.set_snapshot_bound(bound);
        true
    }
    /// Clones `View` on the foreground and builds the newest `cap` bytes of plain text on
    /// the background executor.
    pub fn thread_output(id: &ThreadId, cap: usize, cx: &mut App) -> Task<Result<String>> {
        let lines = Self::global(cx)
            .read(cx)
            .shown(id)
            .map(|shown| shown.view().clone());
        match lines {
            None => Task::ready(Err(missing("thread", id))),
            Some(lines) => cx.background_spawn(async move { Ok(lines.tail(cap)) }),
        }
    }
    /// A stop or any exit code. A signal, a quit, a failure, or an interrupted start does
    /// not count.
    #[must_use]
    pub fn program_ended(&self, id: &ThreadId) -> bool {
        self.end(id).is_some_and(crate::end::End::shows_mark)
    }
    /// A non-zero exit, a signal, a quit, a failure, an interrupted start, or a screen that was
    /// not kept in full.
    #[must_use]
    pub fn recovery(&self, id: &ThreadId) -> bool {
        self.screen_note(id).is_some()
            || self.end(id).is_some_and(|end| {
                !matches!(end, crate::end::End::Stopped | crate::end::End::Exited(0))
            })
    }
}
/// Records each saved screen and returns the kept ones that nothing shows yet.
pub(super) fn remember(workspace: &mut Workspace, screens: Vec<ScreenRow>) -> Vec<ScreenRow> {
    let mut unshown = Vec::new();
    for screen in screens {
        let run = workspace.run_mut(&screen.thread_id);
        run.started_seq = Some(screen.started_seq);
        run.saved = true;
        run.loss = crate::end::Loss::from_kept(screen.screen_kept, screen.history_dropped);
        if screen.screen_kept && run.shown.is_none() {
            unshown.push(screen);
        }
    }
    unshown
}

/// Reads the replay of each unshown screen and restores it on the background executor. A run
/// that began or got a screen in the meantime keeps what it has.
pub(super) async fn restore_unshown(
    this: &WeakEntity<Workspace>,
    screens: Vec<ScreenRow>,
    cx: &mut AsyncApp,
) -> Result<()> {
    if screens.is_empty() {
        return Ok(());
    }
    let database = crate::workspace::database(this, cx)?;
    let replays = database
        .call(move |store| {
            let mut replays = Vec::new();
            for screen in screens {
                if let Some(replay) = store.screen_replay(&screen.thread_id)? {
                    replays.push((screen, replay));
                }
            }
            Ok(replays)
        })
        .await?;
    let restored = cx
        .background_executor()
        .spawn(async move {
            replays
                .into_iter()
                .filter_map(|(screen, replay)| {
                    let finished =
                        kg_terminal::Finished::restore(screen.cols, screen.rows, &replay);
                    if finished.is_none() {
                        eprintln!(
                            "kg: saved screen for {} was outside the bound and was not restored",
                            screen.thread_id
                        );
                    }
                    Some((screen.thread_id, screen.started_seq, finished?))
                })
                .collect::<Vec<_>>()
        })
        .await;
    this.update(cx, |workspace, cx| {
        let mut shown = false;
        for (id, started_seq, finished) in restored {
            let Some(run) = workspace.runs.get_mut(&id) else {
                continue;
            };
            if run.shown.is_none() && run.started_seq == Some(started_seq) {
                run.shown = Some(crate::thread::Shown::Ended(finished));
                shown = true;
            }
        }
        if shown {
            cx.emit(WorkspaceEvent::Changed);
            cx.notify();
        }
    })
    .map_err(closed)
}
