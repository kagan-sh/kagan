use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    Action, App, Context, InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _,
    Styled as _, Window, actions, div,
};
use kg_domain::PluginId;

use super::Kagan;
use super::tabs;
use crate::ui_observe::UiObserveExt as _;

mod tab;
pub(super) use tab::Tab;

pub(super) const WIDTH: f32 = 320.;
pub(super) const WIDTH_RANGE: (f32, f32) = kg_domain::prefs::RIGHT_WIDTH_RANGE;

pub(super) struct Sidebar {
    pub(super) slots: Vec<super::surface::Slot>,
    pub(super) tab: Option<Tab>,
    pub(super) menu: Option<(
        gpui_kit::Entity<gpui_kit::component::menu::PopupMenu>,
        gpui_kit::Subscription,
    )>,
    pub(super) add_focus: gpui_kit::FocusHandle,
    pub(super) fill: Fill,
    pub(super) panels: Vec<super::surface::Declared>,
    pub(super) toggle_focus: gpui_kit::FocusHandle,
}

impl Sidebar {
    pub(super) fn new(cx: &mut gpui_kit::App) -> Self {
        Self {
            slots: Vec::new(),
            tab: None,
            menu: None,
            add_focus: cx.focus_handle().tab_stop(true),
            fill: Fill::Column,
            panels: Vec::new(),
            toggle_focus: cx.focus_handle().tab_stop(true),
        }
    }
}

pub(super) fn header_focus(id: &str) -> String {
    format!("{id}-header-focus")
}

#[derive(Clone, PartialEq, Eq, Action)]
#[action(namespace = right, no_json)]
pub(super) struct ToggleDetails {
    pub(super) panel: String,
}

actions!(right, [ActivatePanelHeader, ToggleFill]);

pub(crate) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", ActivatePanelHeader, Some("PanelHeader")),
        KeyBinding::new("space", ActivatePanelHeader, Some("PanelHeader")),
    ]
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Fill {
    Column,
    Window,
}

pub(super) const BUILT_IN: [(Tab, &str); 3] = [
    (Tab::Questions, "Questions"),
    (Tab::Changes, "Changes"),
    (Tab::Details, "Details"),
];

pub(super) fn plugin_panel_id(plugin: &PluginId, panel: &str) -> String {
    format!("panel-{}-{panel}", plugin.0)
}

pub(super) fn render(
    kagan: &mut Kagan,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> impl IntoElement {
    let selected = selected_tab(kagan, cx);
    let body = selected
        .as_ref()
        .and_then(|tab| tab_body(kagan, tab, window, cx));
    v_flex()
        .size_full()
        .ui_container()
        .id("right-sidebar")
        .bg(cx.theme().sidebar)
        .child(tab_bar(kagan, selected.as_ref(), window, cx))
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

pub(super) fn selected_tab(kagan: &Kagan, cx: &App) -> Option<Tab> {
    let want = kagan
        .right
        .tab
        .clone()
        .or_else(|| kagan.default_right_tab(cx))?;
    if want.available(kagan, cx) {
        Some(want)
    } else {
        kagan.default_right_tab(cx)
    }
}

mod actions;
mod bar;
mod plugin;

use bar::{tab_bar, tab_body};
pub(super) use plugin::plugin_body;
