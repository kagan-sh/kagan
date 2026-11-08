use crate::theme::TypeScale as _;

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::{Copy, Input, SelectAll};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, ClipboardItem, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Role, StatefulInteractiveElement as _, Styled as _, Window, div,
};
use kg_core::Workspace;
use kg_domain::Op;

use super::{
    CONTEXT, CloseReview, FindInDiff, NextChange, NextFile, NextMatch, Pane, PrevChange, PrevFile,
    PrevMatch, Review, SendReview, ToggleViewed, ToggleWrap, change_position, review_title,
};
use crate::kagan::Kagan;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
impl Kagan {
    pub(in crate::kagan) fn render_review(
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
                this.send_review(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseReview, window, cx| {
                if this.draft_focused(window, cx) {
                    this.cancel_comment(window, cx);
                } else {
                    this.close_review(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextChange, _, cx| {
                this.next_change(cx);
            }))
            .on_action(cx.listener(|this, _: &PrevChange, _, cx| {
                this.prev_change(cx);
            }))
            .on_action(
                cx.listener(|this, _: &FindInDiff, window, cx| this.find_in_review(window, cx)),
            )
            .on_action(cx.listener(|this, _: &NextMatch, _, cx| {
                this.search_match(true, cx);
            }))
            .on_action(cx.listener(|this, _: &PrevMatch, _, cx| {
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
                                .text_size(gpui_kit::px(crate::theme::type_body(cx)))
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
            .type_small(cx)
            .text_color(cx.theme().muted_foreground)
            .child(position)
            .into_any_element()
    }

    pub(super) fn review_pane_toggle(review: &Review, cx: &mut Context<Self>) -> AnyElement {
        let label = if review.pane == Pane::Files {
            "Show file"
        } else {
            "Files"
        };
        crate::kagan::controls::control(
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
                .type_small(cx)
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
