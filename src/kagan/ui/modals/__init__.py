"""Modal components for Kagan TUI."""

from kagan.ui.modals.actions import ModalAction
from kagan.ui.modals.confirm import ConfirmModal
from kagan.ui.modals.description_editor import DescriptionEditorModal
from kagan.ui.modals.diff import DiffModal
from kagan.ui.modals.ticket_details import TicketDetailsModal

__all__ = [
    "ConfirmModal",
    "DescriptionEditorModal",
    "DiffModal",
    "ModalAction",
    "TicketDetailsModal",
]
