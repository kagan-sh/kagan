"""Modal components for Kagan TUI."""

from kagan.ui.modals.actions import ModalAction
from kagan.ui.modals.agent_choice import AgentChoiceModal
from kagan.ui.modals.agent_install import AgentInstallModal
from kagan.ui.modals.agent_output import AgentOutputModal
from kagan.ui.modals.branch_select import BaseBranchModal
from kagan.ui.modals.confirm import ConfirmModal
from kagan.ui.modals.debug_log import DebugLogModal
from kagan.ui.modals.description_editor import DescriptionEditorModal
from kagan.ui.modals.diff import DiffModal
from kagan.ui.modals.duplicate_task import DuplicateTaskModal
from kagan.ui.modals.folder_picker import FolderPickerModal
from kagan.ui.modals.global_agent_picker import GlobalAgentPickerModal
from kagan.ui.modals.help import HelpModal
from kagan.ui.modals.instance_locked import InstanceLockedModal
from kagan.ui.modals.mcp_install import McpInstallModal
from kagan.ui.modals.merge_dialog import MergeDialog
from kagan.ui.modals.new_project import NewProjectModal
from kagan.ui.modals.rejection_input import RejectionInputModal
from kagan.ui.modals.review import ReviewModal
from kagan.ui.modals.settings import SettingsModal
from kagan.ui.modals.start_workspace import StartWorkspaceModal
from kagan.ui.modals.task_details_modal import TaskDetailsModal
from kagan.ui.modals.tmux_gateway import PairInstructionsModal, TmuxGatewayModal

__all__ = [
    "AgentChoiceModal",
    "AgentInstallModal",
    "AgentOutputModal",
    "BaseBranchModal",
    "ConfirmModal",
    "DebugLogModal",
    "DescriptionEditorModal",
    "DiffModal",
    "DuplicateTaskModal",
    "FolderPickerModal",
    "GlobalAgentPickerModal",
    "HelpModal",
    "InstanceLockedModal",
    "McpInstallModal",
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
