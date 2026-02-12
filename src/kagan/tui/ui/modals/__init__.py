"""Modal components for Kagan TUI."""

from kagan.tui.ui.modals.agent_choice import AgentChoiceModal
from kagan.tui.ui.modals.branch_select import BaseBranchModal
from kagan.tui.ui.modals.confirm import ConfirmModal
from kagan.tui.ui.modals.debug_log import DebugLogModal
from kagan.tui.ui.modals.description_editor import DescriptionEditorModal
from kagan.tui.ui.modals.diff import DiffModal
from kagan.tui.ui.modals.folder_picker import FolderPickerModal
from kagan.tui.ui.modals.global_agent_picker import GlobalAgentPickerModal
from kagan.tui.ui.modals.help import HelpModal
from kagan.tui.ui.modals.instance_locked import InstanceLockedModal
from kagan.tui.ui.modals.merge_dialog import MergeDialog
from kagan.tui.ui.modals.new_project import NewProjectModal
from kagan.tui.ui.modals.rejection_input import RejectionInputModal
from kagan.tui.ui.modals.review import ReviewModal
from kagan.tui.ui.modals.settings import SettingsModal
from kagan.tui.ui.modals.start_workspace import StartWorkspaceModal
from kagan.tui.ui.modals.task_details_modal import ModalAction, TaskDetailsModal
from kagan.tui.ui.modals.tmux_gateway import PairInstructionsModal, TmuxGatewayModal

__all__ = [
    "AgentChoiceModal",
    "BaseBranchModal",
    "ConfirmModal",
    "DebugLogModal",
    "DescriptionEditorModal",
    "DiffModal",
    "FolderPickerModal",
    "GlobalAgentPickerModal",
    "HelpModal",
    "InstanceLockedModal",
    "MergeDialog",
    "ModalAction",
    "NewProjectModal",
    "PairInstructionsModal",
    "RejectionInputModal",
    "ReviewModal",
    "SettingsModal",
    "StartWorkspaceModal",
    "TaskDetailsModal",
    "TmuxGatewayModal",
]
