use std::collections::{BTreeMap, BTreeSet};
use std::path::Component;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, IconName, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    WeakEntity, div, px, relative,
};
use kg_domain::{Project, ProjectId, ProjectKind, ThreadId};

use super::rows::icon_command;
use crate::kagan::{ArchiveThread, ThreadActions};

use crate::kagan::controls::{menu_popup, state_mark, tip};
use crate::theme::Tone;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

use super::{
    ActivateRow, Collapse, EnterRow, Event, Expand, FocusNextRow, FocusPreviousRow, Fold, Kind,
    Row, Sidebar,
};

pub(super) fn activate_event(kind: &Kind) -> Event {
    match kind {
        Kind::Project { id, .. } => Event::Project(id.clone()),
        Kind::Thread { id, .. } => Event::Thread(id.clone()),
    }
}

pub(super) fn enter_event(kind: &Kind) -> Event {
    match kind {
        Kind::Project { id, .. } => Event::Start(id.clone()),
        Kind::Thread { id, .. } => Event::Thread(id.clone()),
    }
}

pub(super) fn row_actions<
    E: gpui_kit::StatefulInteractiveElement + gpui_kit::prelude::FluentBuilder,
>(
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
        line.on_action(cx.listener(move |_, _: &crate::kagan::NewThread, _, cx| {
            cx.emit(Event::Project(project.clone()));
        }))
    })
    .on_click(cx.listener(move |_, _, _, cx| cx.emit(activate_event(&click))))
}

// At the narrowest sidebar a row is 175 points wide. Past three levels the indent alone would push
// the row's actions off the edge, so deeper threads share the third level's indent.
const INDENT_STEP: f32 = 24.;
const MAX_INDENT_LEVELS: u16 = 3;

pub(super) fn thread_indent(kind: &Kind) -> Option<f32> {
    match kind {
        Kind::Thread { depth, .. } if *depth > 0 => {
            Some(INDENT_STEP * f32::from((*depth).min(MAX_INDENT_LEVELS)))
        }
        _ => None,
    }
}

const PERSONAL_TIP: &str = "Threads that are not in a project. Each one gets its own folder.";

pub(super) fn row_tip(row: &Row) -> Option<SharedString> {
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

pub(super) fn project_text_ids(kind: &Kind, detail: bool) -> (Option<String>, Option<String>) {
    match kind {
        Kind::Project { id, .. } if detail => (
            Some(format!("project-name-{id}")),
            Some(format!("project-suffix-{id}")),
        ),
        _ => (None, None),
    }
}

pub(super) fn thread_controls(
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
            tip("Archive", &ArchiveThread, cx),
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
                    tip("Thread actions", &ThreadActions, cx),
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

pub(super) fn project_menu(
    menu: PopupMenu,
    project: ProjectId,
    view: WeakEntity<Sidebar>,
) -> PopupMenu {
    let rules_view = view.clone();
    let rules_project = project.clone();
    menu.item(
        PopupMenuItem::element(|_, _| {
            div()
                .id("project-agent-rules")
                .ui_observe_as(Facts {
                    role: Role::MenuItem,
                    ..Facts::button("Agent rules…")
                })
                .child("Agent rules…")
        })
        .on_click(move |_, _, cx| {
            _ = rules_view.update(cx, |_, cx| {
                cx.emit(Event::AgentRules(rules_project.clone()));
            });
        }),
    )
    .item(
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

pub(super) fn row_marks(
    kind: &Kind,
    cx: &mut App,
) -> (Option<SharedString>, Option<AnyElement>, Option<AnyElement>) {
    match kind {
        Kind::Thread {
            id,
            tone,
            pinned,
            muted,
            ..
        } => (
            Some(tone.word().into()),
            Some(thread_marks(id, *tone, cx)),
            (*pinned || *muted).then(|| {
                h_flex()
                    .flex_shrink_0()
                    .children(pinned.then(|| {
                        state_mark(
                            format!("thread-pinned-{id}"),
                            gpui_kit::assets::IconName::Pin,
                            "Pinned",
                            cx,
                        )
                    }))
                    .children(muted.then(|| {
                        state_mark(
                            format!("thread-muted-{id}"),
                            gpui_kit::assets::IconName::BellMinus,
                            "Notifications muted",
                            cx,
                        )
                    }))
                    .into_any_element()
            }),
        ),
        Kind::Project { .. } => (None, None, None),
    }
}

pub(super) fn row_glyph(icon: impl Into<Icon>, cx: &App) -> AnyElement {
    div()
        .w_4()
        .flex()
        .flex_shrink_0()
        .justify_center()
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(icon).small())
        .into_any_element()
}

pub(super) fn row_text(
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

pub(super) fn thread_marks(id: &ThreadId, tone: Tone, cx: &mut App) -> AnyElement {
    let word = tone.word();
    div()
        .id(ElementId::Name(format!("thread-mark-{id}").into()))
        .ui_container()
        .size_4()
        .flex()
        .flex_shrink_0()
        .justify_center()
        .items_center()
        .aria_label(word)
        .tooltip(move |window, cx| Tooltip::new(word).build(window, cx))
        .child(tone.mark(cx))
        .into_any_element()
}

// When no suffix tells same-named projects apart, the group gets none.
pub(super) fn parent_suffixes(projects: &[Project]) -> BTreeMap<ProjectId, String> {
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

pub(super) fn parent_parts(path: &std::path::Path) -> Vec<String> {
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

pub(super) fn suffix_of(parts: &[String], length: usize) -> String {
    if length == 0 || parts.len() < length {
        return String::new();
    }
    parts[parts.len() - length..].join("/")
}
