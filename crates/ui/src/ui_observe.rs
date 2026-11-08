use gpui_kit::{Element, Hitbox, InteractiveElement};

use crate::ui_registry::{Facts, Observed};

pub(crate) trait UiObserveExt:
    Element<PrepaintState = Option<Hitbox>> + InteractiveElement + Sized
{
    fn ui_container(self) -> Observed<Self> {
        Observed::new(self)
    }

    fn ui_observe_as(self, facts: Facts) -> Observed<Self> {
        let element = match &facts.focus {
            Some(focus) => self.track_focus(focus),
            None => self,
        };
        Observed::with_facts(element, facts)
    }
}

impl<E: Element<PrepaintState = Option<Hitbox>> + InteractiveElement> UiObserveExt for E {}
