use crate::theme::TypeScale as _;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, PathBuf};

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::InteractiveElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    Icon, IconName, Selectable as _, Sizable as _, StyledExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, DismissEvent, DragMoveEvent, ElementId, Entity,
    EventEmitter, FocusHandle, Focusable as _, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement as _, Role, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, WeakEntity, Window, actions, div, px, relative,
};
use kg_core::Workspace;
use kg_domain::{PluginId, Project, ProjectId, ProjectKind, Thread, ThreadId};

mod brand_menu;
mod order;

use super::surface::{self, OpenSurface};
use order::{DragProject, Place};

use super::shown::{control, menu_item, menu_popup, tip};
use crate::theme::Tone;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

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

// What the person chose in the sidebar. Kagan acts on it.
pub(super) enum Event {
    Thread(ThreadId),
    Project(ProjectId),
    Start(ProjectId),
    Archive(ThreadId),
    ThreadActions(ThreadId),
    RemoveProject(ProjectId),
    Reorder {
        id: ProjectId,
        before: Option<ProjectId>,
    },
    RowPin(ThreadId),
    RowArchive(ThreadId),
    RowRename(ThreadId),
    RowSaveDiagnostics(ThreadId),
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
    pub(super) in_settings: bool,
    // What waits for the person, shown on the rail while a plugin draws the thread rows.
    pub(super) waiting: Option<usize>,
}

fn surface_event(page: bool, selected: bool, open: &OpenSurface) -> Event {
    match (selected, page) {
        (false, _) => Event::OpenSurface(open.clone()),
        (true, true) => Event::ClosePage,
        (true, false) => Event::Threads,
    }
}

// The sidebar's listing and what it shows of it: folds, the chosen project, row focus and scroll.
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
        let shown = thread.id.clone();
        let pin = if thread.pinned { "Unpin" } else { "Pin" };
        let rows = [
            ("thread-pin", pin, Event::RowPin as fn(ThreadId) -> Event),
            ("thread-archive", "Archive", Event::RowArchive),
            ("thread-rename", "Rename…", Event::RowRename),
            (
                "save-diagnostics",
                "Save diagnostics",
                Event::RowSaveDiagnostics,
            ),
        ];
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            rows.into_iter().fold(menu, |menu, (id, label, event)| {
                menu.item(entry(&sidebar, id, label, event, &shown))
                    .separator()
            })
        });
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
        let id = format!("thread-{id}");
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

fn project_line(
    row: &Row,
    sidebar: &mut Sidebar,
    id: &ProjectId,
    open: bool,
    window: &mut Window,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let disclose = if open {
        format!("Hide threads in {}", row.label)
    } else {
        format!("Show threads in {}", row.label)
    };
    let icon = if open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    let started = format!("New thread in {}", row.label);
    let (hide_id, start_id) = (id.clone(), id.clone());
    let face = row_line(row, sidebar, false, window, cx);
    let focused = sidebar
        .row_focus(&format!("project-{id}"))
        .is_some_and(|focus| focus.is_focused(window));
    let project = id.clone();
    let view = cx.entity().downgrade();
    let menu_view = view.clone();
    let standard = matches!(
        row.kind,
        Kind::Project {
            kind: ProjectKind::Standard,
            ..
        }
    );
    let line = project_shell(
        id,
        sidebar.drag.as_ref() == Some(id),
        sidebar.drop.as_ref(),
        cx,
    )
    .child(div().min_w_0().flex_1().child(face))
    .child(
        h_flex()
            .ml_2()
            .flex_shrink_0()
            .items_center()
            .when(!focused, gpui_kit::Styled::invisible)
            .group_hover("project-row", gpui_kit::Styled::visible)
            .child(
                h_flex()
                    .id(ElementId::Name(format!("project-disclose-{id}").into()))
                    .w_0()
                    .flex_shrink_0()
                    .overflow_hidden()
                    .when(focused, gpui_kit::Styled::w_6)
                    .group_hover("project-row", gpui_kit::Styled::w_6)
                    .ui_observe_as(Facts::button(disclose.clone()))
                    .child(icon_button(
                        &format!("project-disclose-{id}"),
                        disclose.clone(),
                        disclose,
                        icon,
                        cx.listener(move |this, _, _, cx| {
                            this.toggle_project(hide_id.clone(), cx);
                        }),
                    )),
            )
            .child(icon_command(
                format!("project-new-{id}"),
                started.clone(),
                tip(&started, &super::NewThread, cx),
                IconName::Plus,
                cx.listener(move |_, _, _, cx| {
                    cx.emit(Event::Project(start_id.clone()));
                }),
            ))
            .when(standard, |controls| {
                controls.child(control(
                    ElementId::Name(format!("project-actions-{id}").into()),
                    Facts::button("Project actions"),
                    Button::new(SharedString::from(format!("project-actions-control-{id}")))
                        .small()
                        .ghost()
                        .icon(IconName::Ellipsis)
                        .accessibility_label("Project actions")
                        .tooltip("Project actions")
                        .tab_stop(false)
                        .dropdown_menu(move |menu, _, _| {
                            project_menu(menu, project.clone(), menu_view.clone())
                        }),
                ))
            }),
    );
    if !standard {
        return line.into_any_element();
    }
    reorder_line(line, id, &row.label, view, cx).into_any_element()
}

