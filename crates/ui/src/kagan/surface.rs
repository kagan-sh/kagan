use gpui_kit::{App, Context, Entity, FocusHandle, KeyBinding, Window, actions};
use gpui_shell::ScriptView;
use kg_domain::{OpError, OpResult, PluginId, SurfacePlacement};
use kg_plugins::{Placement, PluginHost};
use serde_json::Value;

use super::Kagan;
use super::right::{self};

actions!(surface, [ClosePage]);

#[derive(Clone)]
pub(super) struct OpenSurface {
    pub plugin: PluginId,
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
    _snapshot_watch: Option<std::rc::Rc<gpui_kit::Subscription>>,
}

struct Hold {
    tool: Option<Slot>,
    right_slots: Vec<Slot>,
    right_tab: Option<right::Tab>,
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
}

pub(crate) fn bindings() -> Vec<KeyBinding> {
    let mut keys = Vec::new();
    for key in ["enter", "space"] {
        keys.push(KeyBinding::new(key, ClosePage, Some("SurfaceClose")));
    }
    keys.push(KeyBinding::new("escape", ClosePage, Some("Page")));
    keys
}

pub(super) struct Declared {
    pub(super) plugin: PluginId,
    pub(super) id: String,
    pub(super) title: String,
    placement: SurfacePlacement,
}

impl Declared {
    pub(super) fn from(plugin: PluginId, panel: &kg_plugins::PanelContribution) -> Option<Self> {
        Some(Self {
            plugin,
            id: panel.id.clone(),
            title: panel.title.clone(),
            placement: drawn(panel.placement)?,
        })
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
    panels(cx)
        .into_iter()
        .filter_map(|item| {
            let placement = item.placement;
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
            ui.panels
                .iter()
                .filter(|panel| panel.replaces.is_none())
                .filter_map(|panel| Declared::from(plugin.clone(), panel))
                .collect::<Vec<_>>()
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

pub(super) fn body_id(id: &str) -> String {
    format!("{id}-body")
}

mod commit;
mod focus;
mod mount;
mod ops;
mod sync;

pub(super) use mount::{mount, mount_error, page, tool_region};
