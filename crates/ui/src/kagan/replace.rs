#[cfg(test)]
pub(crate) mod read;

use std::collections::BTreeMap;

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, InteractiveElement as _, IntoElement,
    ParentElement as _, Styled as _, Window, WindowId, actions, div,
};
use gpui_shell::ScriptView;
use kg_core::Workspace;
use kg_domain::{DiffNote, DiffScope, Op, PluginId, PluginOp, Slot, SlotChoice};
use kg_plugins::{HostUi, PluginHost, ReviewComment};
use serde_json::json;

use super::{Kagan, surface};
use crate::ui_observe::UiObserveExt as _;

actions!(kagan, [UseBuiltInSidebar, UseBuiltInReview]);

const MAX_COMMENT: usize = 4096;

// Components that draw in a later pass, above the window, so the frame cannot clip them. A
// replacement could use one to draw over the host's question cards.
const FLOATING: &[&str] = &[
    "Popover",
    "HoverCard",
    "Tooltip",
    "DropdownMenu",
    "DropdownButton",
    "Menu",
    "MenuBar",
    "Select",
    "Combobox",
    "Popup",
    "DatePicker",
    "Dialog",
    "AlertDialog",
    "Sheet",
    "Notification",
    "NativeMenuTrigger",
];

// Which plugin fills each slot. Only the person's choice fills one; any failure empties it and
// saves the built-in, so a later install, update, enable or reload never brings the plugin back.
#[derive(Default)]
pub(super) struct Replaced {
    chosen: BTreeMap<Slot, PluginId>,
    mounted: BTreeMap<Slot, surface::Slot>,
    // A choice made here wins over a saved one still being read.
    epoch: u64,
    delivered: Option<(PluginId, serde_json::Value)>,
}

impl Replaced {
    pub(super) fn chosen(&self, slot: Slot) -> Option<&PluginId> {
        self.chosen.get(&slot)
    }

    pub(super) fn mounted_view(&self, slot: Slot) -> Option<&gpui_kit::Entity<ScriptView>> {
        self.mounted.get(&slot).and_then(|slot| slot.view.as_ref())
    }

    // The window drops a plugin's props when the thread leaves the screen, so a review opened
    // again sends them again.
    pub(super) fn forget_review(&mut self) {
        self.delivered = None;
    }
}

fn frame_id(slot: Slot) -> String {
    format!("replaced-{}", slot.id())
}

impl Kagan {
    pub(super) fn load_slots(&mut self, cx: &mut Context<Self>) {
        // Once the plugins have started, a choice matters only while some plugin offers a slot.
        let offered = kg_plugins::contributions(cx).is_ok_and(|list| {
            list.iter()
                .any(|(_, ui)| Slot::ALL.iter().any(|slot| ui.replacement(*slot).is_some()))
        });
        if !offered && PluginHost::try_global(cx).is_some_and(|host| host.read(cx).started()) {
            return;
        }
        let epoch = self.replaced.epoch;
        let reads = Slot::ALL.map(|slot| {
            (
                slot,
                super::layout::read::<SlotChoice>(&slot.preference(), cx),
            )
        });
        let handle = self.window;
        cx.spawn(async move |this, cx| {
            let mut saved = BTreeMap::new();
            for (slot, read) in reads {
                if let Some(SlotChoice::Plugin(id)) = read.await {
                    saved.insert(slot, id);
                }
            }
            _ = cx.update_window(handle, |_, window, cx| {
                _ = this.update(cx, |this, cx| {
                    if this.replaced.epoch == epoch && this.replaced.chosen != saved {
                        this.replaced.chosen = saved;
                        this.sync_replaced(window, cx);
                        cx.notify();
                    }
                });
            });
        })
        .detach();
    }

    pub(super) fn sync_replaced(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !PluginHost::try_global(cx).is_some_and(|host| host.read(cx).started()) {
            return;
        }
        for slot in Slot::ALL {
            let Some(plugin) = self.replaced.chosen.get(&slot).cloned() else {
                self.replaced.mounted.remove(&slot);
                continue;
            };
            if let Err(reason) = self.fill(slot, &plugin, window, cx) {
                self.fall_back(slot, &reason, window, cx);
            }
        }
    }