fn project_shell(
    id: &ProjectId,
    dragged: bool,
    drop: Option<&(ProjectId, Place)>,
    cx: &App,
) -> impl gpui_kit::ParentElement
+ gpui_kit::Styled
+ gpui_kit::StatefulInteractiveElement
+ IntoElement
+ use<> {
    h_flex()
        .id(ElementId::Name(format!("project-drag-{id}").into()))
        .group("project-row")
        .w_full()
        .h_6()
        .px_2()
        .items_center()
        .rounded(cx.theme().radius)
        .hover(|line| line.bg(cx.theme().list_hover))
        .when(dragged, |line| line.opacity(0.5))
        .when(
            drop.is_some_and(|(target, place)| target == id && *place == Place::Before),
            |line| line.border_t_2().border_color(cx.theme().border),
        )
        .when(
            drop.is_some_and(|(target, place)| target == id && *place == Place::After),
            |line| line.border_b_2().border_color(cx.theme().border),
        )
}

fn reorder_line(
    line: impl gpui_kit::StatefulInteractiveElement + IntoElement,
    id: &ProjectId,
    label: &str,
    view: WeakEntity<Sidebar>,
    cx: &mut Context<Sidebar>,
) -> impl IntoElement {
    let drag = DragProject::new(id.clone(), label.to_owned());
    let move_target = id.clone();
    let drop_target = id.clone();
    let start_id = id.clone();
    line.on_drag(drag, move |drag, _, window, cx| {
        let preview = order::preview(drag, cx);
        let end = view.clone();
        let keys = cx.intercept_keystrokes(|event, window, cx| {
            if event.keystroke.key == "escape" && cx.stop_active_drag(window) {
                cx.stop_propagation();
            }
        });
        _ = view.update(cx, |this, cx| {
            this.drag = Some(start_id.clone());
            this.drop = None;
            this.drag_keys = Some(keys);
            cx.notify();
        });
        cx.observe_release_in(&preview, window, move |_, _, cx| {
            _ = end.update(cx, |this, cx| {
                this.drag = None;
                this.drop = None;
                this.drag_keys = None;
                cx.notify();
            });
        })
        .detach();
        preview
    })
    .on_drag_move(
        cx.listener(move |this, event: &DragMoveEvent<DragProject>, _, cx| {
            let from = event.drag(cx).id.clone();
            let inside = event.bounds.contains(&event.event.position);
            let next = if inside {
                let place = order::place_at(event.bounds, event.event.position);
                if order::noop_gap(&from, &move_target, place, &this.listing.projects) {
                    None
                } else {
                    Some((move_target.clone(), place))
                }
            } else if this
                .drop
                .as_ref()
                .is_some_and(|(target, _)| target == &move_target)
            {
                None
            } else {
                return;
            };
            if this.drop != next {
                this.drop = next;
                cx.notify();
            }
        }),
    )
    .on_drop(cx.listener(move |this, drag: &DragProject, _, cx| {
        let Some((_, place)) = this
            .drop
            .take()
            .filter(|(target, _)| target == &drop_target)
        else {
            this.drag = None;
            this.drag_keys = None;
            cx.notify();
            return;
        };
        this.drag = None;
        this.drag_keys = None;
        if let Some(before) = order::before_of(&this.listing.projects, &drop_target, place) {
            cx.emit(Event::Reorder {
                id: drag.id.clone(),
                before: before.into_option(),
            });
        }
        cx.notify();
    }))
}

fn icon_command(
    id: String,
    name: String,
    tooltip: String,
    icon: impl Into<Icon>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let facts = Facts::button(name.clone());
    let button = icon_button(&id, name, tooltip, icon, on_click);
    control(ElementId::Name(id.into()), facts, button)
}

fn icon_button(
    id: &str,
    name: String,
    tooltip: String,
    icon: impl Into<Icon>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    Button::new(SharedString::from(format!("{id}-control")))
        .small()
        .ghost()
        .icon(icon.into())
        .accessibility_label(name)
        .tooltip(tooltip)
        // A row is the one Tab stop; its hover controls are reached by hover, click or keys.
        .tab_stop(false)
        .on_click(on_click)
}

fn row_identity(kind: &Kind) -> String {
    match kind {
        Kind::Project { id, .. } => format!("project-{id}"),
        Kind::Thread { id, .. } => format!("thread-{id}"),
    }
}

fn row_line(
    row: &Row,
    sidebar: &mut Sidebar,
    inset: bool,
    window: &mut Window,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let id = row_identity(&row.kind);
    let focus = super::tab_stop(window, cx, SharedString::from(id.clone()));
    sidebar.add_row(id.clone(), focus.clone());
    let focused = focus.is_focused(window);
    let popup = match &row.kind {
        Kind::Thread { id, .. } => sidebar.menu_for(id),
        Kind::Project { .. } => None,
    };
    row_face(row, id, focus, focused, inset, popup, cx)
}

