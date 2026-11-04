use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Toggle;
use gpui_kit::component::h_flex;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, Anchor, AnyElement, App, ClickEvent, Entity, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, anchored, deferred, div,
};

use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

/// A thread actions menu item `kg ui` can find and activate by id, named as it shows.
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

/// A container `kg ui` can find by id. It has no role or state of its own.
pub(super) fn container(id: &'static str, child: impl IntoElement) -> impl IntoElement {
    div().id(id).ui_container().child(child)
}

/// A control `kg ui` can find by id. Its facts are required: see `Facts`.
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
        .h_6()
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
        .px_2()
        .type_small()
        .font_weight(gpui_kit::FontWeight::MEDIUM)
        .rounded(cx.theme().radius / 2.)
        .text_color(if selected {
            cx.theme().foreground
        } else {
            cx.theme().muted_foreground
        })
        .when(selected, |button| button.bg(cx.theme().secondary))
}

pub(super) fn note(id: &'static str, text: impl Into<SharedString>, color: Hsla) -> AnyElement {
    let text = text.into();
    div()
        .id(id)
        .ui_container()
        .aria_label(text.clone())
        .type_small()
        .text_color(color)
        .child(text)
        .into_any_element()
}

/// The menu, under the button it is wrapped in: its top right corner sits on the button's bottom
/// right corner. The zero-size holder is placed there, because an anchored child on its own sits at
/// the button's top left.
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

pub(super) fn bound_keys(action: &dyn Action, cx: &App) -> String {
    cx.key_bindings()
        .borrow()
        .bindings()
        .find(|binding| binding.action().name() == action.name())
        .map(|binding| {
            binding
                .keystrokes()
                .iter()
                .map(|key| Kbd::format(key.inner()))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

/// A tooltip with the action's live key, so the label always matches the keymap.
pub(super) fn tip(label: &str, action: &dyn Action, cx: &App) -> String {
    let keys = bound_keys(action, cx);
    if keys.is_empty() {
        label.to_owned()
    } else {
        format!("{label} ({keys})")
    }
}
