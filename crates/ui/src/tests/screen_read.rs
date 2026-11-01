use super::*;

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
    kagan.tabs.numbers()
}

pub(crate) fn shown_tab(kagan: &Kagan) -> Option<u32> {
    kagan.tabs.shown()
}

pub(crate) fn tab_grid(kagan: &Kagan, cx: &App) -> Option<kg_core::Grid> {
    match kagan.screen.shown() {
        Shown::Thread(view) => Some(kagan.tab_view(view).read(cx).grid().clone()),
        Shown::Nothing | Shown::Start(_) => None,
    }
}

pub(crate) fn tab_focused(kagan: &Kagan, window: &Window, cx: &App) -> bool {
    match kagan.screen.shown() {
        Shown::Thread(view) => kagan.tab_view(view).read(cx).focused(window),
        Shown::Nothing | Shown::Start(_) => false,
    }
}
