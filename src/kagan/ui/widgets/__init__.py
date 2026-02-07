"""Widget components for Kagan TUI."""

from kagan.ui.widgets.agent_content import (
    StreamingMarkdown,
    UserInput,
)
from kagan.ui.widgets.base import (
    AcceptanceCriteriaArea,
    AgentBackendSelect,
    DescriptionArea,
    PrioritySelect,
    StatusSelect,
    TaskTypeSelect,
    TitleInput,
)
from kagan.ui.widgets.card import TaskCard
from kagan.ui.widgets.chat_panel import ChatPanel
from kagan.ui.widgets.column import KanbanColumn
from kagan.ui.widgets.empty_state import EmptyState
from kagan.ui.widgets.header import KaganHeader
from kagan.ui.widgets.peek_overlay import PeekOverlay
from kagan.ui.widgets.permission_prompt import PermissionPrompt
from kagan.ui.widgets.plan_approval import PlanApprovalWidget
from kagan.ui.widgets.plan_display import PlanDisplay
from kagan.ui.widgets.search_bar import SearchBar
from kagan.ui.widgets.slash_complete import SlashComplete
from kagan.ui.widgets.status_bar import StatusBar
from kagan.ui.widgets.streaming_output import StreamingOutput
from kagan.ui.widgets.tool_call import ToolCall
from kagan.ui.widgets.workspace_repos import WorkspaceReposWidget

__all__ = [
    "AcceptanceCriteriaArea",
    "AgentBackendSelect",
    "ChatPanel",
    "DescriptionArea",
    "EmptyState",
    "KaganHeader",
    "KanbanColumn",
    "PeekOverlay",
    "PermissionPrompt",
    "PlanApprovalWidget",
    "PlanDisplay",
    "PrioritySelect",
    "SearchBar",
    "SlashComplete",
    "StatusBar",
    "StatusSelect",
    "StreamingMarkdown",
    "StreamingOutput",
    "TaskCard",
    "TaskTypeSelect",
    "TitleInput",
    "ToolCall",
    "UserInput",
    "WorkspaceReposWidget",
]
