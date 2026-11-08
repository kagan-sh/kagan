use super::{Kagan, layout, picker, settings};
use gpui_kit::{App, Context, Entity, Window};

pub(crate) use super::keys::{chord_for, is_chord};
pub(crate) use super::layout::read as layout_read;
pub(crate) use super::layout::tests as layout_read;
pub(crate) use super::needs::QUIET;
pub(crate) use super::replace::tests as replace_read;
pub(crate) use super::review::tests as review_read;
pub(crate) use super::screen::tests as screen_read;
pub(crate) use super::tabs::{
    CloseTerminal, NewTerminal, NextTerminal, OpenLocalLink, PreviousTerminal,
};
pub(crate) use super::threads::description;
#[cfg(target_os = "macos")]
pub(crate) use super::web_panel::{
    deliver as post, forget, inject, mark_stale, queue_page, testing_page,
};
pub(crate) use super::window::{open, window_options};

impl Kagan {
    pub(crate) fn question_titles(&self, cx: &App) -> Vec<String> {
        self.questions
            .read(cx)
            .questions
            .iter()
            .map(|asked| asked.title.clone())
            .collect()
    }

    pub(crate) fn right_open(&self) -> bool {
        self.layout.right_open
    }

    pub(crate) fn left_open(&self) -> bool {
        self.layout.left_open
    }

    /// Drops one panel from the registry, so a test can show that the sidebar draws from it.
    pub(crate) fn drop_panel_for_test(&mut self, id: &str, cx: &mut Context<Self>) {
        self.panels.retain(|panel| panel.tab.id() != id);
        cx.notify();
    }

    /// The text a plugin's text setting shows on its page, for the tests.
    pub(crate) fn settings_text(cx: &App, plugin: &kg_domain::PluginId, key: &str) -> String {
        settings::text_for_test(plugin, key, cx)
    }

    pub(crate) fn settings_view_mounted(&self, id: &kg_domain::PluginId) -> bool {
        self.settings_views.contains_key(id)
    }

    /// The load generation a mounted settings view was made from, for the tests.
    pub(crate) fn settings_view_generation(&self, id: &kg_domain::PluginId) -> Option<u64> {
        self.settings_views
            .get(id)
            .map(|(generation, _)| *generation)
    }

    pub(crate) fn agent_integration(&self, cx: &App) -> bool {
        self.prefs.read(cx).agent_integration
    }

    /// Every registered right-sidebar panel's title, in order.
    pub(crate) fn panel_titles(&self) -> Vec<String> {
        self.panels
            .iter()
            .map(|panel| panel.title.clone())
            .collect()
    }

    /// The width of the right panel as the group holds it now.
    pub(crate) fn right_width_now(&self, cx: &App) -> f32 {
        self.columns
            .read(cx)
            .sizes()
            .get(layout::right_index(self.panes.left))
            .map_or(0., |width| f32::from(*width))
    }

    pub(crate) fn resize_left(&self, width: gpui_kit::Pixels, window: &mut Window, cx: &mut App) {
        self.columns
            .update(cx, |state, cx| state.resize_panel(0, width, window, cx));
    }

    pub(crate) fn resize_right(&self, width: gpui_kit::Pixels, window: &mut Window, cx: &mut App) {
        self.columns.update(cx, |state, cx| {
            state.resize_panel(layout::right_index(self.panes.left), width, window, cx);
        });
    }

    pub(crate) fn picker_entity(&self) -> Entity<picker::Picker> {
        self.picker.clone()
    }

    pub(crate) fn window_handle(&self) -> gpui_kit::AnyWindowHandle {
        self.window
    }
}
