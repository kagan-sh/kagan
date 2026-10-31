//! Opt-in headless observation built exclusively on public GPUI APIs.
//!
//! Native accessibility properties are read automatically. Applications register
//! identified GPUI elements with `.ui_container()`, without supplying test-only state.
use gpui_kit::{
    App, Bounds, Element, ElementId, Entity, FocusHandle, Focusable as _, GlobalElementId, Hitbox,
    InspectorElementId, InteractiveElement, IntoElement, LayoutId, Pixels, Point, SharedString,
    Visibility, Window, component::input::InputState, px,
};
use std::{
    cell::RefCell,
    collections::HashMap,
    fmt,
    rc::{Rc, Weak},
};

/// Owned facts from the last paint of an observed element.
#[derive(Clone)]
pub(crate) struct ElementSnapshot {
    role: Option<gpui_kit::Role>,
    path: Vec<ElementId>,
    value: Option<SharedString>,
    bounds: Bounds<Pixels>,
    clipped: Bounds<Pixels>,
    visible: bool,
    focused: Option<bool>,
    disabled: Option<bool>,
    selected: Option<bool>,
    label: Option<SharedString>,
    hint: Option<SharedString>,
}
impl fmt::Debug for ElementSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ElementSnapshot")
            .field("role", &self.role)
            .field("path", &self.path)
            .field("value", &self.value)
            .field("bounds", &self.bounds)
            .field("clipped", &self.clipped)
            .field("visible", &self.visible)
            .field("focused", &self.focused)
            .field("disabled", &self.disabled)
            .field("selected", &self.selected)
            .field("label", &self.label)
            .field("hint", &self.hint)
            .finish()
    }
}

impl ElementSnapshot {
    pub(crate) fn role(&self) -> Option<gpui_kit::Role> {
        self.role
    }
    pub(crate) fn path(&self) -> &[ElementId] {
        &self.path
    }
    /// The native accessibility value, not rendered text or pixels.
    pub(crate) fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    /// The tooltip text the control shows on hover.
    #[cfg(test)]
    pub(crate) fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }

    pub(crate) fn bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }
    pub(crate) fn clipped(&self) -> Bounds<Pixels> {
        self.clipped
    }
    pub(crate) fn visible(&self) -> bool {
        self.visible
    }
    /// Whether the tracked focus scope contains the current keyboard focus.
    /// `None` means no focus binding was observed.
    pub(crate) fn focused(&self) -> Option<bool> {
        self.focused
    }
    /// `None` means the native element does not expose a disabled flag.
    /// Absence is not evidence that a control accepts input.
    pub(crate) fn disabled(&self) -> Option<bool> {
        self.disabled
    }
    /// Whether the view states the control is selected. Nothing native reports it.
    pub(crate) fn selected(&self) -> Option<bool> {
        self.selected
    }
    /// The accessibility label; it can intentionally differ from visible text.
    pub(crate) fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}

/// What a control states about itself, for `kg ui`. Every field is required, so a control that
/// leaves one out does not compile. A container has no facts; it uses `ui_container` instead.
pub(crate) struct Facts {
    pub(crate) role: gpui_kit::Role,
    pub(crate) name: SharedString,
    /// `None` when the control has no value.
    pub(crate) value: Option<SharedString>,
    pub(crate) disabled: bool,
    /// `None` when the control cannot be selected.
    pub(crate) selected: Option<bool>,
    /// `None` when the control takes no keyboard focus.
    pub(crate) focus: Option<FocusHandle>,
    /// The tooltip text a person sees on hover. `None` when the control has none.
    pub(crate) hint: Option<SharedString>,
}

impl Facts {
    /// A button that is enabled, unselected and has no focus of its own to report.
    pub(crate) fn button(name: impl Into<gpui_kit::SharedString>) -> Self {
        Self {
            role: gpui_kit::Role::Button,
            name: name.into(),
            value: None,
            disabled: false,
            selected: None,
            focus: None,
            hint: None,
        }
    }

    pub(crate) fn input(
        name: impl Into<SharedString>,
        input: &Entity<InputState>,
        cx: &App,
    ) -> Self {
        let state = input.read(cx);
        // A masked field says only whether it holds text, not the text or its length.
        let value = if state.presentation().is_masked() {
            if state.value().is_empty() {
                String::new()
            } else {
                "••••••••".to_owned()
            }
        } else {
            state.value().to_string()
        };
        Self {
            role: gpui_kit::Role::TextInput,
            value: Some(value.into()),
            focus: Some(input.focus_handle(cx)),
            ..Self::button(name)
        }
    }
}

