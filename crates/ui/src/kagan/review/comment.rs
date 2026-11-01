use crate::theme::TypeScale as _;
use std::collections::BTreeMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, Subscription, Window, div,
};
use kg_core::Workspace;
use kg_domain::{DiffNote, DiffScope, DiffSide, Op, ThreadId, ThreadOp, format_notes};

use super::Kagan;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

#[derive(Default)]
pub(in crate::kagan) struct Drafts(BTreeMap<ThreadId, Notes>);

#[derive(Default)]
struct Notes {
    saved: Vec<DiffNote>,
    discarded: Vec<DiffNote>,
    pending: Vec<Draft>,
}

const COMMENT_ROW: gpui_kit::Rems = gpui_kit::rems(1.75);

struct Draft {
    input: gpui_kit::Entity<TextareaState>,
    _watch: Subscription,
    note: DiffNote,
}

impl Drafts {
    pub(in crate::kagan) fn save(&mut self, thread: &ThreadId, note: DiffNote) {
        self.0.entry(thread.clone()).or_default().saved.push(note);
    }

    #[cfg(test)]
    pub(in crate::kagan) fn count(&self, thread: &ThreadId) -> usize {
        self.0.get(thread).map_or(0, |notes| notes.saved.len())
    }
    pub(in crate::kagan) fn active(
        &self,
        thread: &ThreadId,
        scope: DiffScope,
        path: &str,
    ) -> Option<usize> {
        self.0
            .get(thread)?
            .pending
            .iter()
            .rposition(|draft| draft.note.scope == scope && draft.note.path == path)
    }
}

fn line_label(note: &DiffNote) -> String {
    let lines = if note.start == note.end {
        note.start.to_string()
    } else {
        format!("{}–{}", note.start, note.end)
    };
    if note.side == DiffSide::Old {
        format!("old {lines}")
    } else {
        lines
    }
}

