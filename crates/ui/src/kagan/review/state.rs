use super::text::renamed_from;

use gpui_kit::{Context, Window};

use crate::kagan::Kagan;
impl Kagan {
    pub(in crate::kagan) fn close_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(review) = self.screen.review_mut()
            && review.selecting.take().is_some()
        {
            window.focus(&review.focus, cx);
            cx.notify();
            return;
        }
        if let Some(review) = self.screen.review_mut()
            && review.searching.take().is_some()
        {
            window.focus(&review.focus, cx);
            cx.notify();
            return;
        }
        self.leave_review(window, cx);
    }

    pub(in crate::kagan) fn leave_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen.review().is_none() {
            return;
        }
        self.screen.close();
        self.read_shown(cx);
        self.sync_panels_keeping_widths(window, cx);
        self.announce_questions(window, cx);
        self.return_focus(crate::kagan::screen::FocusReturn::Center, window, cx);
        cx.notify();
    }

    pub(in crate::kagan) fn sync_review_after_changes(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.screen.review() else {
            return;
        };
        let path = review.path.clone();
        let next = self
            .changes
            .files
            .iter()
            .find(|file| file.path == path)
            .or_else(|| self.changes.files.first());
        if let Some(file) = next {
            if let Some(review) = self.screen.review_mut() {
                if review.path != file.path || review.scope != self.changes.scope {
                    review.draft =
                        self.review_drafts
                            .active(&review.thread, self.changes.scope, &file.path);
                    review.path.clone_from(&file.path);
                    review.shown.clear();
                    review.expanded.clear();
                    review.loading = false;
                    review.generation = review.generation.wrapping_add(1);
                }
                review.scope = self.changes.scope;
                review.renamed_from = renamed_from(file);
                self.changes.selected = Some(file.path.clone());
            }
        } else {
            self.screen.close();
            self.read_shown(cx);
            self.changes.selected = None;
        }
        cx.notify();
    }

    pub(in crate::kagan) fn next_change(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.screen.review_mut() else {
            return;
        };
        let Some(last) = review.hunks.len().checked_sub(1) else {
            return;
        };
        let at = review.at.map_or(0, |at| (at + 1).min(last));
        self.go_to_change(at, cx);
    }

    pub(in crate::kagan) fn prev_change(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.screen.review_mut() else {
            return;
        };
        if review.hunks.is_empty() {
            return;
        }
        let at = review.at.map_or(0, |at| at.saturating_sub(1));
        self.go_to_change(at, cx);
    }

    fn go_to_change(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(review) = self.screen.review_mut() else {
            return;
        };
        review.at = Some(at);
        let row = review.hunks[at];
        review.scroll.scroll_to_top_of_item(row as usize);
        cx.notify();
    }
}