fn row_face(
    row: &Row,
    id: String,
    focus: FocusHandle,
    focused: bool,
    inset: bool,
    popup: Option<Entity<PopupMenu>>,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let kind = row.kind.clone();
    let project = matches!(kind, Kind::Project { .. });
    let name = match &kind {
        Kind::Project { name, .. } => name.clone(),
        Kind::Thread { .. } => row.label.clone(),
    };
    let detail = row.detail.clone();
    let tip = row_tip(row);
    let (state, mark, pin) = row_marks(&kind, cx);
    let (name_id, suffix_id) = project_text_ids(&kind, detail.is_some());
    let face = div()
        .id(ElementId::Name(id.into()))
        .group("thread-row")
        .key_context("SidebarRow")
        .role(Role::TreeItem)
        .aria_label(row.label.clone())
        .aria_selected(row.selected)
        .h_7()
        .w_full()
        .min_h(if project { px(24.) } else { px(28.) })
        .max_h(if project { px(24.) } else { px(28.) })
        .flex_shrink_0()
        .when(inset, gpui_kit::Styled::px_2)
        .when_some(thread_indent(&kind), |line, depth| {
            line.ml(px(depth))
                .border_l_1()
                .border_color(cx.theme().border)
        })
        .gap_2()
        .flex()
        .items_center()
        .rounded(cx.theme().radius)
        .type_body()
        .when(project, |line| {
            line.type_small().text_color(cx.theme().muted_foreground)
        })
        .when(row.selected, |line| {
            line.bg(cx.theme().sidebar_accent)
                .when(!project, gpui_kit::component::StyledExt::font_medium)
        })
        .when(row.open && !row.selected && !project, |line| {
            line.font_medium().text_color(crate::theme::brand_color(cx))
        })
        .when(!row.selected && !project, |line| {
            line.hover(|line| line.bg(cx.theme().list_hover))
        })
        .focus_visible(|line| line.border_1().border_color(cx.theme().ring))
        .when_some(tip, |line, tip| {
            line.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        })
        .ui_observe_as(Facts {
            role: Role::TreeItem,
            name: row.label.clone(),
            value: if row.open { Some("open".into()) } else { state },
            disabled: false,
            selected: Some(row.selected),
            focus: Some(focus),
            hint: row_tip(row),
        });
    let face = row_actions(face, &kind, cx)
        .when(mark.is_none() && inset, |line| {
            line.child(div().w_4().flex_shrink_0())
        })
        .when_some(mark, gpui_kit::ParentElement::child)
        .child(row_text(name, detail, name_id, suffix_id, cx))
        .when_some(pin, gpui_kit::ParentElement::child)
        .when_some(
            thread_controls(&kind, focused, popup, cx),
            gpui_kit::ParentElement::child,
        );
    if let Kind::Project {
        id,
        kind: ProjectKind::Standard,
        ..
    } = &row.kind
    {
        let project = id.clone();
        let view = cx.entity().downgrade();
        face.context_menu(move |menu, _, _| project_menu(menu, project.clone(), view.clone()))
            .into_any_element()
    } else {
        face.into_any_element()
    }
}

#[derive(Clone)]
enum Fold {
    Project(ProjectId),
    Thread(ThreadId),
}

fn activate_event(kind: &Kind) -> Event {
    match kind {
        Kind::Project { id, .. } => Event::Project(id.clone()),
        Kind::Thread { id, .. } => Event::Thread(id.clone()),
    }
}

fn enter_event(kind: &Kind) -> Event {
    match kind {
        Kind::Project { id, .. } => Event::Start(id.clone()),
        Kind::Thread { id, .. } => Event::Thread(id.clone()),
    }
}

fn row_actions<E: gpui_kit::StatefulInteractiveElement + gpui_kit::prelude::FluentBuilder>(
    face: E,
    kind: &Kind,
    cx: &mut Context<Sidebar>,
) -> E {
    let fold = match kind {
        Kind::Project { id, .. } => Some(Fold::Project(id.clone())),
        Kind::Thread {
            id, children: true, ..
        } => Some(Fold::Thread(id.clone())),
        Kind::Thread {
            children: false, ..
        } => None,
    };
    let project_id = if let Kind::Project { id, .. } = kind {
        Some(id.clone())
    } else {
        None
    };
    let (activate, enter, click) = (kind.clone(), kind.clone(), kind.clone());
    face.on_action(cx.listener(move |_, _: &ActivateRow, _, cx| {
        cx.emit(activate_event(&activate));
    }))
    .on_action(cx.listener(move |_, _: &EnterRow, _, cx| {
        cx.emit(enter_event(&enter));
    }))
    .on_action(cx.listener(|this, _: &FocusPreviousRow, window, cx| {
        this.focus_row(false, window, cx);
    }))
    .on_action(cx.listener(|this, _: &FocusNextRow, window, cx| {
        this.focus_row(true, window, cx);
    }))
    .when_some(fold, |line, fold| {
        let (collapse, expand) = (fold.clone(), fold);
        line.on_action(cx.listener(move |this, _: &Collapse, _, cx| {
            this.set_fold(collapse.clone(), false, cx);
        }))
        .on_action(cx.listener(move |this, _: &Expand, _, cx| {
            this.set_fold(expand.clone(), true, cx);
        }))
    })
    .when_some(project_id, |line, project| {
        line.on_action(cx.listener(move |_, _: &super::NewThread, _, cx| {
            cx.emit(Event::Project(project.clone()));
        }))
    })
    .on_click(cx.listener(move |_, _, _, cx| cx.emit(activate_event(&click))))
}

