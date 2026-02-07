"""Pydantic models for database entities."""

from __future__ import annotations

import json
from datetime import datetime
from enum import Enum, StrEnum
from typing import TYPE_CHECKING, Any
from uuid import uuid4

from pydantic import BaseModel, ConfigDict, Field, field_validator

if TYPE_CHECKING:
    import aiosqlite


class TicketStatus(StrEnum):
    """Ticket status values for Kanban columns."""

    BACKLOG = "BACKLOG"
    IN_PROGRESS = "IN_PROGRESS"
    REVIEW = "REVIEW"
    DONE = "DONE"

    @classmethod
    def next_status(cls, current: TicketStatus) -> TicketStatus | None:
        """Get the next status in the workflow."""
        from kagan.constants import COLUMN_ORDER

        idx = COLUMN_ORDER.index(current)
        if idx < len(COLUMN_ORDER) - 1:
            return COLUMN_ORDER[idx + 1]
        return None

    @classmethod
    def prev_status(cls, current: TicketStatus) -> TicketStatus | None:
        """Get the previous status in the workflow."""
        from kagan.constants import COLUMN_ORDER

        idx = COLUMN_ORDER.index(current)
        if idx > 0:
            return COLUMN_ORDER[idx - 1]
        return None


class TicketPriority(int, Enum):
    """Ticket priority levels."""

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


class TicketType(StrEnum):
    """Ticket execution type."""

    AUTO = "AUTO"  # Autonomous execution via ACP scheduler
    PAIR = "PAIR"  # Pair programming via tmux session


class MergeReadiness(StrEnum):
    """Merge readiness indicator for REVIEW tickets."""

    READY = "ready"
    RISK = "risk"
    BLOCKED = "blocked"


# --- Shared coercion helpers ---


def _coerce_enum[E: Enum](
    v: Any, enum_cls: type[E], coerce_types: tuple[type, ...] = (str,)
) -> E | None:
    """Coerce a value to an enum type.

    Args:
        v: Value to coerce
        enum_cls: Target enum class
        coerce_types: Types that should be coerced (default: str only)

    Returns:
        Coerced enum value or None
    """
    if v is None:
        return None
    return enum_cls(v) if isinstance(v, coerce_types) else v


class AgentLogEntry(BaseModel):
    """Entry for agent execution log."""

    id: int
    ticket_id: str
    log_type: str  # 'implementation' or 'review'
    iteration: int
    content: str
    created_at: datetime


class TicketEvent(BaseModel):
    """Audit event for ticket actions."""

    id: int
    ticket_id: str
    event_type: str
    message: str
    created_at: datetime


