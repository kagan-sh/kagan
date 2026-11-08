use gpui_kit::{Context, Focusable as _, Window};
use kg_domain::SurfacePlacement;

use super::{Declared, panels, row_id};
use crate::kagan::Kagan;
use crate::kagan::Shown;

impl Kagan {
    pub(in crate::kagan) fn closed_right_panels(&self) -> impl Iterator<Item = &Declared> {
        self.right
            .panels
            .iter()
            .filter(|item| !self.right.slots.iter().any(|slot| slot.is(item)))
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
                panels(cx).into_iter().find_map(|item| {
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
