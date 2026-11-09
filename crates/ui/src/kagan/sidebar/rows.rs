use crate::theme::TypeScale as _;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, IconName, Sizable as _, StyledExt as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, DragMoveEvent, ElementId, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div, px,
};
use kg_domain::{ProjectId, ProjectKind};

use super::order::{DragProject, Place};

use crate::kagan::controls::{control, tip};
use crate::theme::{SpaceStep, control_height, radius_sm, row_height, space};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

use super::marks::{
    project_menu, project_text_ids, row_actions, row_marks, row_text, row_tip, thread_controls,
    thread_indent,
};
use super::{Event, Kind, Row, Sidebar};

pub(super) fn project_line(
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
            .ml(px(space(SpaceStep::Sm, cx)))
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
                tip(&started, &crate::kagan::NewThread, cx),
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

pub(super) fn project_shell(
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
        .h(px(control_height(cx)))
        .px(px(space(SpaceStep::Sm, cx)))
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

pub(super) fn reorder_line(
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
        let preview = super::order::preview(drag, cx);
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
                let place = super::order::place_at(event.bounds, event.event.position);
                if super::order::noop_gap(&from, &move_target, place, &this.listing.projects) {
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
        if let Some(before) = super::order::before_of(&this.listing.projects, &drop_target, place) {
            cx.emit(Event::Reorder {
                id: drag.id.clone(),
                before: before.into_option(),
            });
        }
        cx.notify();
    }))
}

pub(super) fn icon_command(
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

pub(super) fn icon_button(
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

pub(super) fn row_identity(kind: &Kind) -> String {
    match kind {
        Kind::Project { id, .. } => format!("{}{id}", super::PROJECT_ROW),
        Kind::Thread { id, .. } => format!("{}{id}", super::THREAD_ROW),
    }
}

pub(super) fn row_line(
    row: &Row,
    sidebar: &mut Sidebar,
    inset: bool,
    window: &mut Window,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let id = row_identity(&row.kind);
    let focus = crate::kagan::tab_stop(window, cx, SharedString::from(id.clone()));
    sidebar.add_row(id.clone(), focus.clone());
    let focused = focus.is_focused(window);
    let popup = match &row.kind {
        Kind::Thread { id, .. } => sidebar.menu_for(id),
        Kind::Project { .. } => None,
    };
    let indent = thread_indent(&row.kind);
    div()
        .w_full()
        .when_some(indent, |wrap, depth| wrap.pl(px(depth)))
        .child(row_face(row, id, focus, focused, inset, popup, cx))
        .into_any_element()
}

pub(super) fn row_face(
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
    let height = if project {
        control_height(cx)
    } else {
        row_height(cx)
    };
    let face = div()
        .id(ElementId::Name(id.into()))
        .group("thread-row")
        .key_context("SidebarRow")
        .role(Role::TreeItem)
        .aria_label(row.label.clone())
        .aria_selected(row.selected)
        .h(px(height))
        .w_full()
        .min_h(px(height))
        .max_h(px(height))
        .flex_shrink_0()
        .when(inset, |line| line.px(px(space(SpaceStep::Sm, cx))))
        .when(thread_indent(&kind).is_some(), |line| {
            line.border_l_1().border_color(cx.theme().border)
        })
        .gap(px(space(SpaceStep::Sm, cx)))
        .flex()
        .items_center()
        .rounded(px(radius_sm(cx)))
        .type_body(cx)
        .when(project, |line| {
            line.type_small(cx).text_color(cx.theme().muted_foreground)
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
