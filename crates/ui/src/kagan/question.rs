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
use kg_domain::{
    InteractionOption, OptionRole, PendingInteraction, PendingInteractionId, ThreadId,
};

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
    body(
        question,
        form,
        None,
        Some(kagan),
        &failed(kagan),
        window,
        cx,
    )
}

/// The same card in the transcript: the agent's question with the one line that says what it
/// wants, and nowhere to put the error but the transcript's own line.
pub(super) fn permission_card(
    asked: &PendingInteraction,
    detail: Option<AnyElement>,
    failed: &Failed,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    body(
        Question::thread(asked),
        None,
        detail,
        None,
        failed,
        window,
        cx,
    )
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
    body(
        Question::thread(asked),
        form,
        None,
        Some(kagan),
        &failed(kagan),
        window,
        cx,
    )
}

/// Where an answer's failure goes: the window's note, or the transcript's line.
pub(super) type Failed = Rc<dyn Fn(&mut App, String)>;

fn failed(kagan: &WeakEntity<Kagan>) -> Failed {
    let kagan = kagan.clone();
    Rc::new(move |cx, text| {
        _ = kagan.update(cx, |kagan, cx| {
            kagan.note = Some(text.into());
            cx.notify();
        });
    })
}

pub(super) fn answer_focus(question: &str, option: &str) -> String {
    format!("answer-focus-{question}-{option}")
}

fn body(
    question: Question,
    form: Option<AnyElement>,
    detail: Option<AnyElement>,
    kagan: Option<&WeakEntity<Kagan>>,
    failed: &Failed,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement + use<> {
    let failed = Rc::clone(failed);
    let mut buttons: Vec<AnyElement> = Vec::new();
    for option in &question.options {
        let focus = super::tab_stop(
            window,
            cx,
            ElementId::Name(answer_focus(&question.id.0, &option.id).into()),
        );
        let answer_now: Rc<dyn Fn(&mut App)> = match (
            question.asked.clone(),
            question.thread.clone(),
            kagan.cloned(),
        ) {
            (Some(asked), Some(thread), Some(kagan)) if asked.fields.is_some() => {
                ask_form::answer_callback(thread, asked, option.id.clone(), kagan)
            }
            _ => {
                let (thread, id, choice) = (
                    question.thread.clone(),
                    question.id.clone(),
                    option.id.clone(),
                );
                let failed = Rc::clone(&failed);
                Rc::new(move |cx| {
                    answer(thread.clone(), id.clone(), choice.clone(), cx, &failed);
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
                    button = match option.role {
                        Some(OptionRole::Destructive) => button.danger(),
                        Some(OptionRole::Primary) => button.primary(),
                        _ => button.outline(),
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
        .children(detail)
        .children(form)
        .child(h_flex().gap(px(space(SpaceStep::Sm, cx))).children(buttons))
}

fn answer(
    thread: Option<ThreadId>,
    id: PendingInteractionId,
    option: String,
    cx: &mut App,
    failed: &Failed,
) {
    if crate::synthetic::refuse_for_person() {
        return;
    }
    let answered = match thread {
        Some(thread) => Workspace::answer(&thread, id, option, cx),
        None => Workspace::answer_standalone(id, option, cx),
    };
    let failed = Rc::clone(failed);
    cx.spawn(async move |cx| {
        if let Err(error) = answered.await {
            cx.update(|cx| failed(cx, format!("Could not answer: {error}")));
        }
    })
    .detach();
}
