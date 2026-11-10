use std::collections::{BTreeMap, BTreeSet};

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::InteractiveElementExt as _;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{IconName, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use kg_domain::{ProjectId, Thread, ThreadId};

use crate::theme::{SpaceStep, row_height, space};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

use super::marks::{parent_suffixes, row_glyph};
use super::rail::footer;
use super::rows::{project_line, row_line};
use super::{Kind, Row, Shows, Sidebar};

pub(in crate::kagan) fn list(
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
    let keys = crate::kagan::controls::bound_keys(&crate::kagan::NewThread, cx);
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
                        .pt(px(space(SpaceStep::Md, cx)))
                        .px(px(space(SpaceStep::Md, cx)))
                        .child(
                            div()
                                .id("sidebar-new-thread")
                                .ui_observe_as(Facts::button("New thread"))
                                .h(px(row_height(cx)))
                                .px(px(space(SpaceStep::Sm, cx)))
                                .gap(px(space(SpaceStep::Sm, cx)))
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
                                    window.dispatch_action(Box::new(crate::kagan::NewThread), cx);
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
                            .p(px(space(SpaceStep::Md, cx)))
                            .child(tool),
                    )
                })
                .child(footer(sidebar, window, cx)),
        )
        .into_any_element()
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

pub(super) fn thread_scroll_index(row_order: &[String], id: &str) -> Option<usize> {
    if !(id.starts_with(super::THREAD_ROW)
        || id.starts_with(super::PROJECT_ROW)
        || id == "archived")
    {
        return None;
    }
    row_order.iter().position(|row| row == id)
}

struct Scope<'a> {
    sidebar: &'a Sidebar,
    cx: &'a App,
    shown: Option<ThreadId>,
}

pub(super) fn sidebar_rows(sidebar: &Sidebar, shows: &Shows, cx: &App) -> Vec<Row> {
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
        let mut children: BTreeMap<&ThreadId, Vec<&Thread>> = BTreeMap::new();
        for &thread in &threads {
            if let Some(parent) = &thread.parent_thread_id {
                children.entry(parent).or_default().push(thread);
            }
        }
        let mut visited = BTreeSet::new();
        for thread in &threads {
            if thread
                .parent_thread_id
                .as_ref()
                .is_none_or(|parent| !ids.contains(parent))
            {
                append_thread(&mut rows, thread, &children, &mut visited, 0, &scope);
            }
        }
        for thread in &threads {
            if !visited.contains(&thread.id) {
                append_thread(&mut rows, thread, &children, &mut visited, 0, &scope);
            }
        }
    }
    rows
}

fn append_thread(
    rows: &mut Vec<Row>,
    thread: &Thread,
    children_of: &BTreeMap<&ThreadId, Vec<&Thread>>,
    visited: &mut BTreeSet<ThreadId>,
    depth: u16,
    scope: &Scope,
) {
    if !visited.insert(thread.id.clone()) {
        return;
    }
    let children = children_of.contains_key(&thread.id);
    let expanded = !scope.sidebar.closed_threads.contains(&thread.id);
    rows.push(Row {
        kind: Kind::Thread {
            id: thread.id.clone(),
            tone: crate::kagan::attention::global(scope.cx)
                .read(scope.cx)
                .tone(thread, scope.cx),
            pinned: thread.pinned,
            muted: thread.notify == kg_domain::ThreadNotify::Muted,
            depth,
            children,
            expanded,
        },
        label: thread.title.clone().into(),
        detail: None,
        selected: scope.shown.as_ref() == Some(&thread.id),
        open: false,
    });
    for &child in children_of.get(&thread.id).into_iter().flatten() {
        if expanded {
            append_thread(rows, child, children_of, visited, depth + 1, scope);
        } else {
            mark_descendants(child, children_of, visited);
        }
    }
}

fn mark_descendants(
    thread: &Thread,
    children_of: &BTreeMap<&ThreadId, Vec<&Thread>>,
    visited: &mut BTreeSet<ThreadId>,
) {
    if !visited.insert(thread.id.clone()) {
        return;
    }
    for &child in children_of.get(&thread.id).into_iter().flatten() {
        mark_descendants(child, children_of, visited);
    }
}

fn project_selected(sidebar: &Sidebar, shows: &Shows, id: &ProjectId) -> bool {
    if shows.page.is_some() || shows.archived || shows.thread.is_some() {
        return false;
    }
    sidebar.chosen() == Some(id)
}
