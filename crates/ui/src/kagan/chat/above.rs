use std::collections::BTreeSet;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    Role, SharedString, Styled as _, WeakEntity, Window, div, px,
};
use kg_domain::PlanStatus;

use super::ChatView;
use super::body;
use super::kind;
use super::work;
use crate::theme::{Tone, TypeScale as _, control_height};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

#[derive(Default)]
pub(super) struct Cards {
    pub(super) open: BTreeSet<&'static str>,
}

pub(super) fn stack(
    view: &ChatView,
    window: &mut Window,
    cx: &mut Context<ChatView>,
) -> Vec<AnyElement> {
    let mut cards = Vec::new();
    if !view.children.is_empty() {
        let open = view.cards.open.contains("chat-card-children");
        let body = open.then(|| children_body(view, cx));
        cards.push(
            card(
                Head {
                    id: "chat-card-children",
                    icon: IconName::Users,
                    name: "Active child threads",
                    detail: view.children.len().to_string().into(),
                    open,
                },
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
        let open = view.cards.open.contains("chat-card-plan");
        let body = open.then(|| {
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
                Head {
                    id: "chat-card-plan",
                    icon: IconName::ListChecks,
                    name: "Plan",
                    detail: detail.into(),
                    open,
                },
                body,
                window,
                cx,
            )
            .into_any_element(),
        );
    }
    cards
}

fn children_body(view: &ChatView, cx: &App) -> AnyElement {
    let rows: Vec<AnyElement> = view
        .children
        .iter()
        .map(|(id, title)| {
            h_flex()
                .id(ElementId::Name(format!("chat-card-child-{}", id.0).into()))
                .h(px(control_height(cx)))
                .gap_2()
                .px_2()
                .items_center()
                .type_small(cx)
                .ui_observe_as(Facts::label("Child thread", title.clone()))
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

struct Head {
    id: &'static str,
    icon: IconName,
    name: &'static str,
    detail: SharedString,
    open: bool,
}

fn card(
    head: Head,
    body: Option<AnyElement>,
    window: &mut Window,
    cx: &mut Context<ChatView>,
) -> gpui_kit::Div {
    let id = ElementId::Name(head.id.into());
    let focus = crate::kagan::tab_stop(window, cx, ElementId::Name(head.id.into()));
    let muted = cx.theme().muted_foreground;
    let value = format!(
        "{}, {}",
        kind::pick(head.open, "expanded", "collapsed"),
        head.detail
    );
    let view = cx.weak_entity();
    let toggle: Rc<dyn Fn(&mut App)> = {
        let view = view.clone();
        Rc::new(move |cx| flip(&view, head.id, cx))
    };
    v_flex()
        .border_1()
        .border_color(cx.theme().border)
        .rounded(cx.theme().radius)
        .bg(cx.theme().secondary)
        .child(
            work::disclosure(id, &focus, head.open, true, toggle, cx)
                .ui_observe_as(Facts {
                    role: Role::Button,
                    name: head.name.into(),
                    value: Some(value.into()),
                    disabled: false,
                    selected: Some(head.open),
                    focus: Some(focus.clone()),
                    hint: None,
                })
                .h(px(control_height(cx)))
                .flex()
                .gap_2()
                .px_2()
                .items_center()
                .border_1()
                .border_color(gpui_kit::transparent_black())
                .text_color(if head.open {
                    cx.theme().foreground
                } else {
                    muted
                })
                .child(Icon::new(head.icon).xsmall().text_color(muted))
                .child(
                    div()
                        .id(ElementId::Name(format!("{}-title", head.id).into()))
                        .ui_container()
                        .type_small(cx)
                        .child(head.name),
                )
                .child(div().flex_1())
                .child(div().type_caption(cx).text_color(muted).child(head.detail)),
        )
        .children(body.map(|body| {
            div()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(body)
        }))
}

fn flip(view: &WeakEntity<ChatView>, id: &'static str, cx: &mut App) {
    _ = view.update(cx, |view, cx| {
        if !view.cards.open.remove(id) {
            view.cards.open.insert(id);
        }
        cx.notify();
    });
}
