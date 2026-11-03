use crate::theme::TypeScale as _;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    Action, AnyElement, App, Context, ElementId, Entity, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, actions, div,
};
use gpui_shell::ScriptView;
use kg_domain::{OpError, OpResult, PluginId, SurfaceItem, SurfaceOp, SurfacePlacement};
use kg_plugins::{MountError, Placement, PluginHost};
use serde_json::Value;

use super::right::{self, plugin_body};
use super::{Kagan, Shown, screen::Center};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::{Facts, Located, locate};

actions!(surface, [ClosePage]);

#[derive(Clone, PartialEq, Eq, Action)]
#[action(namespace = surface, no_json)]
pub(super) struct OpenSurface {
    pub plugin: String,
    pub id: String,
}

#[derive(Clone)]
pub(super) struct Slot {
    pub(super) id: String,
    pub(super) plugin: PluginId,
    pub(super) contribution: String,
    pub(super) title: String,
    pub(super) details: bool,
    pub(super) view: Option<Entity<ScriptView>>,
    pub(super) web: Option<String>,
    pub(super) error: Option<String>,
    pub(super) generation: u64,
    pub(super) focus: FocusHandle,
}

struct Hold {
    tool: Option<Slot>,
    right_slots: Vec<Slot>,
    right_tab: Option<String>,
    layout: super::layout::Layout,
}

pub(super) struct Check {
    hold: Hold,
    expect: Expect,
    epoch: u64,
}

pub(crate) enum Commit {
    Ready(Box<OpResult>),
    Paint,
}

pub(crate) fn paint_then_settle(
    kagan: &Entity<Kagan>,
    window: &mut Window,
    cx: &mut App,
) -> Result<OpResult, OpError> {
    crate::ui_dispatch::render_frame(window, cx);
    let settled = kagan.update(cx, |kagan, cx| kagan.settle(window, cx));
    if settled.is_err() {
        crate::ui_dispatch::render_frame(window, cx);
    }
    settled
}

pub(crate) fn schedule_settle(
    kagan: Entity<Kagan>,
    window: &mut Window,
    cx: &mut Context<Kagan>,
    done: impl FnOnce(Result<OpResult, OpError>) + 'static,
) {
    // A later effect in this cycle can replace the open, and its notification is queued
    // behind that effect. Paint only after that notification has landed.
    window.defer(cx, move |window, cx| {
        window.defer(cx, move |window, cx| {
            done(paint_then_settle(&kagan, window, cx));
        });
    });
}

struct Expect {
    id: String,
    present: bool,
    plugin: Option<PluginId>,
    props: Option<Value>,
    focus: Focus,
}

enum Focus {
    Page,
    Tool,
    Right,
    Work,
    None,
}

pub(crate) fn bindings() -> Vec<KeyBinding> {
    let mut keys = Vec::new();
    for key in ["enter", "space"] {
        keys.push(KeyBinding::new(key, ClosePage, Some("SurfaceClose")));
    }
    keys.push(KeyBinding::new("escape", ClosePage, Some("Page")));
    keys
}

impl Kagan {
    pub(super) fn in_settings(&self) -> bool {
        matches!(self.screen.current(), Center::Settings)
    }

