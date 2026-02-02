"""Base modal classes."""

from __future__ import annotations

from typing import TYPE_CHECKING, TypeVar

from textual.screen import ModalScreen

if TYPE_CHECKING:
    from textual.widgets import Button

T = TypeVar("T")


class BaseActionModal(ModalScreen[T]):
    """Base modal with button-to-action mapping."""

    BUTTON_ACTIONS: dict[str, str] = {}

    def on_button_pressed(self, event: Button.Pressed) -> None:
        event.stop()
        if action := self.BUTTON_ACTIONS.get(event.button.id or ""):
            getattr(self, f"action_{action}")()
