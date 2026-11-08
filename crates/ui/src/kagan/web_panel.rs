use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Role, SharedString, Styled as _, Window, div,
};

use super::surface::Slot;
mod native;
mod page_watch;
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) fn body(slot: &Slot, window: &mut Window, cx: &mut App) -> AnyElement {
    let Some(file) = slot.web.clone() else {
        return div().into_any_element();
    };
    let content = native::body(slot, &file, window, cx);
    frame(&slot.id, &slot.title, content)
}

pub(crate) use native::{clear, deliver, drop_plugin, forget, repaint, sync};

fn frame(id: &str, title: &str, content: AnyElement) -> AnyElement {
    div()
        .id(ElementId::Name(format!("web-panel-{id}").into()))
        .size_full()
        .min_h_0()
        .overflow_hidden()
        .ui_observe_as(Facts {
            role: Role::Document,
            name: "Web panel".into(),
            value: Some(title.to_owned().into()),
            disabled: false,
            selected: None,
            focus: None,
            hint: Some("kg ui cannot drive this page".into()),
        })
        .child(content)
        .into_any_element()
}

fn sentence(id: &str, note: &str, fill: bool, cx: &App) -> AnyElement {
    div()
        .w_full()
        .when(fill, gpui_kit::Styled::h_full)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .id(ElementId::Name(format!("web-panel-{id}-note").into()))
                .ui_observe_as(Facts {
                    role: Role::Label,
                    name: note.to_owned().into(),
                    value: None,
                    disabled: false,
                    selected: None,
                    focus: None,
                    hint: None,
                })
                .type_small(cx)
                .text_color(cx.theme().danger)
                .child(SharedString::from(note.to_owned())),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests;

#[cfg(all(test, target_os = "macos"))]
pub(crate) use tests::{inject, mark_stale, queue_page, testing_page};
