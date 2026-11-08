use gpui_kit::{Context, Window};
use kg_domain::{OpError, OpResult, SurfaceOp, SurfacePlacement};
use serde_json::Value;

use super::mount::{expect_of, matching_slot, painted};
use super::{
    Check, Commit, Declared, Expect, Focus, Hold, find_declared, mount, reuse, row_id,
    schedule_settle,
};
use crate::kagan::Kagan;
use crate::kagan::screen::Center;

impl Kagan {
    pub(crate) fn commit(
        &mut self,
        mut op: SurfaceOp,
        props: Option<Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Commit, OpError> {
        if matches!(op, SurfaceOp::List) {
            return Ok(Commit::Ready(Box::new(OpResult::Surfaces(
                self.surface_list(window, cx),
            ))));
        }
        if self.surface_check.is_some() && matches!(op, SurfaceOp::Threads) {
            self.supersede_surface();
            self.tool = None;
            self.show_left(window, cx);
            cx.notify();
            return Ok(Commit::Ready(Box::new(OpResult::Done)));
        }
        if self.surface_check.is_some() {
            return Err(OpError::Failed(
                "a surface change is already waiting for its frame".into(),
            ));
        }
        if matches!(op, SurfaceOp::Threads) {
            if self.in_settings() {
                let work = self.screen.shown().clone();
                self.open_center(Center::Work(work), window, cx);
            }
            self.show_left(window, cx);
            let Some(slot) = self.tool.as_ref() else {
                cx.notify();
                return Ok(Commit::Ready(Box::new(OpResult::Done)));
            };
            op = SurfaceOp::Close {
                plugin: slot.plugin.clone(),
                id: slot.contribution.clone(),
            };
        }
        self.viewport = window.viewport_size().width;
        let hold = Hold {
            tool: self.tool.clone(),
            slots: self.right.slots.clone(),
            tab: self.right.tab.clone(),
            layout: self.layout,
        };
        self.screen.begin();
        if let SurfaceOp::Open { plugin, id } = &op
            && find_declared(plugin, id, cx).is_some_and(|item| {
                item.placement == SurfacePlacement::Center && !self.is_open(&item)
            })
        {
            self.screen.remember_center(window, cx);
        }
        self.surface_hold = true;
        let expect = match self.apply(op, props, window, cx) {
            Ok(expect) => expect,
            Err(error) => {
                self.screen.rollback();
                self.surface_hold = false;
                return Err(error);
            }
        };
        self.surface_check = Some(Check {
            hold,
            expect,
            epoch: self.surface_epoch,
        });
        Ok(Commit::Paint)
    }

    pub(crate) fn settle(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<OpResult, OpError> {
        let Some(Check {
            hold,
            expect,
            epoch,
        }) = self.surface_check.take()
        else {
            return Err(OpError::Failed("no surface is waiting for a frame".into()));
        };
        if epoch != self.surface_epoch {
            self.screen.commit();
            self.surface_hold = false;
            return Err(OpError::Failed(
                "a newer change replaced this surface request before its frame".into(),
            ));
        }
        if painted(window, &expect.id) == expect.present {
            self.screen.commit();
            self.save_open_change(hold.layout, cx);
            self.surface_hold = false;
            match expect.focus {
                Focus::Page => window.focus(&self.surface_close_focus, cx),
                Focus::Tool => self.focus_tool_or_work(window, cx),
                Focus::Right => self.focus_right_tab(window, cx),
                Focus::Work => {
                    self.return_focus(crate::kagan::screen::FocusReturn::Center, window, cx);
                }
            }
            if let Some(plugin) = &expect.plugin
                && let Some(props) = expect.props.clone()
                && let Some(host) = kg_plugins::HostUi::global(cx)
            {
                let window = window.window_handle().window_id();
                host.update(cx, |host, _| host.deliver(window, plugin, props));
            }
            self.tell_host(window, cx);
            Ok(OpResult::Done)
        } else {
            let id = expect.id;
            self.sidebar
                .update(cx, |sidebar, _| sidebar.set_return_tool(None));
            self.screen.rollback();
            self.tool = hold.tool;
            self.right.slots = hold.slots;
            self.right.tab = hold.tab;
            if self.layout.left_open != hold.layout.left_open {
                self.force_sidebar(true, hold.layout.left_open, window, cx);
            }
            if self.layout.right_open != hold.layout.right_open {
                self.force_sidebar(false, hold.layout.right_open, window, cx);
            }
            self.sync_panels_keeping_widths(window, cx);
            cx.notify();
            self.surface_hold = false;
            Err(OpError::Failed(format!("{id} is not on screen")))
        }
    }

    pub(super) fn after_commit(
        &mut self,
        step: Result<Commit, OpError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match step {
            Ok(Commit::Ready(_)) => {}
            Ok(Commit::Paint) => {
                let kagan = cx.entity();
                schedule_settle(kagan, window, cx, |_| {});
            }
            Err(error) => {
                self.note = Some(error.to_string().into());
                cx.notify();
            }
        }
    }

    fn apply(
        &mut self,
        op: SurfaceOp,
        props: Option<Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Expect, OpError> {
        let (plugin, id, open) = match op {
            SurfaceOp::List | SurfaceOp::Threads => unreachable!(),
            SurfaceOp::Open { plugin, id } => (plugin, id, true),
            SurfaceOp::Close { plugin, id } => (plugin, id, false),
        };
        let declared = find_declared(&plugin, &id, cx).ok_or_else(|| {
            OpError::NotFound(format!("{plugin} has no panel {id} in this window"))
        })?;
        if !open {
            if !self.is_open(&declared) {
                return Err(OpError::Invalid(format!("{id} is not open")));
            }
            self.drop_page(&declared, cx);
            match declared.placement {
                SurfacePlacement::Right => {
                    let surface = declared.surface_id();
                    self.right.slots.retain(|slot| {
                        !(slot.plugin == declared.plugin && slot.contribution == declared.id)
                    });
                    if self.right.tab.as_ref() == Some(&super::right::Tab::Plugin(surface)) {
                        self.right.tab = self.default_right_tab(cx);
                    }
                }
                SurfacePlacement::Left => {
                    let row = row_id(&declared.plugin, &declared.id);
                    self.sidebar
                        .update(cx, |sidebar, _| sidebar.set_return_tool(Some(row)));
                    self.tool = None;
                }
                SurfacePlacement::Center => {
                    self.screen.close();
                    self.read_shown(cx);
                }
            }
            cx.notify();
            return Ok(expect_of(&declared, false, None, None));
        }
        if matches!(
            declared.placement,
            SurfacePlacement::Left | SurfacePlacement::Center
        ) {
            self.right.fill = super::right::Fill::Column;
        }
        if declared.placement == SurfacePlacement::Left {
            if self.in_settings() {
                let work = self.screen.shown().clone();
                self.open_center(Center::Work(work), window, cx);
            }
            self.show_left(window, cx);
        }
        if declared.placement == SurfacePlacement::Right {
            self.show_right(window, cx);
        }
        self.sync_panels_keeping_widths(window, cx);
        match declared.placement {
            SurfacePlacement::Center => {
                if !self
                    .screen
                    .page()
                    .is_some_and(|slot| reuse(slot, &declared, cx))
                {
                    let page = mount(&declared, false, window, cx);
                    self.open_center(Center::Page(Box::new(page)), window, cx);
                }
            }
            SurfacePlacement::Left => {
                if !self
                    .tool
                    .as_ref()
                    .is_some_and(|slot| reuse(slot, &declared, cx))
                {
                    self.tool = Some(mount(&declared, false, window, cx));
                }
            }
            SurfacePlacement::Right => {
                if let Some(ix) = self.right.slots.iter().position(|slot| {
                    slot.plugin == declared.plugin && slot.contribution == declared.id
                }) {
                    if !reuse(&self.right.slots[ix], &declared, cx) {
                        let details = self.right.slots[ix].details;
                        self.right.slots[ix] = mount(&declared, details, window, cx);
                    }
                    self.right.tab =
                        Some(super::right::Tab::Plugin(self.right.slots[ix].id.clone()));
                } else {
                    let slot = mount(&declared, false, window, cx);
                    self.right.tab = Some(super::right::Tab::Plugin(slot.id.clone()));
                    self.right.slots.push(slot);
                }
            }
        }
        Ok(expect_of(&declared, true, Some(plugin), props))
    }

    fn drop_page(&self, declared: &Declared, cx: &mut Context<Self>) {
        let id = match declared.placement {
            SurfacePlacement::Right => Some(declared.surface_id()),
            SurfacePlacement::Left => self.tool.as_ref().map(|slot| slot.id.clone()),
            SurfacePlacement::Center => self.screen.page().map(|slot| slot.id.clone()),
        };
        if let Some(id) = id {
            crate::kagan::web_panel::forget(&id, cx);
        }
    }

    fn is_open(&self, declared: &Declared) -> bool {
        matching_slot(
            self.tool.as_ref(),
            self.screen.page(),
            &self.right.slots,
            declared,
        )
        .is_some()
    }
}
