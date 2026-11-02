use crate::theme::TypeScale as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, Selectable as _, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, AnyElement, App, Context, DismissEvent, ElementId, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, Window, actions, div,
};
use kg_core::Workspace;
use kg_domain::{Op, PluginId, PluginOp, SurfaceOp};

use super::Kagan;
use super::plugins_box::{ReloadPlugin, ShowPluginLogs, TogglePluginEnabled, answer, send};
use super::shown::{control, menu_popup};
use super::surface::Slot;
use super::tabs::{self, CloseTab};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) const WIDTH: f32 = 320.;
pub(super) const WIDTH_RANGE: (f32, f32) = (240., 560.);

#[derive(Clone, PartialEq, Eq, Action)]
#[action(namespace = right, no_json)]
pub(super) struct ToggleDetails {
    pub(super) panel: String,
}

actions!(right, [ActivatePanelHeader]);

/// Enter and Space on a focused tab, or on the `+`, act on it.
pub(crate) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", ActivatePanelHeader, Some("PanelHeader")),
        KeyBinding::new("space", ActivatePanelHeader, Some("PanelHeader")),
    ]
}

#[derive(Clone)]
pub(super) enum Body {
    Needs,
    Changes,
    Details,
}

#[derive(Clone)]
pub(super) struct Panel {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) body: Body,
}

pub(super) fn built_in() -> Vec<Panel> {
    [
        ("needs-you", "Questions", Body::Needs),
        ("changes", "Changes", Body::Changes),
        ("details", "Details", Body::Details),
    ]
    .into_iter()
    .map(|(id, title, body)| Panel {
        id: id.into(),
        title: title.into(),
        body,
    })
    .collect()
}

pub(super) fn plugin_panel_id(plugin: &PluginId, panel: &str) -> String {
    format!("panel-{}-{panel}", plugin.0)
}

pub(super) fn render(
    kagan: &Kagan,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> impl IntoElement {
    let selected = selected_tab(kagan, cx);
    let body = selected
        .as_ref()
        .and_then(|id| tab_body(kagan, id, window, cx));
    v_flex()
        .size_full()
        .ui_container()
        .id("right-sidebar")
        .bg(cx.theme().sidebar)
        .child(tab_bar(kagan, selected.as_deref(), window, cx))
        .child(
            div()
                .id("right-sidebar-body")
                .ui_container()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .children(body),
        )
}

fn selected_tab(kagan: &Kagan, cx: &App) -> Option<String> {
    let want = kagan
        .right_tab
        .clone()
        .or_else(|| kagan.default_right_tab(cx))?;
    if tab_available(kagan, &want, cx) {
        Some(want)
    } else {
        kagan.default_right_tab(cx)
    }
}

fn tab_available(kagan: &Kagan, id: &str, cx: &App) -> bool {
    match id {
        "needs-you" => kagan.waiting_count(cx) > 0,
        "changes" | "details" => kagan.thread_visible(cx),
        _ => kagan.right_slots.iter().any(|slot| slot.id == id),
    }
}

fn tab_body(
    kagan: &Kagan,
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
    let slot = kagan.right_slots.iter().find(|slot| slot.id == id)?;
    Some(
        div()
            .id(ElementId::Name(format!("{}-body", slot.id).into()))
            .ui_container()
            .size_full()
            .p_3()
            .track_focus(&slot.focus)
            .child(plugin_body(slot, true, window, cx))
            .into_any_element(),
    )
}

fn tab_bar(
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
        .into_any_element()
}

fn built_in_tab(
    panel: &Panel,
    selected: Option<&str>,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let id = panel.id.clone();
    let focus = super::tab_stop(
        window,
        cx,
        ElementId::Name(format!("{id}-header-focus").into()),
    );
    let select = id.clone();
    div()
        .id(ElementId::Name(id.clone().into()))
        .ui_container()
        .flex_none()
        .child(tabs::tab(
            format!("{id}-header"),
            panel.title.clone().into(),
            selected == Some(id.as_str()),
            move |this, _, cx| this.select_right_tab(&select, cx),
            None,
            Some(focus),
            cx,
        ))
        .into_any_element()
}

fn plugin_tab(slot: &Slot, selected: Option<&str>, cx: &mut Context<Kagan>) -> AnyElement {
    let (plugin, id) = (slot.plugin.clone(), slot.contribution.clone());
    let close: CloseTab = (
        format!("right-tab-close-{}", slot.id).into(),
        Box::new(move |cx| {
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
    let enabled = kagan.closed_right_panels().next().is_some() || menu.is_some();
    let focus = kagan.right_add_focus.clone();
    div()
        .id("right-tab-plus")
        .relative()
        .flex_shrink_0()
        .track_focus(&focus)
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
                ..Facts::button("Add panel")
            },
            Button::new("right-tab-add-button")
                .small()
                .ghost()
                .icon(IconName::Plus)
                .disabled(!enabled)
                .selected(menu.is_some())
                .accessibility_label("Add panel")
                .tooltip("Add panel")
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
                "Add panel",
            ))
        })
        .into_any_element()
}

impl Kagan {
    pub(super) fn thread_visible(&self, cx: &App) -> bool {
        self.shown_thread(cx).is_some() && !self.shows_archived() && self.screen.page().is_none()
    }

    pub(super) fn has_right_content(&self, cx: &App) -> bool {
        self.thread_visible(cx)
            || self.waiting_count(cx) > 0
            || !self.right_slots.is_empty()
            || !self.right_panels.is_empty()
    }