fn thread_indent(kind: &Kind) -> Option<f32> {
    match kind {
        Kind::Thread { depth, .. } if *depth > 0 => Some(24. * f32::from(*depth)),
        _ => None,
    }
}

const PERSONAL_TIP: &str = "Threads that are not in a project. Each one gets its own folder.";

fn row_tip(row: &Row) -> Option<SharedString> {
    match &row.kind {
        Kind::Project {
            kind: ProjectKind::Personal,
            ..
        } => Some(PERSONAL_TIP.into()),
        _ => row.detail.as_ref().map(|_| match &row.kind {
            Kind::Project { name, path, .. } => format!("{name} · {}", path.display()).into(),
            Kind::Thread { .. } => row.label.clone(),
        }),
    }
}

fn project_text_ids(kind: &Kind, detail: bool) -> (Option<String>, Option<String>) {
    match kind {
        Kind::Project { id, .. } if detail => (
            Some(format!("project-name-{id}")),
            Some(format!("project-suffix-{id}")),
        ),
        _ => (None, None),
    }
}

fn thread_controls(
    kind: &Kind,
    focused: bool,
    popup: Option<Entity<PopupMenu>>,
    cx: &mut Context<Sidebar>,
) -> Option<AnyElement> {
    let Kind::Thread {
        id,
        children,
        expanded,
        ..
    } = kind
    else {
        return None;
    };
    let thread = id.clone();
    let archive = thread.clone();
    let menu = thread.clone();
    // An open menu keeps its row's controls on screen, even though focus has moved into the menu.
    let open = popup.is_some();
    let controls = h_flex()
        .items_center()
        .when(!focused && !open, gpui_kit::Styled::invisible)
        .group_hover("thread-row", gpui_kit::Styled::visible)
        .when(*children, |controls| {
            let disclose = if *expanded {
                "Hide child threads"
            } else {
                "Show child threads"
            };
            controls.child(icon_command(
                format!("thread-disclose-{thread}"),
                disclose.to_owned(),
                disclose.to_owned(),
                if *expanded {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                },
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.toggle_thread(thread.clone(), cx);
                }),
            ))
        })
        .child(icon_command(
            format!("thread-archive-{archive}"),
            "Archive".to_owned(),
            tip("Archive", &super::ArchiveThread, cx),
            gpui_kit::assets::IconName::Archive,
            cx.listener(move |_, _, _, cx| {
                cx.stop_propagation();
                cx.emit(Event::Archive(archive.clone()));
            }),
        ))
        .child(
            div()
                .relative()
                .child(icon_command(
                    format!("thread-actions-{menu}"),
                    "Thread actions".to_owned(),
                    tip("Thread actions", &super::ThreadActions, cx),
                    IconName::Ellipsis,
                    cx.listener(move |_, _, _, cx| {
                        cx.stop_propagation();
                        cx.emit(Event::ThreadActions(menu.clone()));
                    }),
                ))
                .when_some(popup, |wrap, popup| {
                    wrap.child(menu_popup(
                        &popup,
                        "thread-menu-layer",
                        "popup-menu",
                        "Thread actions",
                    ))
                }),
        );
    Some(controls.into_any_element())
}

fn entry(
    sidebar: &WeakEntity<Sidebar>,
    id: &'static str,
    label: &'static str,
    event: impl Fn(ThreadId) -> Event + 'static,
    thread: &ThreadId,
) -> PopupMenuItem {
    let (sidebar, thread) = (sidebar.clone(), thread.clone());
    PopupMenuItem::element(move |_, _| menu_item(id, label)).on_click(move |_, _, cx| {
        _ = sidebar.update(cx, |_, cx| cx.emit(event(thread.clone())));
    })
}

fn project_menu(menu: PopupMenu, project: ProjectId, view: WeakEntity<Sidebar>) -> PopupMenu {
    menu.item(
        PopupMenuItem::element(|_, _| {
            div()
                .id("project-remove")
                .ui_observe_as(Facts {
                    role: Role::MenuItem,
                    ..Facts::button("Remove project…")
                })
                .child("Remove project…")
        })
        .on_click(move |_, _, cx| {
            _ = view.update(cx, |_, cx| cx.emit(Event::RemoveProject(project.clone())));
        }),
    )
}

fn row_marks(
    kind: &Kind,
    cx: &mut App,
) -> (Option<SharedString>, Option<AnyElement>, Option<AnyElement>) {
    match kind {
        Kind::Thread {
            id, tone, pinned, ..
        } => (
            Some(tone.word().into()),
            Some(thread_marks(id, *tone, cx)),
            pinned.then(|| pin_mark(format!("thread-pinned-{id}"), cx).into_any_element()),
        ),
        Kind::Project { .. } => (None, None, None),
    }
}

fn pin_mark(id: String, cx: &App) -> impl IntoElement {
    div()
        .id(ElementId::Name(id.into()))
        .w_4()
        .h(px(16.))
        .flex()
        .flex_shrink_0()
        .justify_center()
        .items_center()
        .ui_container()
        .aria_label("Pinned")
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(gpui_kit::assets::IconName::Pin).small())
}

