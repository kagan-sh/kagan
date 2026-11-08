use super::{ActivateRow, EnterRow, ManagePlugins, NewPlugin, Threads, surface_event};
use crate::theme::TypeScale as _;
use std::collections::BTreeSet;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Icon, IconName, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, ElementId, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    div, px,
};
use kg_domain::Thread;

use crate::kagan::surface::{self, OpenSurface};

use crate::kagan::controls::{control, tip};
use crate::theme::{SpaceStep, Tone, radius_xs, space};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

use super::{ArchivedFooter, Event, Shows, Sidebar};

pub(in crate::kagan) fn rail(
    sidebar: &mut Sidebar,
    shows: &Shows,
    window: &mut Window,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let manage_focus = crate::kagan::tab_stop(window, cx, "plugins-manage-focus");
    sidebar
        .row_focus
        .insert("plugins-manage".into(), manage_focus.clone());
    let mut plugins = BTreeSet::new();
    let (pages, tools): (Vec<_>, Vec<_>) = surface::nav_items(cx)
        .into_iter()
        .partition(|item| item.page);
    let tools: Vec<_> = tools
        .into_iter()
        .filter(|item| plugins.insert(item.plugin.clone()))
        .map(|item| rail_tool(sidebar, shows, item, window, cx))
        .collect();
    let pages: Vec<_> = pages
        .into_iter()
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
        .w(px(crate::kagan::layout::RAIL_WIDTH))
        .h_full()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .pt(px(space(SpaceStep::Md, cx)))
        .pb(px(space(SpaceStep::Md, cx)))
        .bg(cx.theme().sidebar)
        .child(super::brand_menu::button(sidebar, cx))
        .child(top)
        .child(crate::kagan::commands::rail_button(cx))
        .child(gear(shows.in_settings, cx))
        .into_any_element()
}

pub(super) fn gear(active: bool, cx: &mut Context<Sidebar>) -> impl IntoElement {
    let tip = crate::kagan::controls::tip("Settings", &crate::kagan::OpenSettings, cx);
    let button = Button::new("settings-gear-button")
        .small()
        .rounded(px(radius_xs(cx)))
        .icon(IconName::Settings)
        .accessibility_label("Settings")
        .tooltip(tip)
        .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::Settings)));
    let button = button.ghost().selected(active);
    crate::kagan::controls::control(
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
pub(super) fn rail_tool(
    sidebar: &mut Sidebar,
    shows: &Shows,
    item: surface::NavItem,
    window: &mut Window,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let id = surface::row_id(&item.plugin, &item.id);
    let focus = crate::kagan::tab_stop(window, cx, SharedString::from(id.clone()));
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
        plugin: item.plugin,
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
                surface::row_id(&action.plugin, &action.id)
            )))
            .small()
            .rounded(px(radius_xs(cx)))
            .ghost()
            .selected(selected)
            .icon(icon)
            .accessibility_label(title.clone())
            .tooltip(title.clone())
            .tab_stop(false)
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(surface_event(page, selected, &action));
            })),
        )
        .into_any_element()
}

pub(super) fn threads_home(shows: &Shows, cx: &mut Context<Sidebar>) -> impl IntoElement {
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
            .type_caption(cx)
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
                .rounded(px(radius_xs(cx)))
                .ghost()
                .selected(shows.tool.is_none() && !shows.in_settings && shows.page.is_none())
                .icon(gpui_kit::assets::IconName::House)
                .accessibility_label("Threads")
                .tooltip(tip("Threads", &Threads, cx))
                .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::Threads))),
        ))
        .children(waiting)
}

pub(super) fn manage_row(manage_focus: FocusHandle, cx: &mut Context<Sidebar>) -> impl IntoElement {
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
                .rounded(px(radius_xs(cx)))
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

pub(super) fn new_plugin_row(cx: &mut Context<Sidebar>) -> impl IntoElement {
    div().flex_shrink_0().child(control(
        "plugins-add",
        Facts::button("New plugin…"),
        Button::new("plugins-add-button")
            .small()
            .rounded(px(radius_xs(cx)))
            .ghost()
            .icon(IconName::Plus)
            .accessibility_label("New plugin…")
            .tooltip(tip("New plugin…", &NewPlugin, cx))
            .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::NewPlugin))),
    ))
}

pub(super) fn footer(
    sidebar: &Sidebar,
    window: &mut Window,
    cx: &mut Context<Sidebar>,
) -> impl IntoElement {
    let archived_focus = crate::kagan::tab_stop(window, cx, "archived-focus");
    h_flex()
        .id("sidebar-add")
        .flex_shrink_0()
        .w_full()
        .pt(px(space(SpaceStep::Sm, cx)))
        .pb(px(space(SpaceStep::Md, cx)))
        .px(px(space(SpaceStep::Md, cx)))
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
                    let keys = crate::kagan::controls::bound_keys(&crate::kagan::AddProject, cx);
                    if keys.is_empty() {
                        "Add project".to_owned()
                    } else {
                        format!("Add project ({keys})")
                    }
                })
                .tab_stop(false)
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(crate::kagan::AddProject), cx);
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

// Same test as the Archived list, so the footer never opens an empty list.
pub(in crate::kagan) fn archived_footer(records: &[Thread]) -> ArchivedFooter {
    let any = records.iter().any(crate::kagan::archived::in_archived_list);
    if any {
        ArchivedFooter::Shown
    } else {
        ArchivedFooter::Hidden
    }
}