    fn fill(
        &mut self,
        slot: Slot,
        plugin: &PluginId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), Lost> {
        let panel = kg_plugins::contributions(cx)
            .ok()
            .and_then(|list| list.into_iter().find(|(id, _)| id == plugin))
            .and_then(|(_, ui)| ui.replacement(slot).cloned())
            .ok_or(Lost::Unoffered)?;
        let host = PluginHost::global(cx);
        if let Some(error) = host
            .read(cx)
            .status()
            .into_iter()
            .find(|status| &status.id == plugin)
            .and_then(|status| status.error.clone())
        {
            return Err(Lost::Stopped(error));
        }
        let generation = host.read(cx).generation(plugin);
        if let Some(mounted) = self.replaced.mounted.get(&slot)
            && mounted.contribution == panel.id
            && generation == Some(mounted.generation)
        {
            return match &mounted.view {
                Some(view) => built(view.read(cx)),
                None => Ok(()),
            };
        }
        let Some(declared) = surface::Declared::from(plugin.clone(), &panel) else {
            return Err(Lost::Unoffered);
        };
        let mounted = surface::mount(&declared, false, window, cx);
        if let Some(error) = &mounted.error {
            return Err(Lost::Unloaded(first_line(error).to_owned()));
        }
        self.replaced.mounted.insert(slot, mounted);
        Ok(())
    }

    fn fall_back(
        &mut self,
        slot: Slot,
        reason: &Lost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(plugin) = self.replaced.chosen.remove(&slot) else {
            return;
        };
        self.empty(slot, window, cx);
        let line = format!("The {} is back to built-in: {reason}", slot.name());
        if let Some(host) = PluginHost::try_global(cx)
            && host
                .read(cx)
                .status()
                .iter()
                .any(|status| *&status.id == plugin)
        {
            host.read(cx).report(&plugin, &line);
        }
        self.note = Some(format!("{line} ({plugin})").into());
    }

    // Drops the plugin's view, saves the built-in, and keeps the keyboard somewhere it can type.
    fn empty(&mut self, slot: Slot, window: &mut Window, cx: &mut Context<Self>) {
        let held = self
            .replaced
            .mounted
            .remove(&slot)
            .is_some_and(|mounted| mounted.focus.contains_focused(window, cx));
        self.replaced.epoch += 1;
        let op = Op::Plugin(PluginOp::Use {
            slot,
            id: SlotChoice::Kagan,
            thread_id: None,
        });
        Workspace::dispatch(op, cx).detach();
        if held {
            match self.screen.review() {
                Some(_) if slot == Slot::Review => self.focus_review(window, cx),
                _ => self.focus_work(window, cx),
            }
        }
        cx.notify();
    }

    pub(super) fn use_built_in_sidebar(
        &mut self,
        _: &UseBuiltInSidebar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replaced.chosen.remove(&Slot::Threads);
        self.empty(Slot::Threads, window, cx);
    }

    pub(super) fn use_built_in_review(
        &mut self,
        _: &UseBuiltInReview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replaced.chosen.remove(&Slot::Review);
        self.empty(Slot::Review, window, cx);
    }