fn row_glyph(icon: impl Into<Icon>, cx: &App) -> AnyElement {
    div()
        .w_4()
        .flex()
        .flex_shrink_0()
        .justify_center()
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(icon).small())
        .into_any_element()
}

fn row_text(
    name: SharedString,
    detail: Option<SharedString>,
    name_id: Option<String>,
    suffix_id: Option<String>,
    cx: &App,
) -> impl IntoElement {
    let suffix = detail.clone();
    let name = div().flex_initial().min_w_0().text_ellipsis().child(name);
    let name = if let Some(id) = name_id {
        name.id(ElementId::Name(id.into()))
            .ui_container()
            .min_w(px(40.))
            .max_w(relative(0.5))
            .into_any_element()
    } else {
        name.into_any_element()
    };
    h_flex()
        .flex_1()
        .min_w_0()
        .child(name)
        .when_some(detail, |line, detail| {
            let dot = div()
                .flex_none()
                .text_color(cx.theme().muted_foreground)
                .child("· ");
            let hint = div()
                .flex_1()
                .min_w_0()
                .text_ellipsis_start()
                .text_color(cx.theme().muted_foreground)
                .child(detail);
            match suffix_id {
                Some(id) => line.child(dot).child(
                    hint.id(ElementId::Name(id.into()))
                        .ui_container()
                        .aria_label(suffix.unwrap_or_default()),
                ),
                None => line.child(dot).child(hint),
            }
        })
}

fn thread_marks(id: &ThreadId, tone: Tone, cx: &mut App) -> AnyElement {
    div()
        .id(ElementId::Name(format!("thread-mark-{id}").into()))
        .ui_container()
        .w_4()
        .flex()
        .flex_shrink_0()
        .justify_center()
        .aria_label(tone.word())
        .child(tone.mark(cx))
        .into_any_element()
}

// When no suffix tells same-named projects apart, the group gets none.
fn parent_suffixes(projects: &[Project]) -> BTreeMap<ProjectId, String> {
    let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, project) in projects.iter().enumerate() {
        groups.entry(project.name.as_str()).or_default().push(index);
    }
    let mut suffixes = BTreeMap::new();
    for indexes in groups.values() {
        if indexes.len() < 2 {
            continue;
        }
        let parents: Vec<Vec<String>> = indexes
            .iter()
            .map(|index| parent_parts(&projects[*index].path))
            .collect();
        let longest = parents.iter().map(Vec::len).max().unwrap_or(0);
        let Some(length) = (1..=longest).find(|length| {
            let mut seen = BTreeSet::new();
            parents.iter().all(|parts| {
                let suffix = suffix_of(parts, *length);
                !suffix.is_empty() && seen.insert(suffix)
            })
        }) else {
            continue;
        };
        for (index, parts) in indexes.iter().zip(parents) {
            suffixes.insert(projects[*index].id.clone(), suffix_of(&parts, length));
        }
    }
    suffixes
}

fn parent_parts(path: &std::path::Path) -> Vec<String> {
    let mut parts: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            Component::Prefix(_)
            | Component::RootDir
            | Component::CurDir
            | Component::ParentDir => None,
        })
        .collect();
    parts.pop();
    parts
}

fn suffix_of(parts: &[String], length: usize) -> String {
    if length == 0 || parts.len() < length {
        return String::new();
    }
    parts[parts.len() - length..].join("/")
}

pub(super) fn list(
    sidebar: &mut Sidebar,
    shows: &Shows,
    threads: Option<AnyElement>,
    tool: Option<AnyElement>,
    window: &mut Window,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    sidebar.row_order.clear();
    let rows = sidebar_rows(sidebar, shows, cx);
    sidebar.reveal_selected(&rows);
    let thread_scroll = sidebar.thread_scroll.clone();
    let lines = thread_lines(&rows, sidebar, window, cx);
    let keys = super::shown::bound_keys(&super::NewThread, cx);
    let tooltip = if keys.is_empty() {
        "New thread".to_owned()
    } else {
        format!("New thread ({keys})")
    };
    v_flex()
        .id("sidebar-pane")
        .track_focus(sidebar.focus())
        .size_full()
        .ui_container()
        .child(
            v_flex()
                .id("sidebar")
                .size_full()
                .overflow_hidden()
                .bg(cx.theme().sidebar)
                .text_color(cx.theme().sidebar_foreground)
                .border_r_1()
                .border_color(cx.theme().sidebar_border)
                .child(
                    v_flex()
                        .id("sidebar-split")
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .pt_3()
                        .px_3()
                        .child(
                            div()
                                .id("sidebar-new-thread")
                                .ui_observe_as(Facts::button("New thread"))
                                .h_7()
                                .px_2()
                                .gap_2()
                                .flex()
                                .items_center()
                                .rounded(cx.theme().radius)
                                .hover(|line| line.bg(cx.theme().list_hover))
                                .tooltip(move |window, cx| {
                                    Tooltip::new(tooltip.clone()).build(window, cx)
                                })
                                .child(row_glyph(IconName::Plus, cx))
                                .child("New thread")
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(super::NewThread), cx);
                                }),
                        )
                        .child(threads.unwrap_or_else(|| {
                            scroll_port(
                                "threads-scroll",
                                "threads-rows",
                                "threads-scrollbar",
                                lines,
                                &thread_scroll,
                            )
                            .into_any_element()
                        })),
                )
                .when_some(tool, |sidebar, tool| {
                    sidebar.child(
                        div()
                            .id("plugin-tool-body")
                            .ui_container()
                            .flex_1()
                            .min_h_0()
                            .p_3()
                            .child(tool),
                    )
                })
                .child(footer(sidebar, window, cx)),
        )
        .into_any_element()
}

