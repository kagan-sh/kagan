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
    region: &'static str,
}

impl Question {
    pub(super) fn thread(asked: &PendingInteraction) -> Self {
        Self {
            thread: Some(asked.thread_id.clone()),
            id: asked.id.clone(),
            title: asked.title.clone(),
            options: asked.options.clone(),
            asked: Some(asked.clone()),
            region: "",
        }
    }

    pub(super) fn standalone(approval: &Approval) -> Self {
        Self {
            thread: None,
            id: approval.id.clone(),
            title: approval.title.clone(),
            options: approval.options.clone(),
            asked: None,
            region: "",
        }
    }

    fn in_the_transcript(asked: &PendingInteraction) -> Self {
        Self {
            region: "chat-",
            ..Self::thread(asked)
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

pub(super) fn permission_card(
    asked: &PendingInteraction,
    detail: Option<AnyElement>,
    failed: &Failed,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    body(
        Question::in_the_transcript(asked),
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

// The option a card opens on: for a permission, the one that grants nothing.
pub(super) fn opening_option(
    options: &[InteractionOption],
    permission: bool,
) -> Option<&InteractionOption> {
    if permission {
        return options
            .iter()
            .find(|option| option.role == Some(OptionRole::Safe))
            .or_else(|| {
                options
                    .iter()
                    .find(|option| option.role == Some(OptionRole::Destructive))
            });
    }
    options
        .iter()
        .find(|option| option.role == Some(OptionRole::Primary))
        .or_else(|| {
            options
                .iter()
                .find(|option| option.role == Some(OptionRole::Safe))
        })
        .or_else(|| options.first())
}

impl Question {
    fn focus_key(&self, option: &str) -> String {
        answer_focus(&format!("{}{}", self.region, self.id.0), option)
    }

    fn element(&self, kind: &str, option: &str) -> String {
        format!("{kind}-{}{}-{option}", self.region, self.id.0)
    }

    fn asks_permission(&self) -> bool {
        self.asked
            .as_ref()
            .is_none_or(|asked| asked.fields.is_none())
    }
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
    for option in &ordered(&question.options, question.asks_permission()) {
        let focus = super::tab_stop(
            window,
            cx,
            ElementId::Name(question.focus_key(&option.id).into()),
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
                    question.element("answer", &option.id).into(),
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
                    let mut button = Button::new(SharedString::from(
                        question.element("answer-button", &option.id),
                    ))
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
            format!("question-{}{}", question.region, question.id.0).into(),
        ))
        .ui_container()
        .p(px(space(SpaceStep::Md, cx)))
        .gap(px(space(SpaceStep::Sm, cx)))
        .rounded(px(crate::theme::radius_lg(cx)))
        .border_1()
        .border_color(cx.theme().warning)
        .bg(cx.theme().muted)
        .child(
            div()
                .id(ElementId::Name(
                    format!("question-title-{}{}", question.region, question.id.0).into(),
                ))
                .ui_observe_as(Facts::label("Question", question.title.clone()))
                .type_body(cx)
                .child(question.title),
        )
        .children(detail)
        .children(form)
        .child(h_flex().gap(px(space(SpaceStep::Sm, cx))).children(buttons))
}

fn ordered(options: &[InteractionOption], permission: bool) -> Vec<InteractionOption> {
    if !permission {
        return options.to_vec();
    }
    let rank = |role: Option<OptionRole>| match role {
        Some(OptionRole::Destructive) => 0,
        Some(OptionRole::Primary) => 2,
        _ => 1,
    };
    let mut ordered = options.to_vec();
    ordered.sort_by_key(|option| rank(option.role));
    ordered
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

#[cfg(test)]
#[path = "question_test.rs"]
mod tests;