    /// The plugin's body for `slot`, clipped to its frame. After a rebuild, a deferred check puts
    /// the built-in back when the view threw or drew a floating component.
    pub(super) fn replaced_body(
        &self,
        slot: Slot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mounted = self.replaced.mounted.get(&slot)?;
        let view = mounted.view.clone()?;
        let state = view.read(cx);
        if state.is_dirty() || state.snapshot().is_none() || built(state).is_err() {
            let kagan = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                _ = kagan.update(cx, |this, cx| this.check_built(slot, window, cx));
            });
        }
        Some(
            div()
                .id(ElementId::Name(frame_id(slot).into()))
                .ui_container()
                .flex_1()
                .size_full()
                .min_h_0()
                .min_w_0()
                .overflow_hidden()
                .track_focus(&mounted.focus)
                .child(view)
                .into_any_element(),
        )
    }

    fn check_built(&mut self, slot: Slot, window: &mut Window, cx: &mut Context<Self>) {
        let built = self
            .replaced
            .mounted
            .get(&slot)
            .and_then(|mounted| mounted.view.as_ref())
            .map(|view| built(view.read(cx)));
        if let Some(Err(reason)) = built {
            self.fall_back(slot, &reason, window, cx);
        }
    }

    /// Tells the review plugin which thread, scope and file the review on screen shows.
    pub(super) fn deliver_review(&mut self, cx: &mut App) {
        let (Some(plugin), Some(review)) = (
            self.replaced.chosen.get(&Slot::Review),
            self.screen.review(),
        ) else {
            return;
        };
        let props = json!({
            "threadId": review.thread,
            "scope": review.scope,
            "path": review.path,
        });
        let next = Some((plugin.clone(), props.clone()));
        if self.replaced.delivered == next {
            return;
        }
        self.replaced.delivered = next;
        if let Some(host) = HostUi::global(cx) {
            let (plugin, window) = (plugin.clone(), self.window.window_id());
            host.update(cx, |host, _| host.deliver(window, &plugin, props));
        }
    }

    /// A sidebar plugin reads the list again when the threads change.
    pub(super) fn wake_threads_plugin(&self, cx: &mut App) {
        if let (Some(plugin), Some(host)) =
            (self.replaced.chosen.get(&Slot::Threads), HostUi::global(cx))
        {
            let plugin = plugin.clone();
            host.update(cx, |host, _| host.wake(&plugin));
        }
    }

    /// A note from the plugin that draws the review in the front window, the one whose context
    /// the plugin reads. Other windows leave the request alone, so it is answered once. The note
    /// joins the host's own, so the host's footer counts, discards and sends it.
    pub(super) fn plugin_comment(
        &mut self,
        plugin: &PluginId,
        comment: &ReviewComment,
        window: WindowId,
        cx: &mut Context<Self>,
    ) -> Option<Result<(), String>> {
        if !HostUi::global(cx).is_some_and(|host| host.read(cx).targets(window)) {
            return None;
        }
        Some(self.save_comment(plugin, comment, cx))
    }

    fn save_comment(
        &mut self,
        plugin: &PluginId,
        comment: &ReviewComment,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let review = self
            .screen
            .review()
            .filter(|_| {
                self.replaced.chosen.get(&Slot::Review) == Some(plugin)
                    && self.replaced.mounted.contains_key(&Slot::Review)
            })
            .ok_or_else(|| format!("{plugin} does not draw the review on screen"))?;
        if comment.text.len() > MAX_COMMENT {
            return Err(format!("a comment holds at most {MAX_COMMENT} bytes"));
        }
        if !self
            .changes
            .files
            .iter()
            .any(|file| file.path == comment.path)
        {
            return Err(format!(
                "{} is not a changed file in this review",
                comment.path
            ));
        }
        let note = DiffNote {
            scope: review.scope,
            rev: (review.scope == DiffScope::Commit).then(|| "HEAD".to_owned()),
            path: comment.path.clone(),
            side: comment.side,
            start: comment.start,
            end: comment.end,
            text: comment.text.trim().to_owned(),
        };
        let thread = review.thread.clone();
        self.review_drafts.save(&thread, note);
        cx.notify();
        Ok(())
    }
}

// Why a slot went back to the built-in. The text goes to the plugin log and the window.
enum Lost {
    Unoffered,
    Stopped(String),
    Unloaded(String),
    Threw(String),
    Floats(&'static str),
}

impl std::fmt::Display for Lost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unoffered => write!(f, "the plugin is off, removed, or no longer offers it"),
            Self::Stopped(error) => write!(f, "the plugin stopped: {error}"),
            Self::Unloaded(error) => write!(f, "its view did not load: {error}"),
            Self::Threw(error) => write!(f, "its view threw: {error}"),
            Self::Floats(name) => write!(
                f,
                "its view uses {name}, which draws outside its frame; a replacement may not"
            ),
        }
    }
}

// A view that threw, or drew a floating component, cannot stand in for the built-in.
fn built(view: &ScriptView) -> Result<(), Lost> {
    if let Some(error) = view.build_error() {
        return Err(Lost::Threw(first_line(error).to_owned()));
    }
    let tree = view.snapshot().map(gpui_shell::RenderSnapshot::debug_tree);
    match tree.and_then(|tree| floating(&tree)) {
        Some(name) => Err(Lost::Floats(name)),
        None => Ok(()),
    }
}

fn floating(tree: &str) -> Option<&'static str> {
    tree.lines().find_map(|line| {
        let name = line.split_whitespace().next()?;
        if name != "text" && line.contains(" :tooltip") {
            return Some("Tooltip");
        }
        FLOATING.iter().copied().find(|floating| *floating == name)
    })
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}
