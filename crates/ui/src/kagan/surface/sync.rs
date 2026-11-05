use gpui_kit::{Context, Window};
use kg_domain::SurfacePlacement;
use kg_plugins::PluginHost;

use super::mount::{holds_focus, mount};
use super::{find_declared, panels};
use crate::kagan::Kagan;
use crate::kagan::screen::Center;

impl Kagan {
    pub(in crate::kagan) fn in_settings(&self) -> bool {
        matches!(self.screen.current(), Center::Settings)
    }

    pub(in crate::kagan) fn flip_slot_details(&mut self, id: &str) {
        if let Some(slot) = self.tool.as_mut().filter(|slot| slot.id == id) {
            slot.details = !slot.details;
        }
        if let Some(slot) = self.screen.page_mut().filter(|slot| slot.id == id) {
            slot.details = !slot.details;
        }
        if let Some(slot) = self.right_slots.iter_mut().find(|slot| slot.id == id) {
            slot.details = !slot.details;
        }
    }

    pub(in crate::kagan) fn sync_slots(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.right_panels = panels(cx)
            .into_iter()
            .filter(|item| item.placement == SurfacePlacement::Right)
            .collect();
        let tool_held = holds_focus(self.tool.as_ref(), window, cx);
        let page_held = holds_focus(self.screen.page(), window, cx);
        let right_held = self
            .right_slots
            .iter()
            .any(|slot| slot.focus.contains_focused(window, cx));
        let close_held = self.surface_close_focus.is_focused(window);
        let tool_changed = self.sync_one(true, window, cx);
        let page_changed = self.sync_one(false, window, cx);
        let right_changed = self.sync_right_slots(window, cx);
        if tool_changed || page_changed || right_changed {
            self.supersede_surface();
        }
        self.restore_surface_focus(
            tool_held && tool_changed,
            page_held && page_changed,
            close_held && page_changed,
            window,
            cx,
        );
        if right_held && right_changed {
            self.focus_right_tab(window, cx);
        }
    }

    fn sync_one(&mut self, tool: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(slot) = (if tool {
            self.tool.as_ref()
        } else {
            self.screen.page()
        }) else {
            return false;
        };
        let declared = find_declared(&slot.plugin, &slot.contribution, cx);
        let generation =
            PluginHost::try_global(cx).and_then(|host| host.read(cx).generation(&slot.plugin));
        if declared.is_some()
            && (generation == Some(slot.generation)
                || (generation.is_none() && slot.error.is_some()))
        {
            return false;
        }
        let next = declared.map(|declared| mount(&declared, slot.details, window, cx));
        if tool {
            self.tool = next;
        } else if let Some(next) = next {
            if let Some(page) = self.screen.page_mut() {
                *page = next;
            }
        } else {
            self.screen.close();
            self.read_shown(cx);
        }
        true
    }

    fn sync_right_slots(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        let mut next = Vec::with_capacity(self.right_slots.len());
        for slot in std::mem::take(&mut self.right_slots) {
            let declared = find_declared(&slot.plugin, &slot.contribution, cx);
            let generation =
                PluginHost::try_global(cx).and_then(|host| host.read(cx).generation(&slot.plugin));
            if declared
                .as_ref()
                .is_some_and(|declared| declared.placement == SurfacePlacement::Right)
                && (generation == Some(slot.generation)
                    || (generation.is_none() && slot.error.is_some()))
            {
                next.push(slot);
                continue;
            }
            changed = true;
            if let Some(declared) =
                declared.filter(|item| item.placement == SurfacePlacement::Right)
            {
                next.push(mount(&declared, slot.details, window, cx));
            }
        }
        self.right_slots = next;
        if changed
            && self
                .right_tab
                .as_ref()
                .is_some_and(|id| !super::right::tab_available(self, id, cx))
        {
            self.right_tab = self.default_right_tab(cx);
        }
        changed
    }
}
