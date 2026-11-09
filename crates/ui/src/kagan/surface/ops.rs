use gpui_kit::{App, Context, Window};
use kg_domain::{SurfaceItem, SurfaceOp};

use super::mount::{matching_slot, painted};
use super::{ClosePage, body_id, panels};
use crate::kagan::Kagan;

impl Kagan {
    pub(in crate::kagan) fn supersede_surface(&mut self) {
        if self.slots.surface_check.is_some() {
            self.screen.commit();
            self.slots.surface_epoch = self.slots.surface_epoch.wrapping_add(1);
            self.slots.surface_hold = false;
        }
    }

    pub(super) fn surface_list(&self, window: &Window, cx: &App) -> Vec<SurfaceItem> {
        panels(cx)
            .into_iter()
            .map(|declared| {
                let placement = declared.placement;
                let current = matching_slot(
                    self.slots.tool.as_ref(),
                    self.screen.page(),
                    &self.right.slots,
                    &declared,
                );
                let committed = self.slots.surface_check.as_ref().map_or(current, |check| {
                    matching_slot(
                        check.hold.tool.as_ref(),
                        self.screen.committed_page(),
                        &check.hold.slots,
                        &declared,
                    )
                });
                let open = committed.is_some();
                let visible = painted(window, &body_id(&declared.surface_id()))
                    && committed.is_some_and(|slot| {
                        current.is_none_or(|current| current.generation == slot.generation)
                    });
                SurfaceItem {
                    plugin: declared.plugin,
                    id: declared.id,
                    title: declared.title,
                    placement,
                    open,
                    visible,
                }
            })
            .collect()
    }

    pub(in crate::kagan) fn run_surface(
        &mut self,
        op: SurfaceOp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let step = self.commit(op, None, window, cx);
        self.after_commit(step, window, cx);
    }

    pub(in crate::kagan) fn close_page(
        &mut self,
        _: &ClosePage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(slot) = self.screen.page().cloned() else {
            return;
        };
        let op = SurfaceOp::Close {
            plugin: slot.plugin,
            id: slot.contribution,
        };
        self.run_surface(op, window, cx);
    }
}
