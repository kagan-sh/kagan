use gpui_kit::{Element, Hitbox, InteractiveElement};

use crate::ui_registry::{Facts, Observed};

pub(crate) trait UiObserveExt:
    Element<PrepaintState = Option<Hitbox>> + InteractiveElement + Sized
{
    /// Observes a container: a box with no role, name or state of its own, such as a pane or a
    /// block of text. A control uses `ui_observe_as`.
    fn ui_container(self) -> Observed<Self> {
        Observed::new(self)
    }

    /// Observes a control. Its facts are required, and its focus is tracked when it has a handle.
    fn ui_observe_as(self, facts: Facts) -> Observed<Self> {
        let element = match &facts.focus {
            Some(focus) => self.track_focus(focus),
            None => self,
        };
        Observed::with_facts(element, facts)
    }
}

impl<E: Element<PrepaintState = Option<Hitbox>> + InteractiveElement> UiObserveExt for E {}
