use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable as _, KeyBinding,
    ScrollHandle, SharedString, Subscription, Window, actions,
};
use kg_core::Workspace;
use kg_domain::{PluginId, Project, ProjectId, ProjectKind, Thread, ThreadId};

mod brand_menu;
mod order;

use super::shown::{ThreadPick, thread_menu};
use super::surface::OpenSurface;
use order::Place;

use crate::theme::Tone;

pub(in crate::kagan) const THREAD_ROW: &str = "thread-";
pub(in crate::kagan) const PROJECT_ROW: &str = "project-";

actions!(
    sidebar,
    [
        ActivateRow,
        EnterRow,
        FocusPreviousRow,
        FocusNextRow,
        Collapse,
        Expand,
        Threads,
        ManagePlugins,
        NewPlugin
    ]
);

pub(super) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", EnterRow, Some("SidebarRow")),
        KeyBinding::new("space", ActivateRow, Some("SidebarRow")),
        KeyBinding::new("up", FocusPreviousRow, Some("SidebarRow")),
        KeyBinding::new("down", FocusNextRow, Some("SidebarRow")),
        KeyBinding::new("left", Collapse, Some("SidebarRow")),
        KeyBinding::new("right", Expand, Some("SidebarRow")),
    ]
}

#[derive(Clone, Copy)]
pub(super) enum ArchivedFooter {
    Hidden,
    Shown,
}

pub(super) enum Event {
    Thread(ThreadId),
    Project(ProjectId),
    Start(ProjectId),
    Archive(ThreadId),
    ThreadActions(ThreadId),
    RemoveProject(ProjectId),
    AgentRules(ProjectId),
    Reorder {
        id: ProjectId,
        before: Option<ProjectId>,
    },
    RowPick(ThreadId, ThreadPick),
    RowMenuClosed,
    OpenSurface(OpenSurface),
    ClosePage,
    Threads,
    ManagePlugins,
    NewPlugin,
    Settings,
    Archived,
}

// What the window shows, read for each frame, so the sidebar keeps no copy of it.
pub(super) struct Shows {
    pub(super) thread: Option<ThreadId>,
    pub(super) page: Option<(PluginId, String)>,
    pub(super) tool: Option<PluginId>,
    pub(super) archived: bool,
    pub(super) skills: bool,
    pub(super) in_settings: bool,
    pub(super) waiting: Option<usize>,
}

pub(super) fn surface_event(page: bool, selected: bool, open: &OpenSurface) -> Event {
    match (selected, page) {
        (false, _) => Event::OpenSurface(open.clone()),
        (true, true) => Event::ClosePage,
        (true, false) => Event::Threads,
    }
}

pub(super) struct Sidebar {
    pub(super) listing: kg_domain::Sidebar,
    archived_footer: ArchivedFooter,
    chosen: Option<ProjectId>,
    // Projects whose threads are hidden. Kept on this window only, and not saved.
    closed: BTreeSet<ProjectId>,
    closed_threads: BTreeSet<ThreadId>,
    focus: FocusHandle,
    thread_scroll: ScrollHandle,
    scroll_thread: Option<String>,
    // The left tool row to focus after its body closes, when that row is still shown.
    return_tool: Option<String>,
    // Row ids in draw order. Arrow keys follow this list, so it matches the rows on screen.
    row_order: Vec<String>,
    row_focus: BTreeMap<String, FocusHandle>,
    row_menu: Option<(Entity<PopupMenu>, Subscription, ThreadId)>,
    brand_focus: FocusHandle,
    brand_menu: Option<(Entity<PopupMenu>, Subscription)>,
    drag: Option<ProjectId>,
    drop: Option<(ProjectId, Place)>,
    drag_keys: Option<Subscription>,
}

impl EventEmitter<Event> for Sidebar {}

impl Sidebar {
    pub(super) fn new(listing: kg_domain::Sidebar, cx: &mut Context<Self>) -> Self {
        Self {
            listing,
            archived_footer: ArchivedFooter::Hidden,
            chosen: None,
            closed: BTreeSet::new(),
            closed_threads: BTreeSet::new(),
            focus: cx.focus_handle(),
            thread_scroll: ScrollHandle::default(),
            scroll_thread: None,
            return_tool: None,
            row_order: Vec::new(),
            row_focus: BTreeMap::new(),
            row_menu: None,
            brand_focus: cx.focus_handle().tab_stop(true),
            brand_menu: None,
            drag: None,
            drop: None,
            drag_keys: None,
        }
    }

    pub(super) fn project(&self, id: &ProjectId) -> Option<&Project> {
        self.listing
            .projects
            .iter()
            .find(|project| project.id == *id)
    }

    pub(super) fn replace(&mut self, listing: kg_domain::Sidebar, footer: ArchivedFooter) {
        self.listing = listing;
        self.archived_footer = footer;
        if self
            .drag
            .as_ref()
            .is_some_and(|id| self.project(id).is_none())
        {
            self.drag = None;
            self.drop = None;
            self.drag_keys = None;
        } else if self
            .drop
            .as_ref()
            .is_some_and(|(id, _)| self.project(id).is_none())
        {
            self.drop = None;
        }
    }

    pub(super) fn chosen(&self) -> Option<&ProjectId> {
        self.chosen.as_ref()
    }

    pub(super) fn choose(&mut self, project: Option<ProjectId>) {
        self.chosen = project;
    }

    pub(super) fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    pub(super) fn row_focus(&self, id: &str) -> Option<&FocusHandle> {
        self.row_focus.get(id)
    }

    pub(super) fn set_return_tool(&mut self, row: Option<String>) {
        self.return_tool = row;
    }

    pub(super) fn take_return_tool(&mut self) -> Option<String> {
        self.return_tool.take()
    }

