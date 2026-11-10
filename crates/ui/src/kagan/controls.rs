use gpui_kit::component::button::{Button, Toggle};
use gpui_kit::component::h_flex;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, Anchor, AnyElement, App, ClickEvent, Context, ElementId, Entity, FocusHandle, Hsla,
    InteractiveElement, IntoElement, Keystroke, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, anchored, deferred, div, px,
};
use kg_domain::PluginId;

use super::Kagan;
use crate::theme::{SpaceStep, TypeScale as _, space};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

// A small muted icon for a state. Its label is the accessible name and the tooltip.
pub(super) fn state_mark(
    id: String,
    icon: gpui_kit::assets::IconName,
    label: &'static str,
    cx: &App,
) -> AnyElement {
    div()
        .id(ElementId::Name(id.into()))
        .w_4()
        .h(px(16.))
        .flex()
        .flex_shrink_0()
        .justify_center()
        .items_center()
        .ui_container()
        .aria_label(label)
        .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
        .text_color(cx.theme().muted_foreground)
        .child(gpui_kit::component::Icon::new(icon).small())
        .into_any_element()
}

pub(super) fn menu_item(id: &'static str, label: &'static str) -> impl IntoElement {
    control(
        id,
        Facts {
            role: Role::MenuItem,
            ..Facts::button(label)
        },
        label,
    )
}

pub(super) fn control(
    id: impl Into<gpui_kit::ElementId>,
    facts: Facts,
    child: impl IntoElement,
) -> impl IntoElement {
    div().id(id).ui_observe_as(facts).child(child)
}

pub(super) fn segments(cx: &App) -> gpui_kit::Div {
    h_flex()
        .self_start()
        .h(px(crate::theme::control_height(cx)))
        .p_0p5()
        .border_1()
        .border_color(cx.theme().border)
        .rounded(cx.theme().radius)
        .bg(cx.theme().tab_bar_segmented)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Selected {
    No,
    Yes,
}

pub(super) fn segment(id: impl Into<gpui_kit::ElementId>, selected: Selected, cx: &App) -> Toggle {
    let selected = matches!(selected, Selected::Yes);
    Toggle::new(id)
        .checked(selected)
        .h_5()
        .px(px(space(SpaceStep::Sm, cx)))
        .type_small(cx)
        .font_weight(gpui_kit::FontWeight::MEDIUM)
        .rounded(cx.theme().radius / 2.)
        .text_color(if selected {
            cx.theme().foreground
        } else {
            cx.theme().muted_foreground
        })
        .when(selected, |button| button.bg(cx.theme().secondary))
}

pub(super) fn note(
    id: &'static str,
    text: impl Into<SharedString>,
    color: Hsla,
    cx: &App,
) -> AnyElement {
    let text = text.into();
    div()
        .id(id)
        .ui_container()
        .aria_label(text.clone())
        .type_small(cx)
        .text_color(color)
        .child(text)
        .into_any_element()
}

// The menu, under the button it is wrapped in: its top right corner sits on the button's bottom
// right corner. The zero-size holder is placed there, because an anchored child on its own sits at
// the button's top left.
pub(super) fn menu_popup(
    menu: &Entity<PopupMenu>,
    layer: &'static str,
    id: &'static str,
    label: &'static str,
) -> impl IntoElement {
    div()
        .absolute()
        .bottom_0()
        .right_0()
        .w_0()
        .h_0()
        .child(deferred(
            anchored().anchor(Anchor::TopRight).child(
                div()
                    .id(layer)
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(control(
                        id,
                        Facts {
                            role: Role::Menu,
                            ..Facts::button(label)
                        },
                        menu.clone(),
                    )),
            ),
        ))
}

pub(super) fn dispatch(
    action: impl Action,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx)
}

pub(super) fn bound_strokes(action: &dyn Action, cx: &App) -> Vec<Keystroke> {
    cx.key_bindings()
        .borrow()
        .bindings()
        .find(|binding| binding.action().partial_eq(action))
        .map(|binding| {
            binding
                .keystrokes()
                .iter()
                .map(|stroke| stroke.inner().clone())
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn bound_keys(action: &dyn Action, cx: &App) -> String {
    keys_text(&bound_strokes(action, cx))
}

pub(super) fn keys_text<'a>(strokes: impl IntoIterator<Item = &'a Keystroke>) -> String {
    strokes
        .into_iter()
        .map(Kbd::format)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn tip(label: &str, action: &dyn Action, cx: &App) -> String {
    let keys = bound_keys(action, cx);
    if keys.is_empty() {
        label.to_owned()
    } else {
        format!("{label} ({keys})")
    }
}

pub(super) fn act<A: Action + Clone>(
    name: impl Into<String>,
    label: &'static str,
    focus: FocusHandle,
    action: A,
) -> AnyElement {
    let name = name.into();
    control(
        name.clone(),
        Facts::button(label),
        Button::new(ElementId::Name(format!("button-{name}").into()))
            .small()
            .outline()
            .label(label)
            .on_click(move |_, window, cx| {
                window.focus(&focus, cx);
                window.dispatch_action(Box::new(action.clone()), cx);
            }),
    )
    .into_any_element()
}

// A key press keeps focus on the button; a mouse press moves focus to `focus`, because a press
// does not focus a button.
pub(super) fn send<A: Action + Clone>(
    name: impl Into<String>,
    label: &'static str,
    focus: &FocusHandle,
    action: A,
) -> AnyElement {
    let name = name.into();
    let focus = focus.clone();
    control(
        name.clone(),
        Facts::button(label),
        Button::new(ElementId::Name(format!("button-{name}").into()))
            .small()
            .outline()
            .label(label)
            .on_click(move |event, window, cx| {
                if matches!(event, ClickEvent::Mouse(_)) {
                    window.focus(&focus, cx);
                }
                window.dispatch_action(Box::new(action.clone()), cx);
            }),
    )
    .into_any_element()
}

pub(super) fn answer<A: Action, E: InteractiveElement>(
    row: E,
    id: &PluginId,
    cx: &mut Context<Kagan>,
    run: fn(&mut Kagan, &PluginId, &mut Context<Kagan>),
) -> E {
    let id = id.clone();
    row.on_action(cx.listener(move |this, _: &A, _, cx| run(this, &id, cx)))
}
