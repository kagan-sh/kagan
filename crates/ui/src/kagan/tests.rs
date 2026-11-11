use super::{Kagan, layout, picker, right, settings};
use gpui_kit::{App, Context, Entity, Window};

pub(crate) use super::keys::{chord_for, is_chord};
pub(crate) use super::layout::read as layout_read;
pub(crate) use super::layout::tests as layout_read;
pub(crate) use super::replace::tests as replace_read;
pub(crate) use super::review::tests as review_read;
pub(crate) use super::screen::tests as screen_read;
#[cfg(unix)]
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

    /// The text a plugin's text setting shows on its page, for the tests.
    pub(crate) fn settings_text(cx: &App, plugin: &kg_domain::PluginId, key: &str) -> String {
        settings::text_for_test(plugin, key, cx)
    }

    pub(crate) fn settings_view_mounted(&self, id: &kg_domain::PluginId) -> bool {
        self.slots.settings_views.contains_key(id)
    }

    /// The load generation a mounted settings view was made from, for the tests.
    pub(crate) fn settings_view_generation(&self, id: &kg_domain::PluginId) -> Option<u64> {
        self.slots
            .settings_views
            .get(id)
            .map(|(generation, _)| *generation)
    }

    pub(crate) fn agent_integration(&self, cx: &App) -> bool {
        self.prefs.read(cx).agent_integration
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

    /// Whether the browser page was last told to show, read without a frame.
    pub(crate) fn browser_shown_for_test(&self) -> bool {
        self.browser.shown
    }

    /// Takes the right panel out of the layout, the step a closed sidebar takes, without a frame.
    pub(crate) fn close_right_panel_for_test(&mut self, cx: &mut Context<Self>) {
        let left = self.panes.left;
        self.sync_to(layout::Panes { left, right: false }, cx);
    }

    pub(crate) fn select_changes_tab_for_test(&mut self, cx: &mut Context<Self>) {
        self.select_right_tab(right::Tab::Changes, cx);
    }

    /// Makes `thread` the one driving the browser. No native page is built, so the tests can read the state.
    pub(crate) fn drive_browser_for_test(&mut self, thread: kg_domain::ThreadId, url: &str) {
        self.browser.agent = Some(super::browser::Agent {
            thread,
            url: url.to_owned(),
            used: std::time::Instant::now(),
        });
    }

    pub(crate) fn window_handle(&self) -> gpui_kit::AnyWindowHandle {
        self.window
    }
}
