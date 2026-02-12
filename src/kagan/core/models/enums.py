"""Core domain enums."""

from __future__ import annotations

from enum import IntEnum, StrEnum


class TaskStatus(StrEnum):
    """Task status values for Kanban columns."""

    BACKLOG = "BACKLOG"
    IN_PROGRESS = "IN_PROGRESS"
    REVIEW = "REVIEW"
    DONE = "DONE"

    @classmethod
    def next_status(cls, current: TaskStatus) -> TaskStatus | None:
        """Return the next status in the workflow."""
        from kagan.core.constants import COLUMN_ORDER

        idx = COLUMN_ORDER.index(current)
        if idx < len(COLUMN_ORDER) - 1:
            return COLUMN_ORDER[idx + 1]
        return None

    @classmethod
    def prev_status(cls, current: TaskStatus) -> TaskStatus | None:
        """Return the previous status in the workflow."""
        from kagan.core.constants import COLUMN_ORDER

        idx = COLUMN_ORDER.index(current)
        if idx > 0:
            return COLUMN_ORDER[idx - 1]
        return None


def transition_status_from_agent_complete(current_status: TaskStatus, success: bool) -> TaskStatus:
    """Return next status after an implementation agent run completes."""
    if success and current_status == TaskStatus.IN_PROGRESS:
        return TaskStatus.REVIEW
    return current_status


def transition_status_from_review_pass(current_status: TaskStatus) -> TaskStatus:
    """Return next status after review approval."""
    if current_status == TaskStatus.REVIEW:
        return TaskStatus.DONE
    return current_status


def transition_status_from_review_reject(current_status: TaskStatus) -> TaskStatus:
    """Return next status after review rejection."""
    if current_status == TaskStatus.REVIEW:
        return TaskStatus.IN_PROGRESS
    return current_status


class TaskPriority(IntEnum):
    """Task priority levels."""

    LOW = 0
    MEDIUM = 1
    HIGH = 2

    @property
    def label(self) -> str:
        """Short display label."""
        return {self.LOW: "LOW", self.MEDIUM: "MED", self.HIGH: "HIGH"}[self]

    @property
    def css_class(self) -> str:
        """CSS class name for styling."""
        return {self.LOW: "low", self.MEDIUM: "medium", self.HIGH: "high"}[self]


class TaskType(StrEnum):
    """Task execution type."""

    AUTO = "AUTO"
    PAIR = "PAIR"


class PairTerminalBackend(StrEnum):
    """Launcher/backend options for PAIR task sessions."""

    TMUX = "tmux"
    VSCODE = "vscode"
    CURSOR = "cursor"


VALID_PAIR_BACKENDS: frozenset[str] = frozenset(b.value for b in PairTerminalBackend)


def coerce_pair_backend(value: object) -> str | None:
    """Coerce a value to a valid pair backend string, or return None."""
    if isinstance(value, PairTerminalBackend):
        return value.value
    if isinstance(value, str):
        normalized = value.strip().lower()
        if normalized in VALID_PAIR_BACKENDS:
            return normalized
    return None


class McpIdentity(StrEnum):
    """Well-known MCP identity labels."""

    DEFAULT = "kagan"
    ADMIN = "kagan_admin"


VALID_MCP_IDENTITIES: frozenset[str] = frozenset(identity.value for identity in McpIdentity)


def coerce_mcp_identity(value: object) -> str | None:
    """Coerce a value to a known MCP identity string, or return None."""
    if isinstance(value, McpIdentity):
        return value.value
    if isinstance(value, str):
        normalized = value.strip().lower()
        if normalized in VALID_MCP_IDENTITIES:
            return normalized
    return None


def resolve_pair_backend(
    task_backend: object = None,
    config_backend: object = None,
) -> str:
    """Resolve pair terminal backend: task → config → tmux default."""
    coerced = coerce_pair_backend(task_backend)
    if coerced is not None:
        return coerced
    coerced = coerce_pair_backend(config_backend)
    if coerced is not None:
        return coerced
    return PairTerminalBackend.TMUX.value


class WorkspaceStatus(StrEnum):
    """Workspace lifecycle status."""

    ACTIVE = "ACTIVE"
    ARCHIVED = "ARCHIVED"


class SessionType(StrEnum):
    """Session backend types."""

    TMUX = "TMUX"
    ACP = "ACP"
    SCRIPT = "SCRIPT"


class SessionStatus(StrEnum):
    """Session lifecycle status."""

    ACTIVE = "ACTIVE"
    CLOSED = "CLOSED"
    FAILED = "FAILED"


class ExecutionRunReason(StrEnum):
    """Execution process run reason."""

    SETUPSCRIPT = "setupscript"
    CODINGAGENT = "codingagent"
    DEVSERVER = "devserver"
    CLEANUPSCRIPT = "cleanupscript"