    pub(super) fn flip_slot_details(&mut self, id: &str) {
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

    pub(super) fn sync_slots(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.right_panels = panels(cx)
            .into_iter()
            .filter(|item| item.placement == Placement::Right)
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
                .is_some_and(|declared| declared.placement == Placement::Right)
                && (generation == Some(slot.generation)
                    || (generation.is_none() && slot.error.is_some()))
            {
                next.push(slot);
                continue;
            }
            changed = true;
            if let Some(declared) = declared.filter(|item| item.placement == Placement::Right) {
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

    // `Commit::Paint` means the caller draws a frame, then calls `settle`.
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
            right_slots: self.right_slots.clone(),
            right_tab: self.right_tab.clone(),
            layout: self.layout,
        };
        self.screen.begin();
        if let SurfaceOp::Open { plugin, id } = &op
            && find_declared(plugin, id, cx)
                .is_some_and(|item| item.placement == Placement::Center && !self.is_open(&item))
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
                Focus::Work => self.return_focus(super::screen::FocusReturn::Center, window, cx),
                Focus::None => {}
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
            self.right_slots = hold.right_slots;
            self.right_tab = hold.right_tab;
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

    fn after_commit(
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
                Placement::Bottom => {}
                Placement::Right => {
                    let surface = declared.surface_id();
                    self.right_slots.retain(|slot| {
                        !(slot.plugin == declared.plugin && slot.contribution == declared.id)
                    });
                    if self.right_tab.as_ref() == Some(&surface) {
                        self.right_tab = self.default_right_tab(cx);
                    }
                }
                Placement::Left => {
                    let row = row_id(&declared.plugin, &declared.id);
                    self.sidebar
                        .update(cx, |sidebar, _| sidebar.set_return_tool(Some(row)));
                    self.tool = None;
                }
                Placement::Center => {
                    self.screen.close();
                    self.read_shown(cx);
                }
            }
            cx.notify();
            return Ok(expect_of(&declared, false, None, None));
        }
        if declared.placement == Placement::Bottom {
            return Err(OpError::Invalid(
                "bottom panels are not in this version of kagan".into(),
            ));
        }
        if matches!(declared.placement, Placement::Left | Placement::Center) {
            self.right_fill = super::right::Fill::Column;
        }
        if declared.placement == Placement::Left {
            if self.in_settings() {
                let work = self.screen.shown().clone();
                self.open_center(Center::Work(work), window, cx);
            }
            self.show_left(window, cx);
        }
        if declared.placement == Placement::Right {
            self.show_right(window, cx);
        }
        self.sync_panels_keeping_widths(window, cx);
        match declared.placement {
            Placement::Center => {
                if !self
                    .screen
                    .page()
                    .is_some_and(|slot| reuse(slot, &declared, cx))
                {
                    let page = mount(&declared, false, window, cx);
                    self.open_center(Center::Page(Box::new(page)), window, cx);
                }
            }
            Placement::Left => {
                if !self
                    .tool
                    .as_ref()
                    .is_some_and(|slot| reuse(slot, &declared, cx))
                {
                    self.tool = Some(mount(&declared, false, window, cx));
                }
            }
            Placement::Right => {
                if let Some(ix) = self.right_slots.iter().position(|slot| {
                    slot.plugin == declared.plugin && slot.contribution == declared.id
                }) {
                    if !reuse(&self.right_slots[ix], &declared, cx) {
                        let details = self.right_slots[ix].details;
                        self.right_slots[ix] = mount(&declared, details, window, cx);
                    }
                    self.right_tab = Some(self.right_slots[ix].id.clone());
                } else {
                    let slot = mount(&declared, false, window, cx);
                    self.right_tab = Some(slot.id.clone());
                    self.right_slots.push(slot);
                }
            }
            Placement::Bottom => {}
        }
        Ok(expect_of(&declared, true, Some(plugin), props))
    }

    fn drop_page(&self, declared: &Declared, cx: &mut Context<Self>) {
        let id = match declared.placement {
            Placement::Right => Some(declared.surface_id()),
            Placement::Left => self.tool.as_ref().map(|slot| slot.id.clone()),
            Placement::Center => self.screen.page().map(|slot| slot.id.clone()),
            Placement::Bottom => None,
        };
        if let Some(id) = id {
            super::web_panel::forget(&id, cx);
        }
    }

    fn is_open(&self, declared: &Declared) -> bool {
        matching_slot(
            self.tool.as_ref(),
            self.screen.page(),
            &self.right_slots,
            declared,
        )
        .is_some()
    }

    pub(super) fn focus_right_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = super::right::selected_tab(self, cx) else {
            return window.focus(&self.right_add_focus, cx);
        };
        if tab == super::tabs::BROWSER && cfg!(target_os = "macos") {
            return self.browser.focus(window, cx);
        }
        let terminal = super::tabs::terminal_number(&tab).and_then(|number| self.tabs.view(number));
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

