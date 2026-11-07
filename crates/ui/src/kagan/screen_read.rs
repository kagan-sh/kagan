use gpui_kit::{App, Entity, Window};
use kg_domain::ProjectId;

use super::{Kagan, Shown, right};
use crate::terminal::TerminalView;

pub(crate) fn start_project(kagan: &Kagan) -> Option<ProjectId> {
    match kagan.screen.shown() {
        Shown::Start(project) => Some(project.clone()),
        Shown::Nothing | Shown::Thread(_) => None,
    }
}

pub(crate) fn terminal_focused(kagan: &Kagan, window: &Window, cx: &App) -> bool {
    match kagan.screen.shown() {
        Shown::Thread(view) => view.read(cx).focused(window),
        Shown::Nothing | Shown::Start(_) => false,
    }
}

pub(crate) fn terminal_fitted(kagan: &Kagan, cx: &App) -> Option<(u16, u16)> {
    match kagan.screen.shown() {
        Shown::Thread(view) => view.read(cx).fitted(),
        Shown::Nothing | Shown::Start(_) => None,
    }
}

pub(crate) fn terminal_view(kagan: &Kagan) -> Option<Entity<TerminalView>> {
    match kagan.screen.shown() {
        Shown::Thread(view) => Some(view.clone()),
        Shown::Nothing | Shown::Start(_) => None,
    }
}

pub(crate) fn terminal_grid(kagan: &Kagan, cx: &App) -> Option<kg_core::Grid> {
    match kagan.screen.shown() {
        Shown::Thread(view) => Some(view.read(cx).grid().clone()),
        Shown::Nothing | Shown::Start(_) => None,
    }
}

pub(crate) fn tab_numbers(kagan: &Kagan) -> Vec<u32> {
    kagan.tabs.numbers().collect()
}

/// The extra terminal the right sidebar shows, if it shows one.
pub(crate) fn shown_tab(kagan: &Kagan, cx: &App) -> Option<u32> {
    match right::selected_tab(kagan, cx) {
        Some(right::Tab::Terminal(number)) => Some(number),
        _ => None,
    }
}

pub(crate) fn tab_grid(kagan: &Kagan, number: u32, cx: &App) -> Option<kg_core::Grid> {
    Some(kagan.tabs.view(number)?.read(cx).grid().clone())
}

#[cfg(desktop)]
pub(crate) fn browser_drawn(kagan: &Kagan, cx: &App) -> bool {
    kagan.browser_drawn(cx)
}

#[cfg(desktop)]
pub(crate) fn browser_address(kagan: &Kagan, cx: &App) -> String {
    kagan.browser.address(cx)
}

pub(crate) fn tab_focused(kagan: &Kagan, number: u32, window: &Window, cx: &App) -> bool {
    kagan
        .tabs
        .view(number)
        .is_some_and(|view| view.read(cx).focused(window))
}
