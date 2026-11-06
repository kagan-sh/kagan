use super::Kagan;

use super::{layout, picker, settings};
use gpui_kit::{App, Context, Entity, Window};

impl Kagan {
    #[cfg(test)]
    pub(crate) fn question_titles(&self, cx: &App) -> Vec<String> {
        self.questions
            .read(cx)
            .questions
            .iter()
            .map(|asked| asked.title.clone())
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn right_open(&self) -> bool {
        self.layout.right_open
    }

    #[cfg(test)]
    pub(crate) fn left_open(&self) -> bool {
        self.layout.left_open
    }

    /// Drops one panel from the registry, so a test can show that the sidebar draws from it.
    #[cfg(test)]
    pub(crate) fn drop_panel_for_test(&mut self, id: &str, cx: &mut Context<Self>) {
        self.panels.retain(|panel| panel.id != id);
        cx.notify();
    }

    /// The text a plugin's text setting shows on its page, for the tests.
    #[cfg(test)]
    pub(crate) fn settings_text(cx: &App, plugin: &kg_domain::PluginId, key: &str) -> String {
        settings::text_for_test(plugin, key, cx)
    }

    #[cfg(test)]
    pub(crate) fn settings_view_mounted(&self, id: &kg_domain::PluginId) -> bool {
        self.settings_views.contains_key(id)
    }

    /// The load generation a mounted settings view was made from, for the tests.
    #[cfg(test)]
    pub(crate) fn settings_view_generation(&self, id: &kg_domain::PluginId) -> Option<u64> {
        self.settings_views
            .get(id)
            .map(|(generation, _)| *generation)
    }

    #[cfg(test)]
    pub(crate) fn agent_integration(&self, cx: &App) -> bool {
        self.prefs.read(cx).agent_integration
    }

    /// Every registered right-sidebar panel's title, in order.
    #[cfg(test)]
    pub(crate) fn panel_titles(&self) -> Vec<String> {
        self.panels
            .iter()
            .map(|panel| panel.title.clone())
            .collect()
    }

    /// The width of the right panel as the group holds it now.
    #[cfg(test)]
    pub(crate) fn right_width_now(&self, cx: &App) -> f32 {
        self.columns
            .read(cx)
            .sizes()
            .get(layout::right_index(self.panes.left))
            .map_or(0., |width| f32::from(*width))
    }

    #[cfg(test)]
    pub(crate) fn resize_left(&self, width: gpui_kit::Pixels, window: &mut Window, cx: &mut App) {
        self.columns
            .update(cx, |state, cx| state.resize_panel(0, width, window, cx));
    }

    #[cfg(test)]
    pub(crate) fn resize_right(&self, width: gpui_kit::Pixels, window: &mut Window, cx: &mut App) {
        self.columns.update(cx, |state, cx| {
            state.resize_panel(layout::right_index(self.panes.left), width, window, cx);
        });
    }

    #[cfg(test)]
    pub(crate) fn picker_entity(&self) -> Entity<picker::Picker> {
        self.picker.clone()
    }

    #[cfg(test)]
    pub(crate) fn window_handle(&self) -> gpui_kit::AnyWindowHandle {
        self.window
    }
}
