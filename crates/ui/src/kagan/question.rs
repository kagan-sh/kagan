use crate::theme::{SpaceStep, TypeScale as _, space};
use std::rc::Rc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, WeakEntity, Window, div, px,
};
use kg_core::{Approval, Workspace};
use kg_domain::{InteractionOption, PendingInteraction, PendingInteractionId, ThreadId};

use super::Kagan;
use super::ask_form::{self, Draft};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) struct Question {
    thread: Option<ThreadId>,
    id: PendingInteractionId,
    title: String,
    options: Vec<InteractionOption>,
    asked: Option<PendingInteraction>,
}

impl Question {
    pub(super) fn thread(asked: &PendingInteraction) -> Self {
        Self {
            thread: Some(asked.thread_id.clone()),
            id: asked.id.clone(),
            title: asked.title.clone(),
            options: asked.options.clone(),
            asked: Some(asked.clone()),
        }
    }

    pub(super) fn standalone(approval: &Approval) -> Self {
        Self {
            thread: None,
            id: approval.id.clone(),
            title: approval.title.clone(),
            options: approval.options.clone(),
            asked: None,
        }
    }
}

pub(super) fn card(
    question: Question,
    draft: Option<&Draft>,
    entity: &Entity<Kagan>,
    kagan: &WeakEntity<Kagan>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let form = question.asked.as_ref().and_then(|asked| {
        if asked.fields.as_ref().is_none_or(Vec::is_empty) {
            return None;
        }
        let draft = draft?;
        Some(ask_form::fields(asked, draft, entity, window, cx).into_any_element())
    });
    body(question, form, kagan, window, cx)
}

pub(super) fn form_card(
    asked: &PendingInteraction,
    draft: &Draft,
    entity: &Entity<Kagan>,
    kagan: &WeakEntity<Kagan>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let form = asked
        .fields
        .as_ref()
        .is_some_and(|fields| !fields.is_empty())
        .then(|| ask_form::fields(asked, draft, entity, window, cx).into_any_element());
    body(Question::thread(asked), form, kagan, window, cx)
}

pub(super) fn answer_focus(question: &str, option: &str) -> String {
    format!("answer-focus-{question}-{option}")
}

fn body(
    question: Question,
    form: Option<AnyElement>,
    kagan: &WeakEntity<Kagan>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let mut buttons: Vec<AnyElement> = Vec::new();
    for option in &question.options {
        let focus = super::tab_stop(
            window,
            cx,
            ElementId::Name(answer_focus(&question.id.0, &option.id).into()),
        );
        let answer_now: Rc<dyn Fn(&mut App)> =
            match (question.asked.clone(), question.thread.clone()) {
                (Some(asked), Some(thread)) if asked.fields.is_some() => {
                    ask_form::answer_callback(thread, asked, option.id.clone(), kagan.clone())
                }
                _ => {
                    let (thread, id, choice, kagan) = (
                        question.thread.clone(),
                        question.id.clone(),
                        option.id.clone(),
                        kagan.clone(),
                    );
                    Rc::new(move |cx| {
                        answer(thread.clone(), id.clone(), choice.clone(), &kagan, cx);
                    })
                }
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
        .p(px(space(SpaceStep::Md, cx)))
        .gap(px(space(SpaceStep::Sm, cx)))
        .rounded(px(crate::theme::radius_lg(cx)))
        .border_1()
        .border_color(cx.theme().warning)
        .bg(cx.theme().muted)
        .child(div().type_body(cx).child(question.title))
        .children(form)
        .child(h_flex().gap(px(space(SpaceStep::Sm, cx))).children(buttons))
}

fn answer(
    thread: Option<ThreadId>,
    id: PendingInteractionId,
    option: String,
    kagan: &WeakEntity<Kagan>,
    cx: &mut App,
) {
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
