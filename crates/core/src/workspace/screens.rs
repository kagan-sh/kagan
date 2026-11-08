use gpui_kit::{App, AppContext as _, Task};
use kg_domain::ThreadId;

use crate::host::database::Result;
use crate::workspace::clock::missing;

use super::Workspace;

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
            None => Task::ready(Err(missing("terminal", id))),
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
pub(super) fn remember(workspace: &mut Workspace, screens: Vec<kg_store::ScreenRow>) {
    for screen in screens {
        let id = screen.thread_id.clone();
        let run = workspace.run_mut(&id);
        run.started_seq = Some(screen.started_seq);
        run.saved = true;
        run.loss = crate::end::Loss::from_kept(screen.screen_kept, screen.history_dropped);
        if screen.screen_kept && run.shown.is_none() {
            match kg_terminal::Finished::restore(screen.cols, screen.rows, &screen.replay) {
                Some(finished) => run.shown = Some(crate::thread::Shown::Ended(finished)),
                None => {
                    eprintln!(
                        "kg: saved screen for {id} was outside the bound and was not restored"
                    );
                }
            }
        }
    }
}
