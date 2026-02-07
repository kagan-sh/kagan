"""Core domain enums."""

from enum import IntEnum, StrEnum


class TaskStatus(StrEnum):
    """Task status values for Kanban columns."""

    BACKLOG = "BACKLOG"
    IN_PROGRESS = "IN_PROGRESS"
    REVIEW = "REVIEW"
    DONE = "DONE"

    @classmethod
    def next_status(cls, current: "TaskStatus") -> "TaskStatus | None":
        """Return the next status in the workflow."""
        from kagan.constants import COLUMN_ORDER

        idx = COLUMN_ORDER.index(current)
        if idx < len(COLUMN_ORDER) - 1:
            return COLUMN_ORDER[idx + 1]
        return None

    @classmethod
    def prev_status(cls, current: "TaskStatus") -> "TaskStatus | None":
        """Return the previous status in the workflow."""
        from kagan.constants import COLUMN_ORDER

        idx = COLUMN_ORDER.index(current)
        if idx > 0:
            return COLUMN_ORDER[idx - 1]
        return None


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


class ExecutionStatus(StrEnum):
    """Execution process status."""

    PENDING = "PENDING"
    RUNNING = "RUNNING"
    SUCCEEDED = "SUCCEEDED"
    FAILED = "FAILED"
    CANCELED = "CANCELED"


class AgentTurnKind(StrEnum):
    """Agent turn classification."""

    PROMPT = "PROMPT"
    RESPONSE = "RESPONSE"
    SUMMARY = "SUMMARY"
    LOG = "LOG"
    EVENT = "EVENT"


class MergeReadiness(StrEnum):
    """Merge readiness indicator for review state."""

    READY = "ready"
    RISK = "risk"
    BLOCKED = "blocked"


class MergeStatus(StrEnum):
    """Merge status values."""

    PENDING = "PENDING"
    READY = "READY"
    MERGED = "MERGED"
    FAILED = "FAILED"
    BLOCKED = "BLOCKED"


class AgentStatus(StrEnum):
    """Agent availability status."""

    AVAILABLE = "available"
    UNAVAILABLE = "unavailable"