    fn add_row(&mut self, id: String, focus: FocusHandle) {
        self.row_order.push(id.clone());
        self.row_focus.insert(id, focus);
    }

    fn toggle_project(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        if !self.closed.remove(&project) {
            self.closed.insert(project);
        }
        cx.notify();
    }

    fn toggle_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if !self.closed_threads.remove(&thread) {
            self.closed_threads.insert(thread);
        }
        cx.notify();
    }

    // Opens the project and every folded parent above the thread, so its row is on screen.
    // Reads the workspace, so a thread the sidebar has not listed yet still reveals.
    pub(super) fn reveal_thread(&mut self, id: &ThreadId, cx: &App) {
        let workspace = Workspace::global(cx);
        let workspace = workspace.read(cx);
        let record = |id: &ThreadId| {
            workspace
                .thread(id)
                .map(|thread| thread.read(cx).record().clone())
        };
        let Some(thread) = record(id) else {
            return;
        };
        let project = thread.project_id;
        let mut parents = Vec::new();
        let mut parent = thread.parent_thread_id;
        while let Some(next) = parent {
            if parents.contains(&next) {
                break;
            }
            parent = record(&next).and_then(|record| record.parent_thread_id);
            parents.push(next);
        }
        self.closed.remove(&project);
        for parent in &parents {
            self.closed_threads.remove(parent);
        }
    }

    fn set_fold(&mut self, fold: Fold, open: bool, cx: &mut Context<Self>) {
        match fold {
            Fold::Project(project) => {
                if open {
                    self.closed.remove(&project);
                } else {
                    self.closed.insert(project);
                }
            }
            Fold::Thread(thread) => {
                if open {
                    self.closed_threads.remove(&thread);
                } else {
                    self.closed_threads.insert(thread);
                }
            }
        }
        cx.notify();
    }

    fn focus_row(&mut self, next: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.row_order.iter().position(|id| {
            self.row_focus
                .get(id)
                .is_some_and(|handle| handle.is_focused(window))
        }) else {
            return;
        };
        let target = if next {
            index + 1
        } else if index == 0 {
            return;
        } else {
            index - 1
        };
        let Some(id) = self.row_order.get(target).cloned() else {
            return;
        };
        if let Some(handle) = self.row_focus.get(&id).cloned() {
            window.focus(&handle, cx);
            cx.notify();
        }
        self.scroll_row(&id);
    }

    pub(super) fn row_or_first(&self, candidate: Option<String>) -> Option<String> {
        candidate
            .filter(|id| self.row_order.iter().any(|row| row == id))
            .or_else(|| self.row_order.first().cloned())
    }

    pub(super) fn reveal_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let focus = self.row_focus.get(id).unwrap_or(&self.focus).clone();
        window.focus(&focus, cx);
        self.scroll_row(id);
        cx.notify();
    }

    pub(super) fn close_row_menu(&mut self) {
        self.row_menu = None;
    }

    fn menu_for(&self, thread: &ThreadId) -> Option<Entity<PopupMenu>> {
        self.row_menu
            .as_ref()
            .filter(|(_, _, open)| open == thread)
            .map(|(menu, _, _)| menu.clone())
    }

    pub(super) fn open_row_menu(
        &mut self,
        thread: &Thread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu_for(&thread.id).is_some() {
            return;
        }
        let sidebar = cx.entity().downgrade();
        let id = thread.id.clone();
        let menu = thread_menu(
            thread,
            move |pick, _, cx| {
                _ = sidebar.update(cx, |_, cx| cx.emit(Event::RowPick(id.clone(), pick)));
            },
            |menu| menu,
            window,
            cx,
        );
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, _, cx| {
            this.row_menu = None;
            cx.emit(Event::RowMenuClosed);
            cx.notify();
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.row_menu = Some((menu, dismiss, thread.id.clone()));
        cx.notify();
    }

    pub(super) fn thread_ids(&self, shows: &Shows, cx: &App) -> Vec<ThreadId> {
        sidebar_rows(self, shows, cx)
            .into_iter()
            .filter_map(|row| match row.kind {
                Kind::Thread { id, .. } => Some(id),
                Kind::Project { .. } => None,
            })
            .collect()
    }

    fn scroll_row(&mut self, id: &str) {
        if let Some(index) = thread_scroll_index(&self.row_order, id) {
            self.thread_scroll.scroll_to_item(index);
        }
    }

    fn reveal_selected(&mut self, rows: &[Row]) {
        let Some(index) = rows
            .iter()
            .position(|row| row.selected && matches!(row.kind, Kind::Thread { .. }))
        else {
            return;
        };
        let Kind::Thread { id, .. } = &rows[index].kind else {
            return;
        };
        let id = format!("{THREAD_ROW}{id}");
        if self.scroll_thread.as_deref() == Some(id.as_str()) {
            return;
        }
        self.scroll_thread = Some(id);
        self.thread_scroll.scroll_to_item(index);
    }
}

#[derive(Clone)]
enum Kind {
    Project {
        id: ProjectId,
        name: SharedString,
        path: PathBuf,
        open: bool,
        kind: ProjectKind,
    },
    Thread {
        id: ThreadId,
        tone: Tone,
        pinned: bool,
        muted: bool,
        depth: u16,
        children: bool,
        expanded: bool,
    },
}

#[derive(Clone)]
struct Row {
    kind: Kind,
    label: SharedString,
    detail: Option<SharedString>,
    selected: bool,
    open: bool,
}

#[derive(Clone)]
enum Fold {
    Project(ProjectId),
    Thread(ThreadId),
}

mod list;
mod marks;
mod rail;
mod rows;

use list::{sidebar_rows, thread_scroll_index};

pub(super) use list::list;
pub(super) use rail::{archived_footer, rail};
#[cfg(test)]
mod tests;
