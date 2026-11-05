use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    Action, App, Context, InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _,
    Styled as _, Window, actions, div,
};
use kg_domain::PluginId;

use super::Kagan;
use super::tabs::{self};
use crate::ui_observe::UiObserveExt as _;

pub(super) const WIDTH: f32 = 320.;
pub(super) const WIDTH_RANGE: (f32, f32) = (240., 560.);

#[derive(Clone, PartialEq, Eq, Action)]
#[action(namespace = right, no_json)]
pub(super) struct ToggleDetails {
    pub(super) panel: String,
}

actions!(right, [ActivatePanelHeader, ToggleFill]);

/// Enter and Space on a focused tab, or on the `+`, act on it.
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
    kagan: &mut Kagan,
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

pub(super) fn selected_tab(kagan: &Kagan, cx: &App) -> Option<String> {
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

pub(super) fn tab_available(kagan: &Kagan, id: &str, cx: &App) -> bool {
    match id {
        "needs-you" => kagan.waiting_count(cx) > 0,
        "changes" | "details" => kagan.thread_visible(cx),
        tabs::BROWSER => kagan.tabs.browser && kagan.thread_visible(cx),
        _ => match tabs::terminal_number(id) {
            Some(number) => kagan.tabs.view(number).is_some() && kagan.thread_visible(cx),
            None => kagan.right_slots.iter().any(|slot| slot.id == id),
        },
    }
}

mod actions;
mod bar;
mod plugin;

use bar::{tab_bar, tab_body};
pub(super) use plugin::plugin_body;
