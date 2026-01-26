"""Modal components for Kagan TUI."""

from kagan.ui.modals.actions import ModalAction
from kagan.ui.modals.agent_output import AgentOutputModal
from kagan.ui.modals.confirm import ConfirmModal
from kagan.ui.modals.description_editor import DescriptionEditorModal
from kagan.ui.modals.permission import PermissionModal
from kagan.ui.modals.ticket_details import TicketDetailsModal
from kagan.ui.modals.ticket_form import TicketFormModal

__all__ = [
    "AgentOutputModal",
    "ConfirmModal",
    "DescriptionEditorModal",
    "ModalAction",
    "PermissionModal",
    "TicketDetailsModal",
    "TicketFormModal",
]
