"""Pydantic models for database entities."""

from datetime import datetime
from enum import Enum
from uuid import uuid4

from pydantic import BaseModel, ConfigDict, Field


class TicketStatus(str, Enum):
    """Ticket status values for Kanban columns."""

    BACKLOG = "BACKLOG"
    IN_PROGRESS = "IN_PROGRESS"
    REVIEW = "REVIEW"
    DONE = "DONE"

    @classmethod
    def next_status(cls, current: "TicketStatus") -> "TicketStatus | None":
        """Get the next status in the workflow."""
        order = [cls.BACKLOG, cls.IN_PROGRESS, cls.REVIEW, cls.DONE]
        idx = order.index(current)
        if idx < len(order) - 1:
            return order[idx + 1]
        return None

    @classmethod
    def prev_status(cls, current: "TicketStatus") -> "TicketStatus | None":
        """Get the previous status in the workflow."""
        order = [cls.BACKLOG, cls.IN_PROGRESS, cls.REVIEW, cls.DONE]
        idx = order.index(current)
        if idx > 0:
            return order[idx - 1]
        return None


class TicketPriority(int, Enum):
    """Ticket priority levels."""

    LOW = 0
    MEDIUM = 1
    HIGH = 2


class Ticket(BaseModel):
    """Ticket model representing a Kanban card."""

    id: str = Field(default_factory=lambda: uuid4().hex[:8])
    title: str = Field(..., min_length=1, max_length=200)
    description: str = Field(default="")
    status: TicketStatus = Field(default=TicketStatus.BACKLOG)
    priority: TicketPriority = Field(default=TicketPriority.MEDIUM)
    assigned_hat: str | None = Field(default=None)
    parent_id: str | None = Field(default=None)
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

    @property
    def short_id(self) -> str:
        """Return shortened ID for display."""
        return self.id[:8]

    @property
    def priority_label(self) -> str:
        """Return human-readable priority label."""
        return {
            TicketPriority.LOW: "LOW",
            TicketPriority.MEDIUM: "MED",
            TicketPriority.HIGH: "HIGH",
        }[self.priority]

    model_config = ConfigDict(use_enum_values=True)


class TicketCreate(BaseModel):
    """Model for creating a new ticket."""

    title: str = Field(..., min_length=1, max_length=200)
    description: str = Field(default="")
    priority: TicketPriority = Field(default=TicketPriority.MEDIUM)
    assigned_hat: str | None = Field(default=None)
    status: TicketStatus = Field(default=TicketStatus.BACKLOG)
    parent_id: str | None = Field(default=None)


class TicketUpdate(BaseModel):
    """Model for updating a ticket."""

    title: str | None = Field(default=None, min_length=1, max_length=200)
    description: str | None = Field(default=None)
    priority: TicketPriority | None = Field(default=None)
    assigned_hat: str | None = Field(default=None)
    status: TicketStatus | None = Field(default=None)
    parent_id: str | None = Field(default=None)