class ExecutionStatus(StrEnum):
    """Execution process status."""

    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"
    KILLED = "killed"


class MergeStatus(StrEnum):
    """Merge status values."""

    OPEN = "open"
    MERGED = "merged"
    CLOSED = "closed"
    UNKNOWN = "unknown"


class MergeType(StrEnum):
    """Merge type values."""

    DIRECT = "direct"
    PR = "pr"


class ScratchType(StrEnum):
    """Scratch payload types."""

    DRAFT_TASK = "DRAFT_TASK"
    DRAFT_FOLLOW_UP = "DRAFT_FOLLOW_UP"
    DRAFT_WORKSPACE = "DRAFT_WORKSPACE"
    PREVIEW_SETTINGS = "PREVIEW_SETTINGS"
    WORKSPACE_NOTES = "WORKSPACE_NOTES"


class AgentStatus(StrEnum):
    """Agent availability status."""

    AVAILABLE = "available"
    UNAVAILABLE = "unavailable"


class ReviewResult(StrEnum):
    """Review modal result actions."""

    APPROVE = "approve"
    REJECT = "reject"
    EXPLORATORY = "exploratory"


class RejectionAction(StrEnum):
    """Rejection feedback action."""

    BACKLOG = "backlog"
    IN_PROGRESS = "in_progress"


class MessageType(StrEnum):
    """Chat message types for log parsing."""

    RESPONSE = "response"
    THINKING = "thinking"
    TOOL_CALL = "tool_call"
    TOOL_CALL_UPDATE = "tool_call_update"
    PLAN = "plan"
    AGENT_READY = "agent_ready"
    AGENT_FAIL = "agent_fail"


class ExecutionKind(StrEnum):
    """Execution event kinds for automation service."""

    STATUS = "status"
    SPAWN = "spawn"


class NotificationSeverity(StrEnum):
    """Notification severity levels."""

    INFORMATION = "information"
    WARNING = "warning"
    ERROR = "error"


class ChatRole(StrEnum):
    """Chat message roles."""

    USER = "user"
    ASSISTANT = "assistant"


class StreamRole(StrEnum):
    """Streaming content roles."""

    RESPONSE = "response"
    THOUGHT = "thought"


class StreamPhase(StrEnum):
    """Phase states for streaming/review UI components."""

    IDLE = "idle"
    THINKING = "thinking"
    STREAMING = "streaming"
    COMPLETE = "complete"

    @property
    def icon(self) -> str:
        """Return status icon for this phase."""
        return {
            self.IDLE: "○",
            self.THINKING: "◐",
            self.STREAMING: "◐",
            self.COMPLETE: "✓",
        }[self]

    @property
    def label(self) -> str:
        """Return display label for this phase."""
        return {
            self.IDLE: "Ready",
            self.THINKING: "Analyzing",
            self.STREAMING: "Streaming",
            self.COMPLETE: "Complete",
        }[self]


class ProposalStatus(StrEnum):
    """Planner proposal lifecycle status."""

    DRAFT = "draft"
    APPROVED = "approved"
    REJECTED = "rejected"


class PlanStatus(StrEnum):
    """Plan entry status values."""

    PENDING = "pending"
    IN_PROGRESS = "in_progress"
    COMPLETED = "completed"
    FAILED = "failed"

    @property
    def icon(self) -> str:
        """Return status icon for plan entries."""
        return {
            self.PENDING: "○",
            self.IN_PROGRESS: "◐",
            self.COMPLETED: "●",
            self.FAILED: "✗",
        }[self]


class CardIndicator(StrEnum):
    """Card status indicator for Kanban board."""

    NONE = "none"
    RUNNING = "running"
    IDLE = "idle"
    REVIEWING = "reviewing"
    BLOCKED = "blocked"
    PASSED = "passed"
    FAILED = "failed"

    @property
    def icon(self) -> str:
        """Return status icon for card indicator."""
        return {
            self.NONE: "",
            self.RUNNING: "▶",
            self.IDLE: "⏸",
            self.REVIEWING: "⟳",
            self.BLOCKED: "⛔",
            self.PASSED: "✓",
            self.FAILED: "✗",
        }[self]

    @property
    def css_class(self) -> str:
        """Return CSS class name for this indicator."""
        return f"indicator-{self.value}"


class ToolCallStatus(StrEnum):
    """Tool call execution status."""

    PENDING = "pending"
    IN_PROGRESS = "in_progress"
    COMPLETED = "completed"
    FAILED = "failed"

    @property
    def icon(self) -> str:
        """Return status icon for tool calls."""
        return {
            self.PENDING: " ⏲",
            self.IN_PROGRESS: " ⋯",
            self.COMPLETED: " ✔",
            self.FAILED: " ❌",
        }[self]
