"""Pydantic models for database entities."""

from __future__ import annotations

from datetime import datetime
from enum import Enum
from typing import Any
from uuid import uuid4

from pydantic import BaseModel, ConfigDict, Field, field_validator


class TicketStatus(str, Enum):
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


class TicketType(str, Enum):
    """Ticket execution type."""

    AUTO = "AUTO"  # Autonomous execution via ACP scheduler
    PAIR = "PAIR"  # Pair programming via tmux session


# --- Shared coercion helpers ---


def _coerce_status(v: Any, allow_none: bool = False) -> TicketStatus | None:
    """Coerce a value to TicketStatus."""
    if v is None:
        return None if allow_none else v
    return TicketStatus(v) if isinstance(v, str) else v


def _coerce_ticket_type(v: Any, allow_none: bool = False) -> TicketType | None:
    """Coerce a value to TicketType."""
    if v is None:
        return None if allow_none else v
    return TicketType(v) if isinstance(v, str) else v


def _coerce_priority(v: Any, allow_none: bool = False) -> TicketPriority | None:
    """Coerce a value to TicketPriority."""
    if v is None:
        return None if allow_none else v
    return TicketPriority(v) if isinstance(v, (str, int)) else v


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
        return _coerce_status(v)

    @field_validator("ticket_type", mode="before")
    @classmethod
    def coerce_ticket_type(cls, v: Any) -> TicketType | None:
        return _coerce_ticket_type(v)

    @field_validator("priority", mode="before")
    @classmethod
    def coerce_priority(cls, v: Any) -> TicketPriority | None:
        return _coerce_priority(v)

    model_config = ConfigDict()

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
        )

    def to_insert_params(
        self,
        serialize_criteria: Any,  # Callable[[list[str]], str]
    ) -> tuple[Any, ...]:
        """Build INSERT parameters for database storage.

        Args:
            serialize_criteria: Function to serialize acceptance_criteria to string.

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
            serialize_criteria(self.acceptance_criteria),
            self.review_summary,
            None if self.checks_passed is None else (1 if self.checks_passed else 0),
            1 if self.session_active else 0,
            self.created_at.isoformat(),
            self.updated_at.isoformat(),
        )