// Each Window owns a distinct Arc<WindowTextSystem>, stable even during initial
// drawing before the Window itself is boxed. Its identity distinguishes equal
// WindowIds in different Apps. Entries never own a Window or a view.
type WindowKey = usize;
type Registry = HashMap<WindowKey, HashMap<GlobalElementId, Weak<Registration>>>;
thread_local! { static REGISTRY: RefCell<Registry> = RefCell::new(HashMap::new()); }

#[doc(hidden)]
pub(crate) struct Registration {
    window: WindowKey,
    global_id: GlobalElementId,
    facts: RefCell<ElementSnapshot>,
}
impl Drop for Registration {
    fn drop(&mut self) {
        let _ = REGISTRY.try_with(|registry| {
            if let Ok(mut registry) = registry.try_borrow_mut()
                && let Some(entries) = registry.get_mut(&self.window)
            {
                entries.remove(&self.global_id);
                if entries.is_empty() {
                    registry.remove(&self.window);
                }
            }
        });
    }
}

/// What a lookup found: nothing, the one element, or several that need a `within` to choose one.
pub(crate) enum Located {
    Missing,
    One(ElementSnapshot),
    /// Each match's path, outermost first, so the caller can name the one it wants.
    Ambiguous(Vec<String>),
}

/// The element with this id. With `within`, only elements inside an element with that id match.
pub(crate) fn locate(window: &Window, id: &ElementId, within: Option<&ElementId>) -> Located {
    REGISTRY.with(|registry| {
        let registry = registry.borrow();
        let Some(entries) = registry.get(&(std::sync::Arc::as_ptr(window.text_system()) as usize))
        else {
            return Located::Missing;
        };
        let matches: Vec<_> = entries
            .values()
            .filter_map(Weak::upgrade)
            .filter(|entry| {
                let Some((last, ancestors)) = entry.global_id.split_last() else {
                    return false;
                };
                last == id && within.is_none_or(|within| ancestors.contains(within))
            })
            .collect();
        match matches.as_slice() {
            [] => Located::Missing,
            [only] => Located::One(only.facts.borrow().clone()),
            many => Located::Ambiguous(
                many.iter()
                    .map(|entry| {
                        entry
                            .global_id
                            .iter()
                            .map(|part| format!("{part:?}"))
                            .collect::<Vec<_>>()
                            .join(" > ")
                    })
                    .collect(),
            ),
        }
    })
}

/// The one element with this id, for the tests. Several matches are a test failure.
#[cfg(test)]
pub(crate) fn find(window: &Window, id: &ElementId) -> Option<ElementSnapshot> {
    match locate(window, id, None) {
        Located::Missing => None,
        Located::One(element) => Some(element),
        Located::Ambiguous(paths) => panic!("ambiguous ElementId {id:?}: {}", paths.join("; ")),
    }
}

