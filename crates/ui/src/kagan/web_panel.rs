//! A plugin panel whose body is an HTML file in the plugin folder.
//!
//! The page is shown only while that panel is on screen, and dropped when the panel closes.
//! The native view runs on macOS and Windows. Linux shows a one-line notice.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Role, SharedString, Styled as _, Window, div,
};
use kg_domain::PluginId;

use super::Kagan;
use super::surface::Slot;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod native;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod page_watch;
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const NO_PAGE: &str = "This page works on macOS and Windows only for now.";

pub(super) fn body(slot: &Slot, window: &mut Window, cx: &mut App) -> AnyElement {
    let Some(file) = slot.web.clone() else {
        return div().into_any_element();
    };
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let content = native::body(slot, &file, window, cx);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let content = {
        let _ = (file, window);
        sentence(&slot.id, NO_PAGE, true, cx)
    };
    frame(&slot.id, &slot.title, content)
}

// Errors say why the page could not take the message, such as when it is off screen.
pub(crate) fn post(
    plugin: &PluginId,
    panel: &str,
    message: &str,
    cx: &mut App,
) -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        native::deliver(plugin, panel, message, cx)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (plugin, panel, message, cx);
        Err(NO_PAGE.into())
    }
}

pub(super) fn clear(cx: &mut App) {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    native::clear(cx);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = cx;
}

// Drops every open page for a plugin so the system web view releases their stores before removal.
pub(crate) fn drop_plugin(plugin: &PluginId, cx: &mut App) {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    native::drop_plugin(plugin, cx);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = (plugin, cx);
}

pub(crate) fn forget(slot: &str, cx: &mut App) {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    native::forget(slot, cx);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = (slot, cx);
}

pub(super) fn sync(kagan: &Kagan, cx: &mut App) {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    native::sync(kagan, cx);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = (kagan, cx);
}

// Pushes the live Workbench theme to every open page, even when the window is not drawing.
pub(super) fn repaint(cx: &mut App) {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    native::repaint(cx);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = cx;
}

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
                .type_small()
                .text_color(cx.theme().danger)
                .child(SharedString::from(note.to_owned())),
        )
        .into_any_element()
}

#[cfg(test)]
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod tests;

#[cfg(all(test, target_os = "macos"))]
pub(crate) use tests::{inject, mark_stale, queue_page, testing_page};