pub(super) fn rail(
    sidebar: &mut Sidebar,
    shows: &Shows,
    window: &mut Window,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let manage_focus = super::tab_stop(window, cx, "plugins-manage-focus");
    sidebar
        .row_focus
        .insert("plugins-manage".into(), manage_focus.clone());
    let mut plugins = BTreeSet::new();
    let tools: Vec<_> = surface::nav_items(cx)
        .into_iter()
        .filter(|item| !item.page && plugins.insert(item.plugin.clone()))
        .map(|item| rail_tool(sidebar, shows, item, window, cx))
        .collect();
    let pages: Vec<_> = surface::nav_items(cx)
        .into_iter()
        .filter(|item| item.page)
        .map(|item| rail_tool(sidebar, shows, item, window, cx))
        .collect();
    let top = v_flex()
        .id("rail-items")
        .ui_container()
        .w_full()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .items_center()
        .gap(px(6.))
        .child(threads_home(shows, cx))
        .children(tools)
        .children(pages)
        .child(manage_row(manage_focus, cx))
        .child(new_plugin_row(cx));
    v_flex()
        .id("sidebar-rail")
        .ui_container()
        .w(px(super::layout::RAIL_WIDTH))
        .h_full()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .pt_3()
        .pb_3()
        .bg(cx.theme().sidebar)
        .child(brand_menu::button(sidebar, cx))
        .child(top)
        .child(super::commands::rail_button(cx))
        .child(gear(shows.in_settings, cx))
        .into_any_element()
}

fn gear(active: bool, cx: &mut Context<Sidebar>) -> impl IntoElement {
    let tip = super::shown::tip("Settings", &super::OpenSettings, cx);
    let button = Button::new("settings-gear-button")
        .small()
        .rounded(px(6.))
        .icon(IconName::Settings)
        .accessibility_label("Settings")
        .tooltip(tip)
        .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::Settings)));
    let button = button.ghost().selected(active);
    super::shown::control(
        "settings-gear",
        Facts {
            selected: Some(active),
            ..Facts::button("Settings")
        },
        button,
    )
}

// One icon per plugin with a left tool, and one per center page. Its focus and id are kept for the
// keyboard and `kg ui`.
fn rail_tool(
    sidebar: &mut Sidebar,
    shows: &Shows,
    item: surface::NavItem,
    window: &mut Window,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let id = surface::row_id(&item.plugin, &item.id);
    let focus = super::tab_stop(window, cx, SharedString::from(id.clone()));
    sidebar.row_focus.insert(id.clone(), focus.clone());
    let page = item.page;
    let selected = if page {
        shows
            .page
            .as_ref()
            .is_some_and(|(plugin, id)| plugin == &item.plugin && id == &item.id)
    } else {
        shows
            .tool
            .as_ref()
            .is_some_and(|plugin| plugin == &item.plugin)
    };
    let icon = kg_plugins::PluginHost::try_global(cx)
        .and_then(|host| {
            host.read(cx)
                .icon(&item.plugin)
                .map(|bytes| Icon::default().data(bytes))
        })
        .unwrap_or_else(|| Icon::new(IconName::PanelLeft));
    let action = OpenSurface {
        plugin: item.plugin.0,
        id: item.id,
    };
    let keyboard = action.clone();
    let title = item.title;
    div()
        .id(ElementId::Name(id.into()))
        .flex_shrink_0()
        .track_focus(&focus)
        .key_context("SidebarRow")
        .on_action({
            let keyboard = keyboard.clone();
            cx.listener(move |_, _: &ActivateRow, _, cx| {
                cx.emit(surface_event(page, selected, &keyboard));
            })
        })
        .on_action(cx.listener(move |_, _: &EnterRow, _, cx| {
            cx.emit(surface_event(page, selected, &keyboard));
        }))
        .ui_observe_as(Facts {
            selected: Some(selected),
            focus: Some(focus),
            ..Facts::button(title.clone())
        })
        .child(
            Button::new(SharedString::from(format!(
                "{}-button",
                surface::row_id(&PluginId(action.plugin.clone()), &action.id)
            )))
            .small()
            .rounded(px(6.))
            .ghost()
            .selected(selected)
            .icon(icon)
            .accessibility_label(title.clone())
            .tooltip(tip(&title, &action, cx))
            .tab_stop(false)
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(surface_event(page, selected, &action));
            })),
        )
        .into_any_element()
}

