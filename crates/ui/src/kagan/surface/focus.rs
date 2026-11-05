use gpui_kit::{App, Context, Focusable as _, Window};
use kg_domain::SurfacePlacement;

use super::{Declared, destinations, row_id};
use crate::kagan::Kagan;
use crate::kagan::Shown;

impl Kagan {
    pub(in crate::kagan) fn focus_right_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = super::right::selected_tab(self, cx) else {
            return window.focus(&self.right_add_focus, cx);
        };
        if tab == crate::kagan::tabs::BROWSER
            && cfg!(any(target_os = "macos", target_os = "windows"))
        {
            return self.browser.focus(window, cx);
        }
        if tab == crate::kagan::preview::TAB
            && let Some(preview) = self.tabs.preview.as_ref()
        {
            return window.focus(&preview.focus_handle(), cx);
        }
        let terminal =
            crate::kagan::tabs::terminal_number(&tab).and_then(|number| self.tabs.view(number));
        let focus = match terminal {
            Some(view) => view.focus_handle(cx),
            None => self
                .right_slots
                .iter()
                .find(|slot| slot.id == tab)
                .map_or_else(|| self.right_add_focus.clone(), |slot| slot.focus.clone()),
        };
        window.focus(&focus, cx);
    }

    pub(in crate::kagan) fn closed_right_panels(&self) -> impl Iterator<Item = &Declared> {
        self.right_panels.iter().filter(|item| {
            !self
                .right_slots
                .iter()
                .any(|slot| slot.plugin == item.plugin && slot.contribution == item.id)
        })
    }

    pub(in crate::kagan) fn default_right_tab(&self, cx: &App) -> Option<String> {
        if self.waiting_count(cx) > 0 {
            return Some("needs-you".into());
        }
        if self.thread_visible(cx) {
            return Some("changes".into());
        }
        self.right_slots.first().map(|slot| slot.id.clone())
    }

    pub(in crate::kagan) fn select_right_tab(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.right_tab.as_deref() == Some(id) {
            return;
        }
        self.right_tab = Some(id.to_owned());
        self.supersede_surface();
        cx.notify();
    }

    pub(in crate::kagan) fn show_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen.review().is_none() && self.room_panes(self.layout.left_open, true).right {
            self.force_sidebar(false, true, window, cx);
        } else {
            self.open_overlay(
                crate::kagan::screen::Side::Right,
                crate::kagan::screen::Activation::Focus,
                window,
                cx,
            );
        }
    }

    pub(in crate::kagan) fn focus_work(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(popup) = self.ask_popup.clone()
            && self.terminal_thread(cx).is_some()
        {
            window.focus(&popup, cx);
            return;
        }
        match self.screen.shown() {
            Shown::Thread(view) => window.focus(&view.focus_handle(cx), cx),
            Shown::Nothing | Shown::Start(_) => window.focus(&self.focus, cx),
        }
    }

    pub(super) fn restore_surface_focus(
        &mut self,
        tool_held: bool,
        page_held: bool,
        close_held: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if page_held && self.screen.page().is_some() {
            window.focus(&self.surface_close_focus, cx);
            return;
        }
        if page_held || (close_held && self.screen.page().is_none()) {
            self.focus_work(window, cx);
            return;
        }
        if tool_held {
            self.focus_tool_or_work(window, cx);
        }
    }

    pub(super) fn focus_tool_or_work(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A narrow overlay is drawn over the center, so its keys (Escape first) must reach it.
        if self.screen.drawn(crate::kagan::screen::Side::Left)
            && !self.panes.left
            && !self.in_settings()
        {
            window.focus(&self.left_overlay_focus, cx);
            return;
        }
        let closed = self
            .sidebar
            .update(cx, |sidebar, _| sidebar.take_return_tool());
        let sidebar = self.sidebar.read(cx);
        let row = self
            .tool
            .as_ref()
            .map(|slot| row_id(&slot.plugin, &slot.contribution))
            .filter(|id| sidebar.row_focus(id).is_some())
            .or(closed.filter(|id| sidebar.row_focus(id).is_some()))
            .or_else(|| {
                destinations(cx).into_iter().find_map(|item| {
                    (item.placement == SurfacePlacement::Left)
                        .then(|| row_id(&item.plugin, &item.id))
                })
            })
            .filter(|id| sidebar.row_focus(id).is_some());
        let handle = row.and_then(|id| sidebar.row_focus(&id).cloned());
        if self.panes.left
            && !self.in_settings()
            && let Some(handle) = handle
        {
            window.focus(&handle, cx);
        } else if self.in_settings() {
            window.focus(&self.focus, cx);
        } else {
            self.focus_work(window, cx);
        }
    }
}
