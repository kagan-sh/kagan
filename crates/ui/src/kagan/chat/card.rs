use std::rc::Rc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div, px,
};
use kg_core::Workspace;
use kg_domain::PendingInteraction;

use crate::kagan::attention;
use crate::kagan::chat::ChatView;
use crate::kagan::chat::kind;
use crate::kagan::chat::work;
use crate::kagan::question;
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

impl ChatView {
    // The permission this thread waits on, and how many wait behind it, oldest first.
    pub(super) fn asked(&self, cx: &App) -> Option<(PendingInteraction, usize)> {
        let attention = attention::global(cx);
        let mut waiting = attention.read(cx).questions.iter().filter(|asked| {
            asked.thread_id == self.thread && asked.fields.as_ref().is_none_or(Vec::is_empty)
        });
        let first = waiting.next()?.clone();
        Some((first, 1 + waiting.count()))
    }

    // A card never takes the keyboard from the draft; it gives the box back its focus when it
    // goes, because the box is where the next thing happens.
    pub(super) fn card_shown(&mut self) {
        self.asking = true;
    }

    pub(super) fn question_left(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.asked(cx).is_none() && std::mem::take(&mut self.asking) {
            // An embed never takes the keyboard: the person may be typing in another chat.
            if self.place != super::Place::Center {
                return;
            }
            let focus = self.prompt.read(cx).focus_handle(cx);
            cx.defer_in(window, move |_, window, cx| window.focus(&focus, cx));
        }
    }

    pub(super) fn approval_detail(
        &self,
        id: &kg_domain::PendingInteractionId,
        cx: &App,
    ) -> (Option<String>, bool) {
        let Some(tool) = self.transcript.approval(id) else {
            return (None, false);
        };
        let environment = Workspace::global(cx)
            .read(cx)
            .thread(&self.thread)
            .is_some_and(|thread| thread.read(cx).record().environment_id.is_some());
        (
            kind::title_of(tool),
            runs_in_worktree(tool.call.kind, environment),
        )
    }
}

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
