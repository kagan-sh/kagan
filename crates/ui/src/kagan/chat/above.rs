use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, ElementId, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, px,
};
use kg_domain::PlanStatus;

use super::ChatView;
use super::body;
use crate::theme::{Tone, TypeScale as _, control_height};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

/// Which of the cards above the prompt box the person has opened. Both start closed, one line
/// each, so the box keeps the room.
#[derive(Default)]
pub(super) struct Cards {
    pub(super) children: bool,
    pub(super) plan: bool,
}

/// What kagan stacks over the prompt box: the child threads that are working, and the agent's plan
/// while it has one. Each is one line with a count until the person opens it.
pub(super) fn stack(
    view: &ChatView,
    window: &mut Window,
    cx: &mut Context<ChatView>,
) -> Vec<AnyElement> {
    let mut cards = Vec::new();
    if !view.children.is_empty() {
        let body = view.cards.children.then(|| children_body(view, cx));
        cards.push(
            card(
                "chat-card-children",
                IconName::Users,
                "Active child threads",
                Some(view.children.len().to_string().into()),
                view.cards.children,
                cx.listener(|this, _, _, cx| {
                    this.cards.children = !this.cards.children;
                    cx.notify();
                }),
                cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        this.cards.children = !this.cards.children;
                        cx.notify();
                    }
                }),
                body,
                window,
                cx,
            )
            .into_any_element(),
        );
    }
    if let Some((id, entries)) = view.transcript.plan() {
        let done = entries
            .iter()
            .filter(|entry| entry.status == PlanStatus::Completed)
            .count();
        let detail = format!("{done} of {} done", entries.len());
        let body = view.cards.plan.then(|| {
            div()
                .id("chat-card-plan-body")
                .px_2()
                .pb_2()
                .ui_container()
                .child(body::plan_body(id, entries, cx))
                .into_any_element()
        });
        cards.push(
            card(
                "chat-card-plan",
                IconName::ListChecks,
                "Plan",
                Some(detail.into()),
                view.cards.plan,
                cx.listener(|this, _, _, cx| {
                    this.cards.plan = !this.cards.plan;
                    cx.notify();
                }),
                cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        this.cards.plan = !this.cards.plan;
                        cx.notify();
                    }
                }),
                body,
                window,
                cx,
            )
            .into_any_element(),
        );
    }
    cards
}

/// The running child threads, each with its spinner and its title.
fn children_body(view: &ChatView, cx: &App) -> AnyElement {
    let rows: Vec<AnyElement> = view
        .children
        .iter()
        .map(|(id, title)| {
            h_flex()
                .id(ElementId::Name(format!("chat-card-child-{}", id.0).into()))
                .h(px(control_height(cx)))
                .gap(px(6.))
                .px_2()
                .items_center()
                .type_small(cx)
                .ui_observe_as(Facts {
                    role: Role::Label,
                    name: "Child thread".into(),
                    value: Some(title.clone()),
                    disabled: false,
                    selected: None,
                    focus: None,
                    hint: None,
                })
                .child(Tone::Busy.mark(cx))
                .child(div().min_w_0().text_ellipsis().child(title.clone()))
                .into_any_element()
        })
        .collect();
    v_flex()
        .id("chat-card-children-body")
        .pb_1()
        .ui_container()
        .children(rows)
        .into_any_element()
}

/// A card is one line — chevron, icon, name, count — and, when open, its body under a hairline.
fn card(
    id: &'static str,
    icon: IconName,
    name: &'static str,
    detail: Option<SharedString>,
    open: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_key: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    body: Option<AnyElement>,
    window: &mut Window,
    cx: &mut App,
) -> gpui_kit::Div {
    let focus = crate::kagan::tab_stop(window, cx, ElementId::Name(id.into()));
    let focused = focus.is_focused(window);
    let muted = cx.theme().muted_foreground;
    let value = match &detail {
        Some(detail) => format!("{}, {detail}", pick(open)),
        None => pick(open).to_owned(),
    };
    v_flex()
        .border_1()
        .border_color(cx.theme().border)
        .rounded(cx.theme().radius)
        .bg(cx.theme().secondary)
        .child(
            h_flex()
                .id(id)
                .ui_observe_as(Facts {
                    role: Role::Button,
                    name: name.into(),
                    value: Some(value.into()),
                    disabled: false,
                    selected: Some(open),
                    focus: Some(focus.clone()),
                    hint: None,
                })
                .track_focus(&focus)
                .h(px(control_height(cx)))
                .gap(px(6.))
                .px_2()
                .items_center()
                .text_color(if open { cx.theme().foreground } else { muted })
                .hover(|row| row.bg(cx.theme().list_hover))
                .child(
                    Icon::new(if open {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .xsmall()
                    .text_color(muted),
                )
                .child(Icon::new(icon).xsmall().text_color(muted))
                .child(div().type_small(cx).child(name))
                .child(div().flex_1())
                .children(
                    detail.map(|detail| div().type_caption(cx).text_color(muted).child(detail)),
                )
                .when(focused, |row| row.border_1().border_color(cx.theme().ring))
                .on_click(on_click)
                .on_key_down(on_key),
        )
        .children(body.map(|body| {
            div()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(body)
        }))
}

fn pick(open: bool) -> &'static str {
    if open { "expanded" } else { "collapsed" }
}