fn threads_home(shows: &Shows, cx: &mut Context<Sidebar>) -> impl IntoElement {
    let waiting = shows.waiting.map(|count| {
        div()
            .id("rail-waiting")
            .ui_observe_as(Facts {
                role: Role::Status,
                ..Facts::button(format!("{count} waiting"))
            })
            .absolute()
            .top_0()
            .right_0()
            .type_caption()
            .text_color(Tone::Needs.color(cx))
            .child(count.to_string())
    });
    div()
        .relative()
        .flex_shrink_0()
        .child(control(
            "threads-home",
            Facts::button("Threads"),
            Button::new("threads-home-button")
                .small()
                .rounded(px(6.))
                .ghost()
                .selected(shows.tool.is_none() && !shows.in_settings && shows.page.is_none())
                .icon(gpui_kit::assets::IconName::House)
                .accessibility_label("Threads")
                .tooltip(tip("Threads", &Threads, cx))
                .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::Threads))),
        ))
        .children(waiting)
}

fn manage_row(manage_focus: FocusHandle, cx: &mut Context<Sidebar>) -> impl IntoElement {
    div()
        .id("plugins-manage")
        .flex_shrink_0()
        .key_context("SidebarRow")
        .track_focus(&manage_focus)
        .ui_observe_as(Facts {
            focus: Some(manage_focus.clone()),
            ..Facts::button("Manage plugins")
        })
        .on_action(cx.listener(|_, _: &ActivateRow, _, cx| {
            cx.emit(Event::ManagePlugins);
        }))
        .on_action(cx.listener(|_, _: &EnterRow, _, cx| {
            cx.emit(Event::ManagePlugins);
        }))
        .child(
            Button::new("plugins-manage-button")
                .small()
                .rounded(px(6.))
                .ghost()
                .icon(gpui_kit::assets::IconName::Puzzle)
                .accessibility_label("Manage plugins")
                .tooltip(tip("Manage plugins", &ManagePlugins, cx))
                .tab_stop(false)
                .on_click(cx.listener(move |_, _, window, cx| {
                    window.focus(&manage_focus, cx);
                    cx.emit(Event::ManagePlugins);
                })),
        )
}

fn new_plugin_row(cx: &mut Context<Sidebar>) -> impl IntoElement {
    div().flex_shrink_0().child(control(
        "plugins-add",
        Facts::button("New plugin…"),
        Button::new("plugins-add-button")
            .small()
            .rounded(px(6.))
            .ghost()
            .icon(IconName::Plus)
            .accessibility_label("New plugin…")
            .tooltip(tip("New plugin…", &NewPlugin, cx))
            .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::NewPlugin))),
    ))
}

fn footer(sidebar: &Sidebar, window: &mut Window, cx: &mut Context<Sidebar>) -> impl IntoElement {
    let archived_focus = super::tab_stop(window, cx, "archived-focus");
    h_flex()
        .id("sidebar-add")
        .flex_shrink_0()
        .w_full()
        .pt_2()
        .pb_3()
        .px_3()
        .justify_between()
        .items_center()
        .child(control(
            "add-project",
            Facts::button("Add project"),
            Button::new("add-project-control")
                .small()
                .ghost()
                .icon(gpui_kit::assets::IconName::FolderPlus)
                .accessibility_label("Add project")
                .tooltip({
                    let keys = super::shown::bound_keys(&super::AddProject, cx);
                    if keys.is_empty() {
                        "Add project".to_owned()
                    } else {
                        format!("Add project ({keys})")
                    }
                })
                .tab_stop(false)
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(super::AddProject), cx);
                }),
        ))
        .when(
            matches!(sidebar.archived_footer, ArchivedFooter::Shown),
            |footer| {
                footer.child(
                    div()
                        .id("archived")
                        .key_context("SidebarRow")
                        .track_focus(&archived_focus)
                        .ui_observe_as(Facts {
                            focus: Some(archived_focus.clone()),
                            ..Facts::button("Archived")
                        })
                        .on_action(
                            cx.listener(|_, _: &ActivateRow, _, cx| cx.emit(Event::Archived)),
                        )
                        .on_action(cx.listener(|_, _: &EnterRow, _, cx| cx.emit(Event::Archived)))
                        .child(
                            Button::new("archived-control")
                                .small()
                                .ghost()
                                .icon(gpui_kit::assets::IconName::Archive)
                                .accessibility_label("Archived")
                                .tooltip("Archived threads")
                                .tab_stop(false)
                                .on_click(cx.listener(move |_, _, window, cx| {
                                    window.focus(&archived_focus, cx);
                                    cx.emit(Event::Archived);
                                })),
                        ),
                )
            },
        )
}

// The same test as the Archived list, so the footer icon never opens an empty list.
pub(super) fn archived_footer(records: &[Thread]) -> ArchivedFooter {
    let any = records.iter().any(|thread| {
        thread.visibility == kg_domain::Visibility::Visible && thread.archived_at.is_some()
    });
    if any {
        ArchivedFooter::Shown
    } else {
        ArchivedFooter::Hidden
    }
}

fn thread_lines(
    rows: &[Row],
    sidebar: &mut Sidebar,
    window: &mut Window,
    cx: &mut Context<Sidebar>,
) -> Vec<AnyElement> {
    rows.iter()
        .map(|row| match &row.kind {
            Kind::Project { id, open, .. } => project_line(row, sidebar, id, *open, window, cx),
            Kind::Thread { .. } => row_line(row, sidebar, true, window, cx).into_any_element(),
        })
        .collect()
}

