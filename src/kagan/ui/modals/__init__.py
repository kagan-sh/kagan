"""Modal components for Kagan TUI."""

from kagan.ui.modals.actions import ModalAction
from kagan.ui.modals.agent_output import AgentOutputModal
from kagan.ui.modals.base import BaseActionModal
from kagan.ui.modals.confirm import ConfirmModal
from kagan.ui.modals.debug_log import DebugLogModal
from kagan.ui.modals.description_editor import DescriptionEditorModal
from kagan.ui.modals.diff import DiffModal
from kagan.ui.modals.duplicate_ticket import DuplicateTicketModal
from kagan.ui.modals.help import HelpModal
from kagan.ui.modals.rejection_input import RejectionInputModal
from kagan.ui.modals.review import ReviewModal
from kagan.ui.modals.settings import SettingsModal
from kagan.ui.modals.ticket_details_modal import TicketDetailsModal
from kagan.ui.modals.tmux_gateway import TmuxGatewayModal

__all__ = [
    "AgentOutputModal",
    "BaseActionModal",
    "ConfirmModal",
    "DebugLogModal",
    "DescriptionEditorModal",
    "DiffModal",
    "DuplicateTicketModal",
    "HelpModal",
    "ModalAction",
    "RejectionInputModal",
    "ReviewModal",
    "SettingsModal",
    "TicketDetailsModal",
    "TmuxGatewayModal",
]
