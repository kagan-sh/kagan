use crate::theme::TypeScale as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    Action, AnyElement, App, Context, ElementId, FocusHandle, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement as _, Role, Styled as _, Window, actions, div,
};
use kg_domain::{PluginId, PluginOp};

use super::Kagan;
use super::layout;
use super::plugins_box::{ReloadPlugin, ShowPluginLogs, TogglePluginEnabled, answer, send};
use super::shown::control;
use super::surface::Slot;
use super::tabs::RightPage;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) const WIDTH: f32 = 320.;
pub(super) const WIDTH_RANGE: (f32, f32) = (240., 560.);

#[derive(Clone, PartialEq, Eq, Action)]
#[action(namespace = right, no_json)]
pub(super) struct TogglePanel {
    pub(super) panel: String,
}

#[derive(Clone, PartialEq, Eq, Action)]
#[action(namespace = right, no_json)]
pub(super) struct ToggleDetails {
    pub(super) panel: String,
}

actions!(right, [ActivatePanelHeader]);

/// Enter and Space on a focused panel header collapse or open that panel.
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
    pub(super) collapsed: bool,
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
        collapsed: false,
    })
    .collect()
}

fn panel_key(id: &str) -> String {
    format!("panel.{id}.collapsed")
}

pub(super) fn plugin_panel_id(plugin: &PluginId, panel: &str) -> String {
    format!("panel-{}-{panel}", plugin.0)
}

pub(super) fn render(kagan: &Kagan, window: &mut Window, cx: &mut Context<Kagan>) -> AnyElement {
    let thread = kagan.thread_visible(cx);
    let tabs = thread.then(|| kagan.render_right_tabs(cx));
    let needs = kagan
        .panels
        .iter()
        .find(|panel| matches!(panel.body, Body::Needs))
        .filter(|_| kagan.waiting_count(cx) > 0)
        .map(|panel| frame(panel, kagan, window, cx));
    let body = match kagan.tabs.right_page() {
        RightPage::Browser if thread => Some(kagan.render_browser(cx)),
        RightPage::Terminal(number) if thread => kagan
            .tabs
            .view(number)
            .map(|view| div().size_full().child(view).into_any_element()),
        RightPage::Panels | RightPage::Browser | RightPage::Terminal(_) => None,
    };
    if let Some(body) = body {
        return v_flex()
            .size_full()
            .ui_container()
            .id("right-sidebar")
            .bg(cx.theme().sidebar)
            .children(tabs)
            .children(needs)
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element();
    }
    let changes = kagan
        .panels
        .iter()
        .find(|panel| matches!(panel.body, Body::Changes))
        .filter(|_| thread)
        .map(|panel| frame(panel, kagan, window, cx));
    let details = kagan
        .panels
        .iter()
        .find(|panel| matches!(panel.body, Body::Details))
        .filter(|_| thread)
        .map(|panel| frame(panel, kagan, window, cx));
    v_flex()
        .size_full()
        .ui_container()
        .overflow_y_scrollbar()
        .id("right-sidebar")
        .bg(cx.theme().sidebar)
        .children(tabs)
        .children(needs)
        .children(changes)
        .children(details)
        .into_any_element()
}

impl Kagan {
    pub(super) fn thread_visible(&self, cx: &App) -> bool {
        self.shown_thread(cx).is_some() && !self.shows_archived() && self.screen.page().is_none()
    }

    pub(super) fn has_right_content(&self, cx: &App) -> bool {
        self.thread_visible(cx) || self.waiting_count(cx) > 0
    }
}