fn scroll_port(
    id: &'static str,
    rows_id: &'static str,
    bar_id: &'static str,
    lines: Vec<AnyElement>,
    scroll: &ScrollHandle,
) -> impl IntoElement {
    div()
        .id(id)
        .ui_container()
        .w_full()
        .overflow_hidden()
        .min_h_0()
        .flex_1()
        .child(
            div()
                .relative()
                .size_full()
                .child(
                    v_flex()
                        .id(rows_id)
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(scroll)
                        .lock_scroll_axis()
                        .ui_container()
                        .children(lines),
                )
                .child(
                    div().absolute().inset_0().child(
                        Scrollbar::vertical(scroll)
                            .id(bar_id)
                            .viewport_from_layout(),
                    ),
                ),
        )
}

fn thread_scroll_index(row_order: &[String], id: &str) -> Option<usize> {
    if !(id.starts_with("thread-") || id.starts_with("project-") || id == "archived") {
        return None;
    }
    row_order.iter().position(|row| row == id)
}

// `shown` is none on a page or in the archive, where no thread row is selected.
struct Scope<'a> {
    sidebar: &'a Sidebar,
    cx: &'a App,
    shown: Option<ThreadId>,
}

fn sidebar_rows(sidebar: &Sidebar, shows: &Shows, cx: &App) -> Vec<Row> {
    let listing = &sidebar.listing;
    let suffixes = parent_suffixes(&listing.projects);
    let on_screen = if shows.page.is_some() || shows.archived {
        None
    } else {
        shows.thread.clone()
    };
    let scope = Scope {
        sidebar,
        cx,
        shown: on_screen,
    };
    let mut rows = Vec::new();
    for project in &listing.projects {
        let open = !sidebar.closed.contains(&project.id);
        let suffix = suffixes.get(&project.id).cloned();
        let label = match &suffix {
            Some(suffix) => format!("{} · {suffix}", project.name),
            None => project.name.clone(),
        };
        rows.push(Row {
            kind: Kind::Project {
                id: project.id.clone(),
                name: project.name.clone().into(),
                path: project.path.clone(),
                open,
                kind: project.kind,
            },
            label: label.into(),
            detail: suffix.map(SharedString::from),
            selected: project_selected(sidebar, shows, &project.id),
            open: false,
        });
        if !open {
            continue;
        }
        let mut threads: Vec<&Thread> = listing
            .threads
            .iter()
            .filter(|thread| thread.archived_at.is_none() && thread.project_id == project.id)
            .collect();
        threads.sort_by_key(|thread| !thread.pinned);
        let ids: BTreeSet<_> = threads.iter().map(|thread| &thread.id).collect();
        let mut visited = BTreeSet::new();
        for thread in &threads {
            if thread
                .parent_thread_id
                .as_ref()
                .is_none_or(|parent| !ids.contains(parent))
            {
                append_thread(&mut rows, thread, &threads, &mut visited, 0, &scope);
            }
        }
        for thread in &threads {
            if !visited.contains(&thread.id) {
                append_thread(&mut rows, thread, &threads, &mut visited, 0, &scope);
            }
        }
    }
    rows
}

fn append_thread(
    rows: &mut Vec<Row>,
    thread: &Thread,
    threads: &[&Thread],
    visited: &mut BTreeSet<ThreadId>,
    depth: u16,
    scope: &Scope,
) {
    if !visited.insert(thread.id.clone()) {
        return;
    }
    let children = threads
        .iter()
        .any(|child| child.parent_thread_id.as_ref() == Some(&thread.id));
    let expanded = !scope.sidebar.closed_threads.contains(&thread.id);
    rows.push(Row {
        kind: Kind::Thread {
            id: thread.id.clone(),
            tone: super::attention::global(scope.cx)
                .read(scope.cx)
                .tone(thread, scope.cx),
            pinned: thread.pinned,
            depth,
            children,
            expanded,
        },
        label: thread.title.clone().into(),
        detail: None,
        selected: scope.shown.as_ref() == Some(&thread.id),
        open: false,
    });
    for child in threads
        .iter()
        .filter(|child| child.parent_thread_id.as_ref() == Some(&thread.id))
    {
        if expanded {
            append_thread(rows, child, threads, visited, depth + 1, scope);
        } else {
            mark_descendants(child, threads, visited);
        }
    }
}

fn mark_descendants(thread: &Thread, threads: &[&Thread], visited: &mut BTreeSet<ThreadId>) {
    if !visited.insert(thread.id.clone()) {
        return;
    }
    for child in threads
        .iter()
        .filter(|child| child.parent_thread_id.as_ref() == Some(&thread.id))
    {
        mark_descendants(child, threads, visited);
    }
}

fn project_selected(sidebar: &Sidebar, shows: &Shows, id: &ProjectId) -> bool {
    if shows.page.is_some() || shows.archived || shows.thread.is_some() {
        return false;
    }
    sidebar.chosen() == Some(id)
}

#[cfg(test)]
mod tests;