impl Kagan {
    pub(super) fn begin_comment(
        &mut self,
        side: DiffSide,
        start: u32,
        end: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(review) = self.screen.review_mut() else {
            return;
        };
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Write a comment")
                .submit_on_enter(true)
        });
        let watch = cx.subscribe_in(&input, window, |this, _, event, window, cx| {
            if matches!(
                event,
                InputEvent::PressEnter {
                    shift: false,
                    secondary: false
                }
            ) {
                this.add_comment(window, cx);
            }
        });
        window.focus(&input.focus_handle(cx), cx);
        let notes = self
            .review_drafts
            .0
            .entry(review.thread.clone())
            .or_default();
        review.draft = Some(notes.pending.len());
        notes.pending.push(Draft {
            input,
            _watch: watch,
            note: DiffNote {
                scope: review.scope,
                rev: (review.scope == DiffScope::Commit).then(|| "HEAD".to_owned()),
                path: review.path.clone(),
                side,
                start,
                end,
                text: String::new(),
            },
        });
        cx.notify();
    }

    pub(super) fn add_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.screen.review_mut() else {
            return;
        };
        let Some(index) = review.draft else {
            return;
        };
        let Some(notes) = self.review_drafts.0.get_mut(&review.thread) else {
            return;
        };
        let Some(draft) = notes.pending.get(index) else {
            return;
        };
        let text = draft.input.read(cx).value().trim().to_owned();
        if text.is_empty() {
            return;
        }
        let mut note = notes.pending.remove(index).note;
        note.text = text;
        notes.saved.push(note);
        review.draft = None;
        window.focus(&review.focus, cx);
        cx.notify();
    }

    pub(super) fn send_review(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.screen.review_mut() else {
            return;
        };
        let notes = self
            .review_drafts
            .0
            .get(&review.thread)
            .map(|notes| notes.saved.clone())
            .unwrap_or_default();
        if notes.is_empty() || review.sending {
            return;
        }
        review.sending = true;
        review.send_error = None;
        let thread = review.thread.clone();
        let window = review.window;
        let task = Workspace::dispatch(
            Op::Thread(ThreadOp::Paste {
                id: thread.clone(),
                text: Some(clean(&format_notes(&notes))),
                file: None,
                terminal: None,
            }),
            cx,
        );
        review.send = cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |this, cx| {
                    let Some(review) = this.screen.review_mut() else {
                        return;
                    };
                    if review.thread != thread {
                        return;
                    }
                    review.sending = false;
                    match result {
                        Ok(_) => {
                            if let Some(saved) = this.review_drafts.0.get_mut(&thread)
                                && saved.saved == notes
                            {
                                saved.saved.clear();
                            }
                            this.close_review(window, cx);
                        }
                        Err(error) => {
                            review.send_error = Some(format!("Could not paste review: {error}"));
                        }
                    }
                    cx.notify();
                });
            });
        });
        cx.notify();
    }

    pub(super) fn inline_comments(
        &self,
        side: DiffSide,
        number: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(review) = self.screen.review() else {
            return div().into_any_element();
        };
        let Some(notes) = self.review_drafts.0.get(&review.thread) else {
            return div().into_any_element();
        };
        let matching = |note: &DiffNote| {
            note.scope == review.scope
                && note.path == review.path
                && note.side == side
                && note.end == number
        };
        let draft = review
            .draft
            .and_then(|index| notes.pending.get(index))
            .filter(|draft| matching(&draft.note));
        if draft.is_none() && !notes.saved.iter().any(matching) {
            return div().into_any_element();
        }
        let mut panel = v_flex()
            .relative()
            .left(-review.scroll.offset().x)
            .w(review.scroll.bounds().size.width)
            .pl(super::page::GUTTER)
            .gap_1();
        let mut list = v_flex();
        for (index, note) in notes.saved.iter().enumerate().filter(|(_, note)| {
            note.scope == review.scope
                && note.path == review.path
                && note.side == side
                && note.end == number
        }) {
            let label = line_label(note);
            list = list.child(
                h_flex()
                    .h(COMMENT_ROW)
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .child(format!("{label}: {}", note.text)),
                    )
                    .child(super::super::shown::control(
                        gpui_kit::ElementId::Name(format!("review-remove-{index}").into()),
                        Facts::button("Remove comment"),
                        Button::new(gpui_kit::ElementId::Name(
                            format!("button-review-remove-{index}").into(),
                        ))
                        .ghost()
                        .small()
                        .icon(IconName::X)
                        .accessibility_label("Remove comment")
                        .tooltip("Remove comment")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(review) = this.screen.review()
                                && let Some(notes) = this.review_drafts.0.get_mut(&review.thread)
                                && index < notes.saved.len()
                            {
                                notes.saved.remove(index);
                            }
                            cx.notify();
                        })),
                    )),
            );
        }
        panel = panel.child(
            div()
                .max_h(COMMENT_ROW * 3.)
                .ui_container()
                .overflow_y_scrollbar()
                .id(gpui_kit::ElementId::Name(
                    format!("review-comment-list-{side:?}-{number}").into(),
                ))
                .child(list),
        );
        if let Some(draft) = draft {
            panel = panel.child(Self::comment_draft(draft, cx));
        }
        panel.into_any_element()
    }

    fn comment_draft(draft: &Draft, cx: &mut Context<Self>) -> AnyElement {
        let input = &draft.input;
        let label = line_label(&draft.note);
        v_flex()
            .gap_1()
            .child(
                div()
                    .type_caption()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "Comment on line {label} for the agent · Enter to add"
                    )),
            )
            .child(
                div()
                    .id("review-comment-input")
                    .ui_observe_as(Facts {
                        role: gpui_kit::Role::TextInput,
                        value: Some(input.read(cx).value().to_string().into()),
                        focus: Some(input.focus_handle(cx)),
                        ..Facts::button("Comment")
                    })
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        Textarea::new(input)
                            .text_size(gpui_kit::px(crate::theme::TYPE_BODY))
                            .small(),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(super::super::shown::control(
                        "review-cancel-comment",
                        Facts::button("Cancel"),
                        Button::new("button-review-cancel-comment")
                            .outline()
                            .small()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.cancel_comment(window, cx);
                            })),
                    ))
                    .child(super::super::shown::control(
                        "review-add-comment",
                        Facts::button("Add comment"),
                        Button::new("button-review-add-comment")
                            .outline()
                            .small()
                            .label("Add comment")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_comment(window, cx);
                            })),
                    )),
            )
            .into_any_element()
    }

    pub(super) fn cancel_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(review) = self.screen.review_mut()
            && let Some(index) = review.draft.take()
            && let Some(notes) = self.review_drafts.0.get_mut(&review.thread)
            && index < notes.pending.len()
        {
            notes.pending.remove(index);
        }
        self.focus_review(window, cx);
        cx.notify();
    }

    pub(super) fn draft_focused(&self, window: &Window, cx: &Context<Self>) -> bool {
        self.screen
            .review()
            .and_then(|review| {
                self.review_drafts
                    .0
                    .get(&review.thread)
                    .and_then(|notes| review.draft.and_then(|index| notes.pending.get(index)))
            })
            .is_some_and(|draft| draft.input.focus_handle(cx).is_focused(window))
    }

    fn undo_discard(cx: &mut Context<Self>) -> AnyElement {
        super::super::shown::control(
            "review-undo-discard",
            Facts::button("Undo discard"),
            Button::new("button-review-undo-discard")
                .outline()
                .small()
                .label("Undo")
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(review) = this.screen.review()
                        && let Some(notes) = this.review_drafts.0.get_mut(&review.thread)
                    {
                        notes.saved.append(&mut notes.discarded);
                    }
                    cx.notify();
                })),
        )
        .into_any_element()
    }

    pub(super) fn review_comments(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(review) = self.screen.review() else {
            return div().into_any_element();
        };
        let mut panel = v_flex()
            .border_t_1()
            .border_color(cx.theme().border)
            .gap_1();
        let notes = self.review_drafts.0.get(&review.thread);
        if let Some(error) = &review.send_error {
            panel = panel.child(
                div()
                    .px_3()
                    .pt_1()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        let count = notes.map_or(0, |notes| notes.saved.len());
        let files = notes.map_or(0, |notes| {
            notes
                .saved
                .iter()
                .map(|note| &note.path)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        });
        let hint = if count == 0 {
            "Hover a line and click + to comment. Drag line numbers to pick several.".to_owned()
        } else {
            format!(
                "{count} {} in {files} {}",
                if count == 1 { "comment" } else { "comments" },
                if files == 1 { "file" } else { "files" }
            )
        };
        let can_undo = notes.is_some_and(|notes| !notes.discarded.is_empty());
        panel
            .child(
                v_flex()
                    .flex_none()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .py_2()
                    .gap_1()
                    .child(
                        div()
                            .id("review-comment-hint")
                            .ui_container()
                            .aria_label(hint.clone())
                            .type_caption()
                            .text_color(cx.theme().muted_foreground)
                            .child(hint),
                    )
                    .child(Self::review_footer_actions(count, can_undo, cx)),
            )
            .into_any_element()
    }

    fn review_footer_actions(count: usize, can_undo: bool, cx: &mut Context<Self>) -> AnyElement {
        let undo = Self::undo_discard(cx);
        let discard = super::super::shown::control(
            "review-discard",
            Facts {
                disabled: count == 0,
                ..Facts::button("Discard comments")
            },
            Button::new("button-review-discard")
                .outline()
                .small()
                .label("Discard comments")
                .tooltip("Discard review comments")
                .disabled(count == 0)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(review) = this.screen.review()
                        && let Some(notes) = this.review_drafts.0.get_mut(&review.thread)
                    {
                        notes.discarded = std::mem::take(&mut notes.saved);
                    }
                    cx.notify();
                })),
        );
        let send = super::super::shown::control(
            "review-send",
            Facts {
                disabled: count == 0,
                ..Facts::button("Send review to terminal")
            },
            Button::new("button-review-send")
                .small()
                .map(|button| {
                    if count == 0 {
                        button.outline()
                    } else {
                        button.primary()
                    }
                })
                .label(if count == 0 {
                    "Send review to terminal".to_owned()
                } else {
                    format!("Send review to terminal ({count})")
                })
                .disabled(count == 0)
                .tooltip("Paste review comments into this thread's terminal")
                .on_click(cx.listener(|this, _, _, cx| this.send_review(cx))),
        );
        h_flex()
            .gap_2()
            .justify_end()
            .when(can_undo, |row| row.child(undo))
            .child(discard)
            .child(send)
            .into_any_element()
    }
}

fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
