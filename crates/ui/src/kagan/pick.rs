use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyElement, App, Context, CursorStyle, ElementId, Entity, EntityId, FocusHandle,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement as _, Pixels, Point, Styled as _, Window, div, px,
};
use kg_domain::{PluginId, UiPick};

use super::Kagan;
use crate::components::{conceal, reveal, shown_pages};
use crate::theme::{SpaceStep, TypeScale as _, space};
use crate::ui_registry::{self, ElementSnapshot};
use crate::web_page::WebPage;

pub(crate) type Answer = Result<Option<UiPick>, String>;

pub(crate) struct Session {
    name: Option<String>,
    reply: async_channel::Sender<Answer>,
    hover: Option<ElementSnapshot>,
    missed: bool,
    focus_before: Option<FocusHandle>,
    hidden: Vec<Entity<WebPage>>,
}

impl Kagan {
    /// Puts this window in pick mode. `name` is the plugin that asked, or `None` for `kg ui pick`.
    pub(crate) fn begin_pick(
        &mut self,
        name: Option<String>,
        reply: async_channel::Sender<Answer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.pick.is_some() {
            return Err("a pick is already on; finish it or press Esc".into());
        }
        let mut hidden = shown_pages(cx);
        if let Some(page) = self.browser.page().filter(|page| page.read(cx).visible(cx)) {
            hidden.push(page);
        }
        for page in &hidden {
            conceal(page, cx);
        }
        self.pick = Some(Session {
            name,
            reply,
            hover: None,
            missed: false,
            focus_before: window.focused(cx),
            hidden,
        });
        window.focus(&self.pick_focus, cx);
        cx.notify();
        Ok(())
    }

    /// Ends pick mode. `None` is a cancel. The reply goes to whoever asked.
    pub(crate) fn end_pick(
        &mut self,
        answer: Option<UiPick>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.pick.take() else {
            return;
        };
        for page in &session.hidden {
            reveal(page, cx);
        }
        if let Some(focus) = &session.focus_before {
            window.focus(focus, cx);
        }
        let _ = session.reply.try_send(Ok(answer)); // A caller that gave up reads no reply.
        cx.notify();
    }

    /// A pick whose plugin unloaded has no one to answer. Its window ends the pick.
    pub(crate) fn drop_orphan_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .pick
            .as_ref()
            .is_some_and(|pick| pick.reply.is_closed())
        {
            self.end_pick(None, window, cx);
        }
    }

    fn pick_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let hover = topmost(window, event.position);
        let Some(session) = self.pick.as_mut() else {
            return;
        };
        let changed = session
            .hover
            .as_ref()
            .map(|element| (&element.path, element.bounds))
            != hover
                .as_ref()
                .map(|element| (&element.path, element.bounds));
        if changed {
            session.hover = hover;
            cx.notify();
        }
    }

    fn pick_click(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.pick.is_none() || event.button != MouseButton::Left {
            return;
        }
        // Only a person's click finishes a pick: a synthetic one is refused, and pick stays on.
        if crate::synthetic::refuse_for_person() {
            return;
        }
        if let Some(element) = topmost(window, event.position) {
            let pick = to_pick(&element, owner(&element, cx));
            self.end_pick(Some(pick), window, cx);
        } else {
            if let Some(session) = self.pick.as_mut() {
                session.missed = true;
            }
            cx.notify();
        }
    }

    fn pick_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Pick mode eats every key, so no shortcut or terminal sees it. Only Escape does anything.
        cx.stop_propagation();
        if event.keystroke.key == "escape" {
            self.end_pick(None, window, cx);
        }
    }

    pub(super) fn pick_layer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.pick.as_ref()?;
        let hint = match (&session.name, session.missed) {
            (_, true) => "Nothing here can be picked. Click another spot, or press Esc.".to_owned(),
            (Some(name), false) => {
                format!("{name} asks you to pick something. Click it, or press Esc.")
            }
            (None, false) => "Click something to pick it. Press Esc to cancel.".to_owned(),
        };
        let bar = div()
            .absolute()
            .top(px(space(SpaceStep::Lg, cx)))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .h(px(24.))
                    .px(px(space(SpaceStep::Md, cx)))
                    .flex()
                    .items_center()
                    .bg(cx.theme().popover)
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(cx.theme().radius)
                    .type_small(cx)
                    .text_color(cx.theme().foreground)
                    .child(hint),
            );
        let outline = session
            .hover
            .as_ref()
            .map(|element| outline(element, cx))
            .into_iter();
        Some(
            div()
                .id("pick-layer")
                .track_focus(&self.pick_focus)
                .absolute()
                .inset_0()
                .occlude()
                .cursor(CursorStyle::Crosshair)
                .capture_key_down(cx.listener(Self::pick_key))
                .capture_any_mouse_down(cx.listener(Self::pick_click))
                .on_mouse_move(cx.listener(Self::pick_move))
                .child(bar)
                .children(outline)
                .into_any_element(),
        )
    }
}

fn to_pick(element: &ElementSnapshot, owner: Option<String>) -> UiPick {
    let shown = crate::ui_dispatch::element(element);
    UiPick {
        role: shown.role,
        label: shown.label,
        value: shown.value,
        path: shown.path,
        bounds: shown.bounds,
        owner,
    }
}

fn topmost(window: &Window, at: Point<Pixels>) -> Option<ElementSnapshot> {
    ui_registry::snapshots(window)
        .into_iter()
        .filter(|element| element.visible && element.clipped.contains(&at))
        .max_by_key(|element| element.order)
}

// The innermost plugin view on the element's path, if a plugin drew it.
fn owner(element: &ElementSnapshot, cx: &App) -> Option<String> {
    let views: Vec<EntityId> = element
        .path
        .iter()
        .filter_map(|id| match id {
            ElementId::View(entity) => Some(*entity),
            _ => None,
        })
        .collect();
    kg_plugins::PluginHost::try_global(cx)
        .and_then(|host| host.read(cx).owner_of(&views))
        .map(|plugin: PluginId| plugin.0)
}

fn outline(element: &ElementSnapshot, cx: &App) -> AnyElement {
    let area = element.clipped;
    let label = match (element.role, element.label.as_deref()) {
        (Some(role), Some(label)) => format!("{role:?} {label}"),
        (Some(role), None) => format!("{role:?}"),
        (None, Some(label)) => label.to_owned(),
        (None, None) => String::new(),
    };
    div()
        .absolute()
        .left(area.origin.x)
        .top(area.origin.y)
        .w(area.size.width)
        .h(area.size.height)
        .border_2()
        .border_color(cx.theme().ring)
        .rounded(cx.theme().radius)
        .child(
            div()
                .absolute()
                .left_0()
                .top(px(-24.))
                .max_w(px(320.))
                .h(px(20.))
                .px(px(space(SpaceStep::Sm, cx)))
                .flex()
                .items_center()
                .truncate()
                .bg(cx.theme().popover)
                .rounded(cx.theme().radius)
                .type_small(cx)
                .text_color(cx.theme().foreground)
                .child(label),
        )
        .into_any_element()
}