    pub(super) fn open_right_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.right_menu.is_some() {
            return;
        }
        let items: Vec<_> = self
            .closed_right_panels()
            .map(|item| (item.plugin.clone(), item.id.clone(), item.title.clone()))
            .collect();
        if items.is_empty() {
            return;
        }
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            items.into_iter().fold(menu, |menu, (plugin, id, title)| {
                let item_id = format!("right-add-{}-{id}", plugin.0);
                menu.item(
                    PopupMenuItem::element(move |_, _| {
                        control(
                            ElementId::Name(item_id.clone().into()),
                            Facts {
                                role: Role::MenuItem,
                                ..Facts::button(title.clone())
                            },
                            title.clone(),
                        )
                    })
                    .on_click(move |_, _, cx| {
                        Workspace::dispatch(
                            Op::Surface(SurfaceOp::Open {
                                plugin: plugin.clone(),
                                id: id.clone(),
                            }),
                            cx,
                        )
                        .detach();
                    }),
                )
            })
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, _, cx| {
            this.right_menu = None;
            cx.notify();
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.right_menu = Some((menu, dismiss));
        cx.notify();
    }
}

/// The recovery buttons under a failed card. Each one sends its action from where it has focus.
fn recovery_row(id: &str, details: bool, focus: &FocusHandle) -> AnyElement {
    h_flex()
        .flex_wrap()
        .gap_1()
        .child(send(format!("{id}-reload"), "Reload", focus, ReloadPlugin))
        .child(send(format!("{id}-logs"), "Logs", focus, ShowPluginLogs))
        .child(send(
            format!("{id}-disable"),
            "Disable",
            focus,
            TogglePluginEnabled,
        ))
        .child(send(
            format!("{id}-details"),
            if details { "Hide details" } else { "Details" },
            focus,
            ToggleDetails {
                panel: id.to_owned(),
            },
        ))
        .into_any_element()
}

pub(super) fn plugin_body(
    slot: &Slot,
    fill: bool,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let error = slot.error.clone().or_else(|| {
        slot.view
            .as_ref()?
            .read(cx)
            .build_error()
            .map(ToString::to_string)
    });
    match (error, &slot.view) {
        (Some(error), _) => failed_plugin(slot, &error, window, cx),
        (None, Some(view)) if fill => div().size_full().child(view.clone()).into_any_element(),
        (None, Some(view)) => div().child(view.clone()).into_any_element(),
        (None, None) => div().into_any_element(),
    }
}

fn failed_plugin(
    slot: &Slot,
    error: &str,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let (id, title, plugin, details) = (slot.id.clone(), &slot.title, &slot.plugin, slot.details);
    let reason = error.lines().next().unwrap_or_default().to_owned();
    let focus = super::tab_stop(
        window,
        cx,
        ElementId::Name(format!("{id}-failed-focus").into()),
    );
    // Only the card text carries the row keys. The buttons sit beside it and send their actions
    // from where they have focus, so Space on a button never reaches the row.
    let card = div()
        .id(ElementId::Name(format!("{id}-failed").into()))
        .ui_observe_as(Facts {
            role: Role::ListBoxOption,
            focus: Some(focus.clone()),
            ..Facts::button(title.to_owned())
        })
        .track_focus(&focus)
        .tab_stop(true)
        .key_context("PluginRow")
        .rounded(cx.theme().radius)
        .focus(|card| card.border_1().border_color(cx.theme().ring))
        .flex_col()
        .gap_1()
        .child(div().type_body().child(format!("Could not show {title}")))
        .child(
            div()
                .type_small()
                .text_color(cx.theme().muted_foreground)
                .child(reason),
        );
    let body_focus = window
        .use_keyed_state(
            ElementId::Name(format!("{id}-failed-body-focus").into()),
            cx,
            |_, cx| cx.focus_handle(),
        )
        .read(cx)
        .clone();
    let body = v_flex()
        .id(ElementId::Name(format!("{id}-failed-body").into()))
        .track_focus(&body_focus)
        .gap_2()
        .child(card);
    let body = answer::<ReloadPlugin, _>(body, plugin, cx, |this, id, cx| {
        let op = PluginOp::Reload {
            id: id.clone(),
            thread_id: None,
        };
        this.plugin_op(op, "reload the plugin", |_, _, _| {}, cx);
    });
    let logs_id = plugin.clone();
    let body = body.on_action(cx.listener(move |this, _: &ShowPluginLogs, window, cx| {
        this.open_plugin_logs(&logs_id, window, cx);
    }));
    let body = answer::<TogglePluginEnabled, _>(body, plugin, cx, |this, id, cx| {
        this.toggle_plugin_enabled(id, cx);
    });
    let mut body = body.child(recovery_row(&id, details, &body_focus));
    if details {
        body = body.child(
            div()
                .id(ElementId::Name(format!("{id}-error").into()))
                .ui_observe_as(Facts {
                    role: Role::Label,
                    value: Some(error.to_owned().into()),
                    ..Facts::button("Error")
                })
                .type_small()
                .child(error.to_owned()),
        );
    }
    body.into_any_element()
}

impl Kagan {
    pub(super) fn toggle_details_action(
        &mut self,
        action: &ToggleDetails,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flip_slot_details(&action.panel);
        cx.notify();
    }
}
