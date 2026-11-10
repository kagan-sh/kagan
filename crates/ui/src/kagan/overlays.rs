use gpui_kit::{
    AnyElement, App, Bounds, ContentMask, Context, Element, ElementId, Entity, GlobalElementId,
    InspectorElementId, InteractiveElement as _, IntoElement, LayoutId, ParentElement as _, Pixels,
    Styled as _, Window, canvas, div,
};
use gpui_shell::ScriptView;
use kg_core::Workspace;
use kg_domain::PluginId;
use kg_plugins::{HostUi, PluginHost};

use super::screen::Center;
use super::surface::mount_error;
use super::{Kagan, Shown};
use crate::ui_observe::UiObserveExt as _;

// A plugin's paint-only view drawn over the center (decision 100).
pub(super) struct Overlay {
    plugin: PluginId,
    activation: Option<u64>,
    generation: Option<u64>,
    view: Option<Entity<ScriptView>>,
}

impl Kagan {
    pub(super) fn sync_overlays(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = PluginHost::try_global(cx) else {
            return;
        };
        let declared: Vec<PluginId> = kg_plugins::contributions(cx)
            .map(|list| {
                list.into_iter()
                    .filter(|(_, ui)| ui.overlay.as_deref().is_some())
                    .map(|(plugin, _)| plugin)
                    .collect()
            })
            .unwrap_or_default();
        let current = |plugin: &PluginId, cx: &App| {
            let host = host.read(cx);
            (host.activation(plugin), host.generation(plugin))
        };
        self.slots.overlays.retain(|overlay| {
            let (activation, generation) = current(&overlay.plugin, cx);
            declared.contains(&overlay.plugin)
                && activation == overlay.activation
                && (overlay.view.is_none() || generation == overlay.generation)
        });
        for plugin in declared {
            if self
                .slots
                .overlays
                .iter()
                .any(|overlay| overlay.plugin == plugin)
            {
                continue;
            }
            let view = host
                .update(cx, |host, cx| host.mount_overlay(&plugin, window, cx))
                .inspect_err(|error| {
                    let message = format!("could not draw the overlay: {}", mount_error(error));
                    host.read(cx).report(&plugin, &message, cx);
                })
                .ok(); // The error is logged above.
            let (activation, generation) = current(&plugin, cx);
            self.slots.overlays.push(Overlay {
                plugin,
                activation,
                generation,
                view,
            });
        }
        cx.notify();
    }
}

fn drawn_over(kagan: &Kagan, cx: &App) -> bool {
    let work = match kagan.screen.current() {
        Center::Work(Shown::Thread(_)) | Center::Page(_) => true,
        Center::Work(Shown::Nothing | Shown::Start(_))
        | Center::Review(_)
        | Center::Settings
        | Center::Archived(_)
        | Center::Skills(..) => false,
    };
    let shown = kagan.shown_thread(cx);
    let waiting = Workspace::global(cx).read(cx).approvals().next().is_some()
        || kagan
            .questions
            .read(cx)
            .questions
            .iter()
            .any(|question| Some(&question.thread_id) == shown.as_ref());
    work && !waiting
}

pub(super) fn render(kagan: &Kagan, cx: &App) -> Option<AnyElement> {
    if !drawn_over(kagan, cx) {
        return None;
    }
    let drawn: Vec<_> = kagan
        .slots
        .overlays
        .iter()
        .filter_map(|overlay| Some((overlay, overlay.view.clone()?)))
        .filter(|(_, view)| view.read(cx).build_error().is_none())
        .collect();
    if drawn.is_empty() {
        return None;
    }
    let id = kagan.window.window_id();
    let measure = canvas(
        move |bounds, window, cx| {
            let size = bounds.size;
            if let Some(ui) = HostUi::global(cx)
                && !ui.read(cx).overlay_is(id, size)
            {
                window.defer(cx, move |_, cx| {
                    ui.update(cx, |ui, _| ui.set_overlay(id, size));
                });
            }
        },
        |_, (), _, _| {},
    )
    .absolute()
    .size_full();
    Some(
        div()
            .id("plugin-overlays")
            .ui_container()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .child(measure)
            .children(drawn.into_iter().map(|(overlay, view)| {
                div()
                    .id(ElementId::Name(
                        format!("plugin-overlay-{}", overlay.plugin).into(),
                    ))
                    .ui_container()
                    .absolute()
                    .inset_0()
                    .child(Inert {
                        view: view.clone(),
                        child: div().size_full().child(view).into_any_element(),
                    })
            }))
            .into_any_element(),
    )
}

// Paints a paint-only view, or nothing once its build failed: the error box would cover the
// terminal. Any hitbox inside is clipped to nothing as well, though a paint-only view has none.
struct Inert {
    view: Entity<ScriptView>,
    child: AnyElement,
}

impl IntoElement for Inert {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Inert {
    type RequestLayoutState = ();
    type PrepaintState = bool;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let drawn = self.view.read(cx).build_error().is_none();
        if drawn {
            window.with_content_mask(Some(ContentMask::default()), |window| {
                self.child.prepaint(window, cx);
            });
        }
        drawn
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut (),
        drawn: &mut bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        if *drawn {
            self.child.paint(window, cx);
        }
    }
}