fn frame(panel: &Panel, kagan: &Kagan, window: &mut Window, cx: &mut Context<Kagan>) -> AnyElement {
    let (icon, verb) = if panel.collapsed {
        (IconName::ChevronDown, "Show")
    } else {
        (IconName::ChevronUp, "Hide")
    };
    let (id, title) = (panel.id.clone(), panel.title.clone());
    let toggle_id = id.clone();
    let toggle = control(
        format!("panel-toggle-{id}"),
        Facts {
            selected: Some(!panel.collapsed),
            ..Facts::button(format!("{verb} {title}"))
        },
        Button::new(ElementId::Name(format!("button-panel-toggle-{id}").into()))
            .small()
            .ghost()
            .icon(icon)
            .accessibility_label(format!("{verb} {title}"))
            .on_click(move |_, window, cx| {
                window.dispatch_action(
                    Box::new(TogglePanel {
                        panel: toggle_id.clone(),
                    }),
                    cx,
                );
            }),
    );
    let header_focus = super::tab_stop(
        window,
        cx,
        ElementId::Name(format!("{id}-header-focus").into()),
    );
    let activate_id = id.clone();
    let label = div()
        .id(ElementId::Name(format!("{id}-header").into()))
        .ui_observe_as(Facts {
            focus: Some(header_focus.clone()),
            ..Facts::button(panel.title.clone())
        })
        .track_focus(&header_focus)
        .tab_stop(true)
        .key_context("PanelHeader")
        .rounded(cx.theme().radius)
        .focus(|header| header.border_1().border_color(cx.theme().ring))
        .type_caption()
        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(title)
        .on_action(cx.listener(move |_, _: &ActivatePanelHeader, window, cx| {
            window.dispatch_action(
                Box::new(TogglePanel {
                    panel: activate_id.clone(),
                }),
                cx,
            );
        }));
    let header = h_flex()
        .justify_between()
        .items_center()
        .child(label)
        .child(toggle);
    let mut frame = v_flex()
        .id(ElementId::Name(id.into()))
        .ui_container()
        .p_3()
        .gap_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(header);
    if !panel.collapsed {
        let body = match &panel.body {
            Body::Needs => kagan.render_needs(window, cx).into_any_element(),
            Body::Changes => kagan.render_changes(cx).into_any_element(),
            Body::Details => kagan.render_details(cx),
        };
        frame = frame.child(
            div()
                .id(ElementId::Name(format!("{}-body", panel.id).into()))
                .ui_container()
                .child(body),
        );
    }
    frame.into_any_element()
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
    pub(super) fn toggle_panel(&mut self, id: &str, cx: &mut Context<Self>) {
        let collapsed = self
            .panels
            .iter()
            .find(|panel| panel.id == id)
            .is_some_and(|panel| !panel.collapsed);
        self.set_collapsed(id, collapsed, cx);
    }

    /// Shows or hides one panel, and saves the choice so a restart and a plugin's own open keep it.
    pub(super) fn set_collapsed(&mut self, id: &str, collapsed: bool, cx: &mut Context<Self>) {
        let changed = self
            .panels
            .iter()
            .any(|panel| panel.id == id && panel.collapsed != collapsed);
        self.bump_collapse(id);
        if let Some(panel) = self.panels.iter_mut().find(|panel| panel.id == id) {
            panel.collapsed = collapsed;
        }
        if changed {
            self.supersede_surface();
        }
        if !self.surface_hold {
            layout::save(&panel_key(id), collapsed, cx);
        }
        cx.notify();
    }

    pub(super) fn restore_collapsed(&self, id: String, cx: &mut Context<Self>) {
        let stamp = self.stamped(&id);
        let read = layout::read::<bool>(&panel_key(&id), cx);
        cx.spawn(async move |this, cx| {
            let collapsed = read.await.unwrap_or(false);
            _ = this.update(cx, |this, cx| {
                if this.stamped(&id) != stamp {
                    return;
                }
                let changed = this
                    .panels
                    .iter()
                    .any(|panel| panel.id == id && panel.collapsed != collapsed);
                if !changed {
                    return;
                }
                if let Some(panel) = this.panels.iter_mut().find(|panel| panel.id == id) {
                    panel.collapsed = collapsed;
                }
                this.supersede_surface();
                cx.notify();
            });
        })
        .detach();
    }

    fn stamped(&self, id: &str) -> u64 {
        self.collapse_stamp.get(id).copied().unwrap_or(0)
    }

    fn bump_collapse(&mut self, id: &str) {
        let next = self.stamped(id).wrapping_add(1);
        self.collapse_stamp.insert(id.to_owned(), next);
    }

    pub(super) fn toggle_panel_action(
        &mut self,
        action: &TogglePanel,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_panel(&action.panel, cx);
    }

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
