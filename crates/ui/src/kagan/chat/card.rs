use std::rc::Rc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div, px,
};

use crate::kagan::chat::ChatView;
use crate::kagan::chat::work;
use crate::kagan::question;
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) fn approval_card(
    asked: &kg_domain::PendingInteraction,
    tool_title: Option<&str>,
    worktree: bool,
    weak: &WeakEntity<ChatView>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let detail = detail(&asked.id, tool_title, worktree, cx);
    let failed: question::Failed = {
        let weak = weak.clone();
        Rc::new(move |cx, text| {
            _ = weak.update(cx, |this, cx| this.say(text, cx));
        })
    };
    div()
        .id(work::named("approval", &asked.id.0))
        .child(question::permission_card(
            asked,
            Some(detail),
            &failed,
            window,
            cx,
        ))
        .ui_container()
        .into_any_element()
}

pub(super) fn runs_in_worktree(kind: Option<kg_domain::ToolKind>, environment: bool) -> bool {
    environment
        && matches!(
            kind,
            Some(kg_domain::ToolKind::Execute | kg_domain::ToolKind::Edit)
        )
}

fn detail(
    id: &kg_domain::PendingInteractionId,
    tool_title: Option<&str>,
    worktree: bool,
    cx: &mut App,
) -> gpui_kit::AnyElement {
    let mono = cx.theme().mono_font_family.clone();
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .when_some(tool_title.map(str::to_owned), |detail, title| {
            let tip = title.clone();
            let fact = Facts {
                role: gpui_kit::Role::Label,
                ..Facts::button(title.clone())
            };
            detail.child(
                div()
                    .id(gpui_kit::ElementId::Name(
                        format!("chat-approval-value-{}", id.0).into(),
                    ))
                    .ui_observe_as(fact)
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().input)
                    .font_family(mono)
                    .type_small(cx)
                    .text_color(cx.theme().foreground)
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .child(title),
            )
        })
        .when(worktree, |detail| {
            detail.child(
                div()
                    .id(gpui_kit::ElementId::Name(
                        format!("chat-approval-runs-{}", id.0).into(),
                    ))
                    .ui_observe_as(Facts {
                        role: gpui_kit::Role::Label,
                        ..Facts::button("It runs in this thread's worktree.")
                    })
                    .type_caption(cx)
                    .text_color(cx.theme().muted_foreground)
                    .child("It runs in this thread's worktree."),
            )
        })
        .into_any_element()
}

#[cfg(test)]
#[path = "card_test.rs"]
mod tests;