class Ticket(BaseModel):
    """Ticket model representing a Kanban card."""

    id: str = Field(default_factory=lambda: uuid4().hex[:8])
    title: str = Field(..., min_length=1, max_length=200)
    description: str = Field(default="", max_length=10000)
    status: TicketStatus = Field(default=TicketStatus.BACKLOG)
    priority: TicketPriority = Field(default=TicketPriority.MEDIUM)
    ticket_type: TicketType = Field(default=TicketType.PAIR)
    assigned_hat: str | None = Field(default=None)
    parent_id: str | None = Field(default=None)
    agent_backend: str | None = Field(default=None)
    acceptance_criteria: list[str] = Field(default_factory=list)
    review_summary: str | None = Field(default=None, max_length=5000)
    checks_passed: bool | None = Field(default=None)
    session_active: bool = Field(default=False)
    total_iterations: int = Field(default=0)
    merge_failed: bool = Field(default=False)
    merge_error: str | None = Field(default=None, max_length=500)
    merge_readiness: MergeReadiness = Field(default=MergeReadiness.RISK)
    last_error: str | None = Field(default=None, max_length=500)
    block_reason: str | None = Field(default=None, max_length=500)
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

    @property
    def short_id(self) -> str:
        """Return shortened ID for display."""
        return self.id[:8]

    @property
    def priority_label(self) -> str:
        """Return human-readable priority label."""
        return self.priority.label

    @field_validator("status", mode="before")
    @classmethod
    def coerce_status(cls, v: Any) -> TicketStatus | None:
        return _coerce_enum(v, TicketStatus)

    @field_validator("ticket_type", mode="before")
    @classmethod
    def coerce_ticket_type(cls, v: Any) -> TicketType | None:
        return _coerce_enum(v, TicketType)

    @field_validator("priority", mode="before")
    @classmethod
    def coerce_priority(cls, v: Any) -> TicketPriority | None:
        return _coerce_enum(v, TicketPriority, coerce_types=(str, int))

    @field_validator("merge_readiness", mode="before")
    @classmethod
    def coerce_merge_readiness(cls, v: Any) -> MergeReadiness | None:
        return _coerce_enum(v, MergeReadiness)

    model_config = ConfigDict()

    @classmethod
    def from_row(cls, row: aiosqlite.Row) -> Ticket:
        """Convert a database row to a Ticket model."""
        ticket_type_raw = row["ticket_type"]
        ticket_type = TicketType(ticket_type_raw) if ticket_type_raw else TicketType.PAIR

        # Deserialize acceptance_criteria
        acceptance_criteria: list[str] = []
        if raw_criteria := row["acceptance_criteria"]:
            try:
                parsed = json.loads(raw_criteria)
                if isinstance(parsed, list):
                    acceptance_criteria = [str(item) for item in parsed]
            except json.JSONDecodeError:
                acceptance_criteria = [raw_criteria]

        return cls(
            id=row["id"],
            title=row["title"],
            description=row["description"] or "",
            status=TicketStatus(row["status"]),
            priority=TicketPriority(row["priority"]),
            ticket_type=ticket_type,
            assigned_hat=row["assigned_hat"],
            agent_backend=row["agent_backend"],
            parent_id=row["parent_id"],
            acceptance_criteria=acceptance_criteria,
            review_summary=row["review_summary"],
            checks_passed=None if row["checks_passed"] is None else bool(row["checks_passed"]),
            session_active=bool(row["session_active"]),
            total_iterations=row["total_iterations"] or 0,
            merge_failed=bool(row["merge_failed"]) if row["merge_failed"] is not None else False,
            merge_error=row["merge_error"],
            merge_readiness=MergeReadiness(row["merge_readiness"])
            if row["merge_readiness"]
            else MergeReadiness.RISK,
            last_error=row["last_error"],
            block_reason=row["block_reason"],
            created_at=(
                datetime.fromisoformat(row["created_at"]) if row["created_at"] else datetime.now()
            ),
            updated_at=(
                datetime.fromisoformat(row["updated_at"]) if row["updated_at"] else datetime.now()
            ),
        )

    @classmethod
    def create(
        cls,
        title: str,
        description: str = "",
        priority: TicketPriority = TicketPriority.MEDIUM,
        ticket_type: TicketType = TicketType.PAIR,
        status: TicketStatus = TicketStatus.BACKLOG,
        assigned_hat: str | None = None,
        parent_id: str | None = None,
        agent_backend: str | None = None,
        acceptance_criteria: list[str] | None = None,
        review_summary: str | None = None,
        checks_passed: bool | None = None,
        session_active: bool = False,
        merge_failed: bool = False,
        merge_error: str | None = None,
        merge_readiness: MergeReadiness = MergeReadiness.RISK,
        last_error: str | None = None,
        block_reason: str | None = None,
    ) -> Ticket:
        """Create a new ticket with generated ID and timestamps."""
        return cls(
            title=title,
            description=description,
            priority=priority,
            ticket_type=ticket_type,
            status=status,
            assigned_hat=assigned_hat,
            parent_id=parent_id,
            agent_backend=agent_backend,
            acceptance_criteria=acceptance_criteria or [],
            review_summary=review_summary,
            checks_passed=checks_passed,
            session_active=session_active,
            merge_failed=merge_failed,
            merge_error=merge_error,
            merge_readiness=merge_readiness,
            last_error=last_error,
            block_reason=block_reason,
        )

    def get_agent_config(self, config: Any) -> Any:  # KaganConfig -> AgentConfig
        """Resolve agent config with priority order.

        Priority:
        1. ticket.agent_backend (explicit override per ticket)
        2. config.general.default_worker_agent (project default)
        3. Fallback agent config (hardcoded sensible default)

        Args:
            config: The Kagan configuration

        Returns:
            The resolved AgentConfig
        """
        from kagan.config import get_fallback_agent_config
        from kagan.data.builtin_agents import get_builtin_agent

        # Priority 1: ticket's agent_backend field
        if self.agent_backend:
            if builtin := get_builtin_agent(self.agent_backend):
                return builtin.config
            if agent_config := config.get_agent(self.agent_backend):
                return agent_config

        # Priority 2: config's default_worker_agent
        default_agent = config.general.default_worker_agent
        if builtin := get_builtin_agent(default_agent):
            return builtin.config
        if agent_config := config.get_agent(default_agent):
            return agent_config

        # Priority 3: fallback
        return get_fallback_agent_config()

    def to_insert_params(self) -> tuple[Any, ...]:
        """Build INSERT parameters for database storage.

        Returns:
            Tuple of values for INSERT SQL statement.
        """
        return (
            self.id,
            self.title,
            self.description,
            self.status.value,
            self.priority.value,
            self.ticket_type.value,
            self.assigned_hat,
            self.agent_backend,
            self.parent_id,
            json.dumps(self.acceptance_criteria),
            self.review_summary,
            None if self.checks_passed is None else (1 if self.checks_passed else 0),
            1 if self.session_active else 0,
            self.total_iterations,
            1 if self.merge_failed else 0,
            self.merge_error,
            self.merge_readiness.value,
            self.last_error,
            self.block_reason,
            self.created_at.isoformat(),
            self.updated_at.isoformat(),
        )
