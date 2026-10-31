#[cfg(test)]
#[path = "../tests/review_read.rs"]
pub(crate) mod read;
use crate::theme::TypeScale as _;
mod actions;
mod comment;
mod page;
mod search;
mod text;
pub(super) mod tree;

pub(super) use comment::Drafts;

use text::{layout, notice, renamed_from};

use std::collections::BTreeSet;

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::{Copy, Input, InputState, SelectAll};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, AppContext as _, ClipboardItem, Context, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, ParentElement as _, Role, ScrollHandle,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, actions, div,
};
use kg_core::Workspace;
use kg_domain::{DiffOp, DiffScope, Op, OpError, OpResult, ReviewRow, ThreadId, review_rows};

use super::Kagan;
use super::changes::query;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(crate) const CONTEXT: &str = "Review";

actions!(
    review,
    [
        CloseReview,
        NextChange,
        PrevChange,
        FindInDiff,
        NextMatch,
        PrevMatch,
        NextFile,
        PrevFile,
        ToggleViewed,
        ToggleWrap,
        SendReview
    ]
);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    File,
    Files,
}

pub(super) struct Review {
    pub(super) thread: ThreadId,
    pub(super) scope: DiffScope,
    pub(super) path: String,
    window: AnyWindowHandle,
    search: Entity<InputState>,
    _search_watch: Subscription,
    match_at: Option<usize>,
    match_row: Option<usize>,
    searching: Option<()>,
    selecting: Option<()>,
    focus: FocusHandle,
    renamed_from: Option<String>,
    notice: Option<String>,
    hunks: Vec<u32>,
    at: Option<usize>,
    // Unchanged text must not reset the editor's cursor during a poll.
    pub(super) shown: String,
    revision: String,
    pub(super) loading: bool,
    load: Task<()>,
    pub(super) generation: u64,
    pub(super) rows: Vec<ReviewRow>,
    expanded: BTreeSet<u32>,
    filter: Entity<InputState>,
    _filter_watch: Subscription,
    tree: bool,
    pane: Pane,
    collapsed: BTreeSet<String>,
    tree_focus: Option<String>,
    pub(super) draft: Option<usize>,
    drag: Option<(kg_domain::DiffSide, u32)>,
    scroll: ScrollHandle,
    send: Task<()>,
    sending: bool,
    send_error: Option<String>,
}

struct ReadExpect {
    generation: u64,
    thread: ThreadId,
    scope: DiffScope,
    path: String,
}