#[doc(hidden)]
pub(crate) fn snapshots(window: &Window) -> Vec<ElementSnapshot> {
    REGISTRY.with(|registry| {
        registry
            .borrow()
            .get(&(std::sync::Arc::as_ptr(window.text_system()) as usize))
            .map(|entries| {
                entries
                    .values()
                    .filter_map(Weak::upgrade)
                    .map(|entry| entry.facts.borrow().clone())
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// Transparent forwarding element. Its registration is owned by GPUI element
/// state, so normal state cleanup and cached-view replay determine its lifetime.
pub(crate) struct Observed<E> {
    inner: E,
    focus: Option<FocusHandle>,
    facts: Option<Facts>,
}
impl<E: Element> Observed<E> {
    pub(crate) fn new(inner: E) -> Self {
        Self {
            inner,
            focus: None,
            facts: None,
        }
    }
    pub(crate) fn with_facts(inner: E, facts: Facts) -> Self {
        Self {
            inner,
            focus: facts.focus.clone(),
            facts: Some(facts),
        }
    }
}
impl<E: Element<PrepaintState = Option<Hitbox>> + InteractiveElement> IntoElement for Observed<E> {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl<E: Element<PrepaintState = Option<Hitbox>> + InteractiveElement> Element for Observed<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = (Option<Hitbox>, Option<Rc<Registration>>);
    fn id(&self) -> Option<ElementId> {
        Element::id(&self.inner)
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector, window, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let Some(global_id) = id else {
            return (
                self.inner
                    .prepaint(id, inspector, bounds, layout, window, cx),
                None,
            );
        };
        // A control reports the facts it states. A container reports what its native node says.
        let stated = self.facts.as_ref();
        let role = stated.map(|stated| stated.role).or(self.inner.a11y_role());
        let mut node = gpui_kit::accesskit::Node::new(role.unwrap_or(gpui_kit::Role::Unknown));
        self.inner.write_a11y_info(&mut node);
        let facts = ElementSnapshot {
            role,
            path: global_id.to_vec(),
            value: match stated {
                Some(stated) => stated.value.clone(),
                None => node.value().map(|value| value.to_owned().into()),
            },
            bounds,
            clipped: bounds,
            visible: false,
            focused: None,
            disabled: match stated {
                Some(stated) => Some(stated.disabled),
                None => node.is_disabled().then_some(true),
            },
            selected: match stated {
                Some(stated) => stated.selected,
                None => node.is_selected(),
            },
            label: match stated {
                Some(stated) => Some(stated.name.clone()),
                None => node.label().map(|label| label.to_owned().into()),
            },
            hint: stated.and_then(|stated| stated.hint.clone()),
        };
        let registration =
            window.with_element_state(global_id, |state: Option<Rc<Registration>>, window| {
                let registration = state.unwrap_or_else(|| {
                    Rc::new(Registration {
                        window: std::sync::Arc::as_ptr(window.text_system()) as usize,
                        global_id: global_id.clone(),
                        facts: RefCell::new(facts.clone()),
                    })
                });
                *registration.facts.borrow_mut() = facts;
                REGISTRY.with(|registry| {
                    registry
                        .borrow_mut()
                        .entry(registration.window)
                        .or_default()
                        .insert(global_id.clone(), Rc::downgrade(&registration));
                });
                (registration.clone(), registration)
            });
        (
            self.inner
                .prepaint(id, inspector, bounds, layout, window, cx),
            Some(registration),
        )
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        paint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let style = self
            .inner
            .interactivity()
            .compute_style(id, paint.0.as_ref(), window, cx);
        let clipped = bounds
            .intersect(&window.content_mask().bounds)
            .intersect(&Bounds::new(Point::default(), window.viewport_size()));
        if let Some(registration) = &paint.1 {
            let mut facts = registration.facts.borrow_mut();
            facts.clipped = clipped;
            facts.visible = style.visibility != Visibility::Hidden
                && style.opacity.unwrap_or(1.) > 0.
                && clipped.size.width > px(0.)
                && clipped.size.height > px(0.);
            facts.focused = self
                .focus
                .as_ref()
                .map(|focus| focus.contains_focused(window, cx));
        }
        self.inner
            .paint(id, inspector, bounds, layout, &mut paint.0, window, cx);
    }
    fn a11y_role(&self) -> Option<gpui_kit::Role> {
        self.inner.a11y_role()
    }
    fn write_a11y_info(&self, node: &mut gpui_kit::accesskit::Node) {
        self.inner.write_a11y_info(node);
    }
    fn a11y_synthetic_children(
        &mut self,
        paint: &mut Self::PrepaintState,
        builder: &mut gpui_kit::A11ySubtreeBuilder,
    ) {
        self.inner.a11y_synthetic_children(&mut paint.0, builder);
    }
}

impl<E: Element<PrepaintState = Option<Hitbox>> + InteractiveElement + gpui_kit::Styled>
    gpui_kit::Styled for Observed<E>
{
    fn style(&mut self) -> &mut gpui_kit::StyleRefinement {
        self.inner.style()
    }
}
impl<E: Element<PrepaintState = Option<Hitbox>> + InteractiveElement> InteractiveElement
    for Observed<E>
{
    fn interactivity(&mut self) -> &mut gpui_kit::Interactivity {
        self.inner.interactivity()
    }
    fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self.inner = self.inner.track_focus(focus);
        self
    }
}
impl<E: Element<PrepaintState = Option<Hitbox>> + InteractiveElement + gpui_kit::ParentElement>
    gpui_kit::ParentElement for Observed<E>
{
    fn extend(&mut self, elements: impl IntoIterator<Item = gpui_kit::AnyElement>) {
        self.inner.extend(elements);
    }
}

impl<E: Element<PrepaintState = Option<Hitbox>> + gpui_kit::StatefulInteractiveElement>
    gpui_kit::StatefulInteractiveElement for Observed<E>
{
}

impl<
    E: Element<PrepaintState = Option<Hitbox>>
        + InteractiveElement
        + gpui_kit::Styled
        + gpui_kit::ParentElement,
> gpui_kit::component::scroll::ScrollableElement for Observed<E>
{
}
