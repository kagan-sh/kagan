use super::text::{layout, notice, renamed_from};

use std::collections::BTreeSet;

use gpui_kit::component::input::InputState;
use gpui_kit::{AppContext as _, Context, ScrollHandle, Task, Window};
use kg_core::Workspace;
use kg_domain::{DiffOp, Op, OpError, OpResult};

use super::rows::{ReviewRow, review_rows};

use super::{Pane, ReadExpect, Review};
use crate::kagan::Kagan;
use crate::kagan::changes::query;
impl Kagan {
    pub(crate) fn open_review(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thread) = self.changes.thread.clone() else {
            return;
        };
        if self.screen.review().is_none() {
            self.screen.remember_center(window, cx);
        }
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Find in file"));
        let search_watch = cx.observe(&search, |this, _, cx| {
            if let Some(review) = this.screen.review_mut() {
                review.match_at = None;
                review.match_row = None;
            }
            cx.notify();
        });
        let focus = cx.focus_handle();
        self.changes.selected = Some(path.clone());
        let renamed = self
            .changes
            .files
            .iter()
            .find(|file| file.path == path)
            .and_then(renamed_from);
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter files"));
        let filter_watch = cx.observe(&filter, |_, _, cx| cx.notify());
        let draft = self
            .review_drafts
            .active(&thread, self.changes.scope, &path);
        self.open_center(
            crate::kagan::screen::Center::Review(Box::new(Review {
                thread,
                scope: self.changes.scope,
                path,
                window: window.window_handle(),
                search,
                _search_watch: search_watch,
                match_at: None,
                match_row: None,
                searching: None,
                selecting: None,
                focus,
                renamed_from: renamed,
                notice: None,
                hunks: Vec::new(),
                at: None,
                shown: String::new(),
                revision: String::new(),
                loading: false,
                load: Task::ready(()),
                generation: 0,
                rows: Vec::new(),
                syntax: super::syntax::Syntax::default(),
                syntax_revision: String::new(),
                expanded: BTreeSet::new(),
                filter,
                _filter_watch: filter_watch,
                tree: true,
                pane: Pane::File,
                collapsed: BTreeSet::new(),
                tree_focus: None,
                draft,
                drag: None,
                scroll: ScrollHandle::new(),
            })),
            window,
            cx,
        );
        self.focus_review(window, cx);
        if self.changes.viewed_checking.is_none() {
            let loaded = crate::kagan::layout::read::<std::collections::BTreeMap<String, String>>(
                "review-viewed",
                cx,
            );
            cx.spawn(async move |this, cx| {
                let Ok(marks) = loaded.await else {
                    return;
                };
                let marks = marks.unwrap_or_default();
                _ = this.update(cx, |this, cx| {
                    if this.changes.viewed_checking.is_none() {
                        this.changes.viewed = marks;
                        this.changes.viewed_checking = Some(false);
                        this.validate_viewed(cx);
                        if this.screen.review().is_some_and(|review| !review.loading) {
                            this.invalidate_viewed(cx);
                        }
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        self.sync_panels_keeping_widths(window, cx);
        self.replaced.forget_review();
        self.refresh_review(cx);
        cx.notify();
    }

    pub(in crate::kagan) fn focus_review(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(review) = self.screen.review() {
            window.focus(&review.focus, cx);
        }
    }

    pub(in crate::kagan) fn refresh_review(&mut self, cx: &mut Context<Self>) {
        self.deliver_review(cx);
        let Some(review) = self.screen.review_mut() else {
            return;
        };
        if review.loading {
            return;
        }
        review.loading = true;
        review.generation = review.generation.wrapping_add(1);
        let expect = ReadExpect {
            generation: review.generation,
            thread: review.thread.clone(),
            scope: review.scope,
            path: review.path.clone(),
        };
        let shown = Workspace::dispatch(
            Op::Diff(DiffOp::Show {
                query: query(expect.thread.clone(), expect.scope),
                path: expect.path.clone(),
            }),
            cx,
        );
        let window = review.window;
        let renamed = review.renamed_from.clone();
        review.load = cx.spawn(async move |this, cx| {
            let shown = shown.await;
            let syntax = fresh_syntax(&this, cx, &expect, &shown, renamed).await;
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |this, cx| {
                    this.apply_review(&expect, shown, syntax, window, cx);
                });
            });
        });
    }

    fn apply_review(
        &mut self,
        expect: &ReadExpect,
        shown: Result<OpResult, OpError>,
        syntax: Option<super::syntax::Syntax>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(review) = self.screen.review_mut() else {
            return;
        };
        if review.generation != expect.generation
            || review.thread != expect.thread
            || review.scope != expect.scope
            || review.path != expect.path
        {
            return;
        }
        match shown {
            Ok(OpResult::DiffFile(view)) if view.path == review.path => {
                if review.revision != view.revision {
                    review.expanded.clear();
                }
                review.revision.clone_from(&view.revision);
                review.rows = review_rows(&view);
                if let Some(syntax) = syntax {
                    review.syntax = syntax;
                    review.syntax_revision.clone_from(&view.revision);
                }
                review.notice = notice(&view);
                let (text, _) = layout(&view);
                review.hunks = view
                    .hunks
                    .iter()
                    .filter_map(|hunk| hunk.lines.first())
                    .filter_map(|first| {
                        review.rows.iter().position(
                            |row| matches!(row, ReviewRow::Line { line, .. } if line == first),
                        )
                    })
                    .filter_map(|index| u32::try_from(index).ok())
                    .collect();
                let text_changed = text != review.shown;

                if text_changed {
                    review.at = None;
                    review.shown.clone_from(&text);
                    review.match_at = None;
                }
            }
            Ok(_) => review.notice = Some("Could not read this file.".to_owned()),
            Err(error) => review.notice = Some(format!("Could not read this file: {error}")),
        }
        review.loading = false;
        self.invalidate_viewed(cx);
        cx.notify();
    }
}

async fn fresh_syntax(
    this: &gpui_kit::WeakEntity<Kagan>,
    cx: &mut gpui_kit::AsyncApp,
    expect: &ReadExpect,
    shown: &Result<OpResult, OpError>,
    renamed: Option<String>,
) -> Option<super::syntax::Syntax> {
    let Ok(OpResult::DiffFile(view)) = shown else {
        return None;
    };
    if view.path != expect.path || view.binary || view.truncated || view.too_large {
        return None;
    }
    let kept = this
        .read_with(cx, |kagan, _| {
            kagan.screen.review().is_some_and(|review| {
                review.path == expect.path && review.syntax_revision == view.revision
            })
        })
        .unwrap_or(false);
    if kept {
        return None;
    }
    let view = view.clone();
    let path = view.path.clone();
    Some(
        cx.background_spawn(
            async move { super::syntax::captures(&view, &path, renamed.as_deref()) },
        )
        .await,
    )
}