impl Review {
    pub(super) fn is_for(&self, thread: &ThreadId) -> bool {
        &self.thread == thread
    }
}

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
            super::screen::Center::Review(Box::new(Review {
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
                send: Task::ready(()),
                sending: false,
                send_error: None,
            })),
            window,
            cx,
        );
        self.focus_review(window, cx);
        if self.changes.viewed_checking.is_none() {
            let loaded = super::layout::read::<std::collections::BTreeMap<String, String>>(
                "review-viewed",
                cx,
            );
            cx.spawn(async move |this, cx| {
                let marks = loaded.await.unwrap_or_default();
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

    pub(super) fn focus_review(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(review) = self.screen.review() {
            window.focus(&review.focus, cx);
        }
    }

    pub(super) fn refresh_review(&mut self, cx: &mut Context<Self>) {
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
        review.load = cx.spawn(async move |this, cx| {
            let shown = shown.await;
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |this, cx| {
                    this.apply_review(&expect, shown, window, cx);
                });
            });
        });
    }

    fn apply_review(
        &mut self,
        expect: &ReadExpect,
        shown: Result<OpResult, OpError>,
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

    pub(super) fn close_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        if self.screen.review().is_none() {
            return;
        }
        self.screen.close();
        self.read_shown(cx);
        self.sync_panels_keeping_widths(window, cx);
        self.announce_questions(window, cx);
        self.return_focus(super::screen::FocusReturn::Center, window, cx);
        cx.notify();
    }

    pub(super) fn sync_review_after_changes(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn next_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.screen.review_mut() else {
            return;
        };
        let Some(last) = review.hunks.len().checked_sub(1) else {
            return;
        };
        let at = review.at.map_or(0, |at| (at + 1).min(last));
        self.go_to_change(at, window, cx);
    }

    pub(super) fn prev_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.screen.review_mut() else {
            return;
        };
        if review.hunks.is_empty() {
            return;
        }
        let at = review.at.map_or(0, |at| at.saturating_sub(1));
        self.go_to_change(at, window, cx);
    }

    fn go_to_change(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.screen.review_mut() else {
            return;
        };
        review.at = Some(at);
        let row = review.hunks[at];
        review.scroll.scroll_to_top_of_item(row as usize);
        let _ = window;
        cx.notify();
    }

    pub(super) fn render_review(
        &self,
        replaced: Option<AnyElement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(review) = self.screen.review() else {
            return div().into_any_element();
        };
        let narrow = f32::from(self.viewport)
            - if self.panes.left {
                f32::from(self.layout.left_width)
            } else {
                0.
            }
            < 640.;
        v_flex()
            .id("review")
            .ui_observe_as(Facts::button(review.path.clone()))
            .key_context(CONTEXT)
            .size_full()
            .min_w_0()
            .min_h_0()
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                if let Some(review) = this.screen.review_mut() {
                    review.selecting = Some(());
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &Copy, _, cx| {
                if let Some(review) = this.screen.review()
                    && review.selecting.is_some()
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(review.shown.clone()));
                }
            }))
            .on_action(cx.listener(|this, _: &NextFile, _, cx| this.review_file(true, cx)))
            .on_action(cx.listener(|this, _: &PrevFile, _, cx| this.review_file(false, cx)))
            .on_action(cx.listener(|this, _: &ToggleViewed, _, cx| this.toggle_viewed(cx)))
            .on_action(cx.listener(|_, _: &ToggleWrap, _, cx| {
                Workspace::dispatch(Op::Window(kg_domain::WindowOp::ToggleReviewWrap), cx).detach();
            }))
            .on_action(cx.listener(|this, _: &SendReview, window, cx| {
                if this.draft_focused(window, cx) {
                    this.add_comment(window, cx);
                }
                this.send_review(cx);
            }))
            .on_action(cx.listener(|this, _: &CloseReview, window, cx| {
                if this.draft_focused(window, cx) {
                    this.cancel_comment(window, cx);
                } else {
                    this.close_review(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextChange, window, cx| {
                this.next_change(window, cx);
            }))
            .on_action(cx.listener(|this, _: &PrevChange, window, cx| {
                this.prev_change(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &FindInDiff, window, cx| this.find_in_review(window, cx)),
            )
            .on_action(cx.listener(|this, _: &NextMatch, window, cx| {
                let _ = window;
                this.search_match(true, cx);
            }))
            .on_action(cx.listener(|this, _: &PrevMatch, window, cx| {
                let _ = window;
                this.search_match(false, cx);
            }))
            .child(div().key_context("ReviewNav").track_focus(&review.focus))
            .child(self.review_summary(cx))
            .child(match replaced {
                Some(body) => h_flex().flex_1().min_h_0().child(body),
                None => h_flex()
                    .flex_1()
                    .min_h_0()
                    .when(!narrow || review.pane == Pane::File, |row| {
                        row.child(self.review_file_column(review, narrow, cx))
                    })
                    .when(!narrow || review.pane == Pane::Files, |row| {
                        row.child(self.review_tree(narrow, window, cx))
                    }),
            })
            .child(self.review_comments(cx))
            .into_any_element()
    }

    fn review_file_column(
        &self,
        review: &Review,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let position = change_position(review);
        let title = review_title(review, cx);
        let actions = self.review_actions(review, cx);
        let body = self.review_body(review, cx);
        v_flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .min_h_0()
            .child(
                h_flex()
                    .h_9()
                    .flex_none()
                    .px_3()
                    .gap_3()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .when(narrow, |bar| {
                        bar.child(Self::review_pane_toggle(review, cx))
                    })
                    .child(div().flex_1().min_w_0().child(title))
                    .child(Self::review_position(position, cx))
                    .child(actions),
            )
            .when(review.searching.is_some(), |column| {
                column.child(
                    div()
                        .id("review-search")
                        .ui_observe_as(Facts::input("Find in file", &review.search, cx))
                        .w_full()
                        .px_3()
                        .py_1()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .child(
                            Input::new(&review.search)
                                .text_size(gpui_kit::px(crate::theme::TYPE_BODY))
                                .small(),
                        ),
                )
            })
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    fn review_position(position: String, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("review-position")
            .ui_observe_as(Facts {
                role: Role::Label,
                value: Some(position.clone().into()),
                ..Facts::button("Change position")
            })
            .flex_none()
            .whitespace_nowrap()
            .type_small()
            .text_color(cx.theme().muted_foreground)
            .child(position)
            .into_any_element()
    }

    fn review_pane_toggle(review: &Review, cx: &mut Context<Self>) -> AnyElement {
        let label = if review.pane == Pane::Files {
            "Show file"
        } else {
            "Files"
        };
        super::shown::control(
            "review-toggle-files",
            Facts::button(label),
            gpui_kit::component::button::Button::new("button-review-toggle-files")
                .ghost()
                .small()
                .icon(gpui_kit::assets::IconName::PanelRight)
                .accessibility_label(label)
                .tooltip(label)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Some(review) = this.screen.review_mut() {
                        review.pane = if review.pane == Pane::Files {
                            Pane::File
                        } else {
                            Pane::Files
                        };
                        window.focus(&review.focus, cx);
                    }
                    cx.notify();
                })),
        )
        .into_any_element()
    }

    fn review_body(&self, review: &Review, cx: &mut Context<Self>) -> AnyElement {
        match &review.notice {
            Some(notice) => div()
                .id("review-notice")
                .ui_observe_as(Facts {
                    role: Role::Label,
                    value: Some(notice.clone().into()),
                    ..Facts::button("Review notice")
                })
                .type_small()
                .text_color(cx.theme().muted_foreground)
                .child(notice.clone())
                .into_any_element(),
            None => div()
                .relative()
                .size_full()
                .min_h_0()
                .child(
                    div()
                        .id("review-rows")
                        .ui_container()
                        .size_full()
                        .when(
                            self.layout.review_wrap,
                            gpui_kit::StatefulInteractiveElement::overflow_y_scroll,
                        )
                        .when(
                            !self.layout.review_wrap,
                            gpui_kit::StatefulInteractiveElement::overflow_scroll,
                        )
                        .track_scroll(&review.scroll)
                        .children(
                            review
                                .rows
                                .iter()
                                .enumerate()
                                .map(|(index, row)| self.review_row(index, row, cx))
                                .collect::<Vec<_>>(),
                        ),
                )
                .into_any_element(),
        }
    }
}

fn change_position(review: &Review) -> String {
    match (review.hunks.len(), review.at) {
        (0, _) => "No line changes".to_owned(),
        (count, Some(at)) => format!("Change {} of {count}", at + 1),
        (count, None) => format!("{count} change{}", if count == 1 { "" } else { "s" }),
    }
}

fn review_title(review: &Review, cx: &mut Context<Kagan>) -> AnyElement {
    let (directory, name) = review
        .path
        .rsplit_once('/')
        .map_or(("", review.path.as_str()), |(directory, name)| {
            (directory, name)
        });
    h_flex()
        .id("review-title")
        .ui_observe_as(Facts::button(review.path.clone()))
        .min_w_0()
        .whitespace_nowrap()
        .type_body()
        .when_some(
            review
                .renamed_from
                .as_ref()
                .map(|old| format!("Renamed from {old}")),
            |title, renamed| {
                title.tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(renamed.clone()).build(window, cx)
                })
            },
        )
        .when(!directory.is_empty(), |title| {
            title.child(
                div()
                    .min_w_0()
                    .text_ellipsis_start()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{directory}/")),
            )
        })
        .child(
            div()
                .flex_none()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .child(name.to_owned()),
        )
        .into_any_element()
}
