use std::collections::BTreeMap;

use gpui_kit::{App, AppContext as _, Task};
use kg_domain::ThreadId;

use crate::host::database::Result;
use crate::workspace::clock::missing;

use super::Workspace;

impl Workspace {
    #[must_use]
    pub fn end_line(&self, id: &ThreadId) -> Option<String> {
        self.ends.get(id).map(crate::end::End::line)
    }
    #[must_use]
    pub fn screen_note(&self, id: &ThreadId) -> Option<&'static str> {
        self.losses.get(id).copied().map(crate::end::Loss::note)
    }
    #[cfg(any(test, feature = "test"))]
    pub fn set_snapshot_bound(&mut self, id: &ThreadId, bound: usize) -> bool {
        let Some(crate::thread::Shown::Running(terminal)) = self.terminals.get(id) else {
            return false;
        };
        terminal.set_snapshot_bound(bound);
        true
    }
    /// Clones `Lines` on the foreground and builds the newest `cap` bytes of plain text on
    /// the background executor.
    pub fn thread_output(id: &ThreadId, cap: usize, cx: &mut App) -> Task<Result<String>> {
        let lines = Self::global(cx)
            .read(cx)
            .terminals
            .get(id)
            .map(|shown| match shown {
                crate::thread::Shown::Running(terminal) => terminal.lines(),
                crate::thread::Shown::Ended(finished) => finished.lines(),
            });
        match lines {
            None => Task::ready(Err(missing("terminal", id))),
            Some(lines) => cx.background_spawn(async move { Ok(lines.tail(cap)) }),
        }
    }
    /// A stop or any exit code. A signal, a quit, a failure, or an interrupted start does
    /// not count.
    #[must_use]
    pub fn program_ended(&self, id: &ThreadId) -> bool {
        self.ends.get(id).is_some_and(crate::end::End::shows_mark)
    }
    /// A non-zero exit, a signal, a quit, a failure, an interrupted start, or a screen that was
    /// not kept in full.
    #[must_use]
    pub fn recovery(&self, id: &ThreadId) -> bool {
        self.screen_note(id).is_some()
            || self.ends.get(id).is_some_and(|end| {
                !matches!(end, crate::end::End::Stopped | crate::end::End::Exited(0))
            })
    }
}
pub(super) fn remember(
    workspace: &mut Workspace,
    screens: Vec<kg_store::ScreenRow>,
) -> BTreeMap<ThreadId, crate::end::Loss> {
    let mut losses = BTreeMap::new();
    for screen in screens {
        let id = screen.thread_id.clone();
        workspace.runs.insert(id.clone(), screen.started_seq);
        workspace.saved.insert((id.clone(), screen.started_seq));
        if let Some(loss) = crate::end::Loss::from_kept(screen.screen_kept, screen.history_dropped)
        {
            losses.insert(id.clone(), loss);
        }
        if screen.screen_kept && !workspace.terminals.contains_key(&id) {
            match kg_terminal::Finished::restore(screen.cols, screen.rows, &screen.replay) {
                Some(finished) => {
                    workspace
                        .terminals
                        .insert(id, crate::thread::Shown::Ended(finished));
                }
                None => {
                    eprintln!(
                        "kg: saved screen for {id} was outside the bound and was not restored"
                    );
                }
            }
        }
    }
    losses
}
