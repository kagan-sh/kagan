use crate::theme::TypeScale as _;
use std::rc::Rc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, WeakEntity, Window, div,
};
use kg_core::{Approval, Workspace};
use kg_domain::{InteractionOption, PendingInteraction, PendingInteractionId, ThreadId};

use super::Kagan;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

/// A question waiting for the person: on a thread, or a standalone approval.
pub(super) struct Question {
    thread: Option<ThreadId>,
    id: PendingInteractionId,
    title: String,
    options: Vec<InteractionOption>,
}

impl Question {
    pub(super) fn thread(asked: &PendingInteraction) -> Self {
        Self {
            thread: Some(asked.thread_id.clone()),
            id: asked.id.clone(),
            title: asked.title.clone(),
            options: asked.options.clone(),
        }
    }

    pub(super) fn standalone(approval: &Approval) -> Self {
        Self {
            thread: None,
            id: approval.id.clone(),
            title: approval.title.clone(),
            options: approval.options.clone(),
        }
    }
}

pub(super) fn card(
    question: Question,
    kagan: &WeakEntity<Kagan>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let mut buttons: Vec<AnyElement> = Vec::new();
    for option in &question.options {
        // The answer is a focus stop of its own, so `kg ui` can report when it has focus. Its
        // handle is kept by key, so it holds across renders.
        let focus = super::tab_stop(
            window,
            cx,
            ElementId::Name(format!("answer-focus-{}-{}", question.id.0, option.id).into()),
        );
        let answer_now: Rc<dyn Fn(&mut App)> = {
            let (thread, id, choice, kagan) = (
                question.thread.clone(),
                question.id.clone(),
                option.id.clone(),
                kagan.clone(),
            );
            Rc::new(move |cx| answer(thread.clone(), id.clone(), choice.clone(), &kagan, cx))
        };
        let (on_key, on_click) = (answer_now.clone(), answer_now);
        let focused = focus.is_focused(window);
        buttons.push(
            div()
                .id(ElementId::Name(
                    format!("answer-{}-{}", question.id.0, option.id).into(),
                ))
                .ui_observe_as(Facts {
                    focus: Some(focus.clone()),
                    ..Facts::button(option.label.clone())
                })
                .track_focus(&focus)
                .tab_stop(true)
                .rounded(cx.theme().radius)
                .when(focused, |answer| {
                    answer.border_1().border_color(cx.theme().ring)
                })
                .on_key_down(move |event, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        on_key(cx);
                    }
                })
                .child({
                    let mut button = Button::new(SharedString::from(option.id.clone()))
                        .small()
                        .tab_stop(false)
                        .label(option.label.clone());
                    button = if matches!(option.label.as_str(), "Delete" | "Remove") {
                        button.danger()
                    } else {
                        button.outline()
                    };
                    button.on_click(move |_, _, cx| on_click(cx))
                })
                .into_any_element(),
        );
    }
    v_flex()
        .id(ElementId::Name(
            format!("question-{}", question.id.0).into(),
        ))
        .ui_container()
        .p_3()
        .gap_2()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().warning)
        .bg(cx.theme().muted)
        .child(div().type_body().child(question.title))
        .child(h_flex().gap_2().children(buttons))
}

fn answer(
    thread: Option<ThreadId>,
    id: PendingInteractionId,
    option: String,
    kagan: &WeakEntity<Kagan>,
    cx: &mut App,
) {
    // `kg ui` input is synthetic, and only a person answers.
    if crate::synthetic::refuse_for_person() {
        return;
    }
    let answered = match thread {
        Some(thread) => Workspace::answer(&thread, id, option, cx),
        None => Workspace::answer_standalone(id, option, cx),
    };
    let kagan = kagan.clone();
    cx.spawn(async move |cx| {
        if let Err(error) = answered.await {
            _ = kagan.update(cx, |kagan, cx| {
                kagan.note = Some(format!("Could not answer: {error}").into());
                cx.notify();
            });
        }
    })
    .detach();
}
