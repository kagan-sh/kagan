//! A plugin panel whose body is an HTML file in the plugin folder.
//!
//! The page is shown only while that panel is on screen, and dropped when the panel closes.
//! The native view runs on macOS only. Other systems show a one-line notice.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Role, SharedString, Styled as _, Window, div,
};
use kg_domain::PluginId;

use super::Kagan;
use super::surface::Slot;
#[cfg(target_os = "macos")]
mod native;
#[cfg(target_os = "macos")]
#[path = "page_watch.rs"]
mod page_watch;
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

#[cfg(not(target_os = "macos"))]
const ONLY_MACOS: &str = "This page works on macOS only for now.";

pub(super) fn body(slot: &Slot, window: &mut Window, cx: &mut App) -> AnyElement {
    let Some(file) = slot.web.clone() else {
        return div().into_any_element();
    };
    #[cfg(target_os = "macos")]
    let content = native::body(slot, &file, window, cx);
    #[cfg(not(target_os = "macos"))]
    let content = {
        let _ = (file, window);
        sentence(&slot.id, ONLY_MACOS, true, cx)
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
    #[cfg(target_os = "macos")]
    {
        native::deliver(plugin, panel, message, cx)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (plugin, panel, message, cx);
        Err(ONLY_MACOS.into())
    }
}

pub(super) fn clear(cx: &mut App) {
    #[cfg(target_os = "macos")]
    native::clear(cx);
    #[cfg(not(target_os = "macos"))]
    let _ = cx;
}

// Drops every open page for a plugin so `WebKit` releases their stores before removal.
pub(crate) fn drop_plugin(plugin: &PluginId, cx: &mut App) {
    #[cfg(target_os = "macos")]
    native::drop_plugin(plugin, cx);
    #[cfg(not(target_os = "macos"))]
    let _ = (plugin, cx);
}

pub(crate) fn forget(slot: &str, cx: &mut App) {
    #[cfg(target_os = "macos")]
    native::forget(slot, cx);
    #[cfg(not(target_os = "macos"))]
    let _ = (slot, cx);
}

pub(super) fn sync(kagan: &Kagan, cx: &mut App) {
    #[cfg(target_os = "macos")]
    native::sync(kagan, cx);
    #[cfg(not(target_os = "macos"))]
    let _ = (kagan, cx);
}

// Pushes the live Workbench theme to every open page, even when the window is not drawing.
pub(super) fn repaint(cx: &mut App) {
    #[cfg(target_os = "macos")]
    native::repaint(cx);
    #[cfg(not(target_os = "macos"))]
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
#[cfg(target_os = "macos")]
#[path = "web_panel_test.rs"]
mod web_panel_test;

#[cfg(all(test, target_os = "macos"))]
pub(crate) use web_panel_test::{inject, mark_stale, queue_page, testing_page};
