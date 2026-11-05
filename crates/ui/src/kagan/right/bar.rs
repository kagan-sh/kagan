use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, Selectable as _, Sizable as _, h_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use kg_core::Workspace;
use kg_domain::{Op, SurfaceOp};

use super::tabs::{self, CloseTab};
use super::{ActivatePanelHeader, Body, Fill, Panel, ToggleFill, plugin_body, tab_available};
use crate::components::fills_panel;
use crate::kagan::controls::{control, menu_popup};
use crate::kagan::surface::Slot;
use crate::kagan::{Kagan, tab_stop};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
pub(super) fn tab_body(
    kagan: &mut Kagan,
    id: &str,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> Option<AnyElement> {
    if let Some(panel) = kagan.panels.iter().find(|panel| panel.id == id) {
        let body = match &panel.body {
            Body::Needs => kagan.render_needs(window, cx).into_any_element(),
            Body::Changes => kagan.render_changes(cx).into_any_element(),
            Body::Details => kagan.render_details(cx),
        };
        return Some(
            div()
                .id(ElementId::Name(format!("{id}-body").into()))
                .ui_container()
                .p_3()
                .child(body)
                .into_any_element(),
        );
    }
    if id == tabs::BROWSER {
        return Some(kagan.render_browser(cx));
    }
    if let Some(number) = tabs::terminal_number(id) {
        let view = kagan.tabs.view(number)?.clone();
        return Some(div().size_full().child(view).into_any_element());
    }
    let slot = kagan.right_slots.iter().find(|slot| slot.id == id)?;
    let page = slot.web.is_some() || slot.view.as_ref().is_some_and(|view| fills_panel(view, cx));
    if !page {
        crate::components::nudge_once(window, cx);
    }
    Some(
        div()
            .id(ElementId::Name(format!("{}-body", slot.id).into()))
            .ui_container()
            .size_full()
            .when(!page, gpui_kit::Styled::p_3)
            .when(page, gpui_kit::Styled::overflow_hidden)
            .track_focus(&slot.focus)
            .child(plugin_body(slot, true, window, cx))
            .into_any_element(),
    )
}

pub(super) fn tab_bar(
    kagan: &Kagan,
    selected: Option<&str>,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let mut tabs = Vec::new();
    for panel in &kagan.panels {
        if tab_available(kagan, &panel.id, cx) {
            tabs.push(built_in_tab(panel, selected, window, cx));
        }
    }
    for slot in &kagan.right_slots {
        tabs.push(plugin_tab(slot, selected, cx));
    }
    if kagan.thread_visible(cx) {
        tabs.extend(kagan.right_tabs(selected, cx));
    }
    h_flex()
        .id("right-tab-bar")
        .ui_container()
        .flex_none()
        .w_full()
        .min_w_0()
        .h_9()
        .gap_2()
        .px_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .id("right-tabs")
                .ui_container()
                .role(Role::TabList)
                .min_w_0()
                .flex_1()
                .overflow_x_scroll()
                .gap_1()
                .children(tabs),
        )
        .child(plus_button(kagan, cx))
        .when(kagan.panes.right, |bar| bar.child(full_button(kagan)))
        .into_any_element()
}

fn full_button(kagan: &Kagan) -> impl IntoElement {
    let (icon, label) = match kagan.right_fill {
        Fill::Column => (IconName::Maximize, "Full screen"),
        Fill::Window => (IconName::Minimize, "Exit full screen"),
    };
    control(
        "right-full",
        Facts {
            selected: Some(kagan.right_fill == Fill::Window),
            ..Facts::button(label)
        },
        Button::new("right-full-button")
            .small()
            .ghost()
            .icon(icon)
            .accessibility_label(label)
            .tooltip(label)
            .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleFill), cx)),
    )
}

fn built_in_tab(
    panel: &Panel,
    selected: Option<&str>,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let id = panel.id.clone();
    let focus = tab_stop(
        window,
        cx,
        ElementId::Name(format!("{id}-header-focus").into()),
    );
    let active = selected == Some(id.as_str());
    let header = format!("{id}-header");
    let take = focus.clone();
    let select = id.clone();
    let tab = tabs::tab(
        header,
        panel.title.clone().into(),
        active,
        move |this, window, cx| {
            window.focus(&take, cx);
            this.select_right_tab(&select, cx);
        },
        None,
        Some(focus),
        cx,
    );
    div()
        .id(ElementId::Name(id.into()))
        .ui_container()
        .flex_none()
        .child(tab)
        .into_any_element()
}

fn plugin_tab(slot: &Slot, selected: Option<&str>, cx: &mut Context<Kagan>) -> AnyElement {
    let (plugin, id) = (slot.plugin.clone(), slot.contribution.clone());
    let close: CloseTab = (
        format!("right-tab-close-{}", slot.id).into(),
        Box::new(move |_, _, cx| {
            Workspace::dispatch(
                Op::Surface(SurfaceOp::Close {
                    plugin: plugin.clone(),
                    id: id.clone(),
                }),
                cx,
            )
            .detach();
        }),
    );
    let select = slot.id.clone();
    tabs::tab(
        format!("right-tab-{}", slot.id),
        slot.title.clone().into(),
        selected == Some(slot.id.as_str()),
        move |this, _, cx| this.select_right_tab(&select, cx),
        Some(close),
        None,
        cx,
    )
}

fn plus_button(kagan: &Kagan, cx: &mut Context<Kagan>) -> AnyElement {
    let menu = kagan.right_menu.as_ref().map(|(menu, _)| menu.clone());
    let enabled = kagan.closed_right_panels().next().is_some()
        || kagan.can_open_browser(cx)
        || kagan.can_open_terminal(cx)
        || menu.is_some();
    let focus = kagan.right_add_focus.clone();
    div()
        .id("right-tab-plus")
        .relative()
        .flex_shrink_0()
        .track_focus(&focus)
        .tab_stop(true)
        .key_context("PanelHeader")
        .rounded(cx.theme().radius)
        .focus(|add| add.border_1().border_color(cx.theme().ring))
        .on_action(cx.listener(|this, _: &ActivatePanelHeader, window, cx| {
            this.open_right_menu(window, cx);
        }))
        .child(control(
            "right-tab-add",
            Facts {
                disabled: !enabled,
                selected: Some(menu.is_some()),
                focus: Some(focus),
                ..Facts::button("Add tab")
            },
            Button::new("right-tab-add-button")
                .small()
                .ghost()
                .icon(IconName::Plus)
                .disabled(!enabled)
                .selected(menu.is_some())
                .accessibility_label("Add tab")
                .tooltip("Add tab")
                .tab_stop(false)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_right_menu(window, cx);
                })),
        ))
        .when_some(menu, |wrap, menu| {
            wrap.child(menu_popup(
                &menu,
                "right-tab-menu-layer",
                "right-tab-menu",
                "Add tab",
            ))
        })
        .into_any_element()
}