    pub(super) fn closed_right_panels(&self) -> impl Iterator<Item = &Declared> {
        self.right_panels.iter().filter(|item| {
            !self
                .right_slots
                .iter()
                .any(|slot| slot.plugin == item.plugin && slot.contribution == item.id)
        })
    }

    pub(super) fn default_right_tab(&self, cx: &App) -> Option<String> {
        if self.waiting_count(cx) > 0 {
            return Some("needs-you".into());
        }
        if self.thread_visible(cx) {
            return Some("changes".into());
        }
        self.right_slots.first().map(|slot| slot.id.clone())
    }

    pub(super) fn select_right_tab(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.right_tab.as_deref() == Some(id) {
            return;
        }
        self.right_tab = Some(id.to_owned());
        self.supersede_surface();
        cx.notify();
    }

    pub(super) fn show_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen.review().is_none() && self.room_panes(self.layout.left_open, true).right {
            self.force_sidebar(false, true, window, cx);
        } else {
            self.open_overlay(
                super::screen::Side::Right,
                super::screen::Activation::Focus,
                window,
                cx,
            );
        }
    }

    pub(super) fn focus_work(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.screen.shown() {
            Shown::Thread(view) => window.focus(&view.focus_handle(cx), cx),
            Shown::Nothing | Shown::Start(_) => window.focus(&self.focus, cx),
        }
    }

    fn restore_surface_focus(
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

    fn focus_tool_or_work(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A narrow overlay is drawn over the center, so its keys (Escape first) must reach it.
        if self.screen.drawn(super::screen::Side::Left) && !self.panes.left && !self.in_settings() {
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
                    (item.placement == Placement::Left).then(|| row_id(&item.plugin, &item.id))
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

    pub(super) fn supersede_surface(&mut self) {
        if self.surface_check.is_some() {
            self.screen.commit();
            self.surface_epoch = self.surface_epoch.wrapping_add(1);
            self.surface_hold = false;
        }
    }

    fn surface_list(&self, window: &Window, cx: &App) -> Vec<SurfaceItem> {
        destinations(cx)
            .into_iter()
            .filter_map(|declared| {
                let placement = drawn(declared.placement)?;
                let current = matching_slot(
                    self.tool.as_ref(),
                    self.screen.page(),
                    &self.right_slots,
                    &declared,
                );
                let committed = self.surface_check.as_ref().map_or(current, |check| {
                    matching_slot(
                        check.hold.tool.as_ref(),
                        self.screen.committed_page(),
                        &check.hold.right_slots,
                        &declared,
                    )
                });
                let open = committed.is_some();
                let visible = painted(window, &body_id(&declared.surface_id()))
                    && committed.is_some_and(|slot| {
                        current.is_none_or(|current| current.generation == slot.generation)
                    });
                Some(SurfaceItem {
                    plugin: declared.plugin,
                    id: declared.id,
                    title: declared.title,
                    placement,
                    open,
                    visible,
                })
            })
            .collect()
    }

    pub(super) fn run_surface(
        &mut self,
        op: SurfaceOp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let step = self.commit(op, None, window, cx);
        self.after_commit(step, window, cx);
    }

    pub(super) fn close_page(
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

pub(super) struct Declared {
    pub(super) plugin: PluginId,
    pub(super) id: String,
    pub(super) title: String,
    placement: Placement,
}

impl Declared {
    pub(super) fn from(plugin: PluginId, panel: &kg_plugins::PanelContribution) -> Self {
        Self {
            plugin,
            id: panel.id().to_owned(),
            title: panel.title().to_owned(),
            placement: panel.placement(),
        }
    }

    fn surface_id(&self) -> String {
        right::plugin_panel_id(&self.plugin, &self.id)
    }
}

pub(super) struct NavItem {
    pub plugin: PluginId,
    pub id: String,
    pub title: String,
    pub page: bool,
}

pub(super) fn row_id(plugin: &PluginId, id: &str) -> String {
    format!("surface-item-{plugin}-{id}")
}

pub(super) fn nav_items(cx: &App) -> Vec<NavItem> {
    destinations(cx)
        .into_iter()
        .filter_map(|item| {
            let placement = drawn(item.placement)?;
            if matches!(placement, SurfacePlacement::Right) {
                return None;
            }
            let page = placement == SurfacePlacement::Center;
            Some(NavItem {
                plugin: item.plugin,
                id: item.id,
                title: item.title,
                page,
            })
        })
        .collect()
}

fn panels(cx: &App) -> Vec<Declared> {
    let Ok(list) = kg_plugins::contributions(cx) else {
        return Vec::new();
    };
    list.into_iter()
        .flat_map(|(plugin, ui)| {
            ui.panels()
                .iter()
                .filter(|panel| panel.replaces().is_none())
                .map(|panel| Declared::from(plugin.clone(), panel))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn destinations(cx: &App) -> Vec<Declared> {
    panels(cx)
        .into_iter()
        .filter(|item| {
            matches!(
                item.placement,
                Placement::Left | Placement::Center | Placement::Right
            )
        })
        .collect()
}

fn drawn(placement: Placement) -> Option<SurfacePlacement> {
    match placement {
        Placement::Left => Some(SurfacePlacement::Left),
        Placement::Center => Some(SurfacePlacement::Center),
        Placement::Right => Some(SurfacePlacement::Right),
        Placement::Bottom => None,
    }
}

fn reuse(slot: &Slot, declared: &Declared, cx: &App) -> bool {
    let generation =
        PluginHost::try_global(cx).and_then(|host| host.read(cx).generation(&declared.plugin));
    slot.plugin == declared.plugin
        && slot.contribution == declared.id
        && generation == Some(slot.generation)
}

fn find_declared(plugin: &PluginId, id: &str, cx: &App) -> Option<Declared> {
    panels(cx)
        .into_iter()
        .find(|item| item.plugin == *plugin && item.id == id)
}

pub(super) fn mount(
    declared: &Declared,
    details: bool,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> Slot {
    let host = PluginHost::global(cx);
    let web = host.read(cx).web_file(&declared.plugin, &declared.id);
    if web.is_some() {
        let loaded = host.update(cx, |host, cx| host.load_plugin(&declared.plugin, cx));
        let (web, error) = match loaded {
            Ok(()) => (web, None),
            Err(error) => (None, Some(mount_error(&error))),
        };
        return Slot {
            id: declared.surface_id(),
            plugin: declared.plugin.clone(),
            contribution: declared.id.clone(),
            title: declared.title.clone(),
            details,
            view: None,
            web,
            error,
            generation: host.read(cx).generation(&declared.plugin).unwrap_or(0),
            focus: cx.focus_handle(),
        };
    }
    let mounted = host.update(cx, |host, cx| {
        host.mount_panel(&declared.plugin, &declared.id, window, cx)
    });
    let (view, error) = match mounted {
        Ok(view) => (Some(view), None),
        Err(error) => (None, Some(mount_error(&error))),
    };
    Slot {
        id: declared.surface_id(),
        plugin: declared.plugin.clone(),
        contribution: declared.id.clone(),
        title: declared.title.clone(),
        details,
        view,
        web: None,
        error,
        generation: host.read(cx).generation(&declared.plugin).unwrap_or(0),
        focus: cx.focus_handle(),
    }
}

fn matching_slot<'a>(
    tool: Option<&'a Slot>,
    page: Option<&'a Slot>,
    right: &'a [Slot],
    declared: &Declared,
) -> Option<&'a Slot> {
    let same = |slot: &&Slot| slot.contribution == declared.id && slot.plugin == declared.plugin;
    match declared.placement {
        Placement::Left => tool.filter(same),
        Placement::Center => page.filter(same),
        Placement::Right => right.iter().find(same),
        Placement::Bottom => None,
    }
}

fn focus_for(placement: Placement, open: bool) -> Focus {
    match (placement, open) {
        (Placement::Center, true) => Focus::Page,
        (Placement::Center, false) => Focus::Work,
        (Placement::Left, _) => Focus::Tool,
        (Placement::Right, _) => Focus::Right,
        (Placement::Bottom, _) => Focus::None,
    }
}

fn body_id(id: &str) -> String {
    format!("{id}-body")
}

fn expect_of(
    declared: &Declared,
    open: bool,
    plugin: Option<PluginId>,
    props: Option<Value>,
) -> Expect {
    Expect {
        id: body_id(&declared.surface_id()),
        present: open,
        plugin: if open { plugin } else { None },
        props: if open { props } else { None },
        focus: focus_for(declared.placement, open),
    }
}

fn holds_focus(slot: Option<&Slot>, window: &Window, cx: &App) -> bool {
    slot.is_some_and(|slot| slot.focus.contains_focused(window, cx))
}

pub(super) fn mount_error(error: &MountError) -> String {
    match error {
        MountError::Missing(text) | MountError::Failed(text) => text.clone(),
    }
}

pub(super) fn painted(window: &Window, id: &str) -> bool {
    match locate(window, &ElementId::Name(id.into()), None) {
        Located::One(element) => {
            element.visible()
                && f32::from(element.bounds().size.width) > 0.
                && f32::from(element.bounds().size.height) > 0.
        }
        Located::Missing | Located::Ambiguous(_) => false,
    }
}

pub(super) fn tool_region(
    kagan: &Kagan,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> Option<AnyElement> {
    kagan.tool.as_ref().map(|slot| {
        div()
            .id("surface-tool")
            .child(
                div()
                    .id(ElementId::Name(body_id(&slot.id).into()))
                    .ui_container()
                    .track_focus(&slot.focus)
                    .child(plugin_body(slot, false, window, cx)),
            )
            .into_any_element()
    })
}

fn center_body(slot: &Slot, window: &mut Window, cx: &mut Context<Kagan>) -> AnyElement {
    let id = ElementId::Name(format!("surface-scroll-{}", slot.id).into());
    let body = plugin_body(slot, true, window, cx);
    if slot.web.is_none() {
        div()
            .size_full()
            .p_4()
            .ui_container()
            .overflow_y_scrollbar()
            .id(id)
            .child(body)
            .into_any_element()
    } else {
        div()
            .size_full()
            .overflow_hidden()
            .id(id)
            .child(body)
            .into_any_element()
    }
}

pub(super) fn page(kagan: &Kagan, window: &mut Window, cx: &mut Context<Kagan>) -> AnyElement {
    let Some(slot) = kagan.screen.page() else {
        return div().into_any_element();
    };
    let focus = kagan.surface_close_focus.clone();
    let title = slot.title.clone();
    v_flex()
        .id("surface-page")
        .ui_container()
        .size_full()
        .min_h_0()
        .key_context("Page")
        .child(
            h_flex()
                .px_4()
                .py_2()
                .gap_2()
                .justify_between()
                .items_center()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .id("surface-title")
                        .ui_container()
                        .flex_1()
                        .min_w_0()
                        .type_title()
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .text_ellipsis()
                        .child(title.clone()),
                )
                .child(
                    div()
                        .id("surface-close")
                        .key_context("SurfaceClose")
                        .tab_stop(true)
                        .px_2()
                        .py_1()
                        .rounded(cx.theme().radius)
                        .type_body()
                        .focus(|close| close.border_1().border_color(cx.theme().ring))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_page(&ClosePage, window, cx);
                        }))
                        .child("Close")
                        .ui_observe_as(Facts {
                            focus: Some(focus.clone()),
                            ..Facts::button("Close")
                        }),
                ),
        )
        .child(
            div().flex_1().min_h_0().child(
                div()
                    .id(ElementId::Name(body_id(&slot.id).into()))
                    .size_full()
                    .min_h_0()
                    .ui_container()
                    .track_focus(&slot.focus)
                    .child(center_body(slot, window, cx)),
            ),
        )
        .into_any_element()
}
