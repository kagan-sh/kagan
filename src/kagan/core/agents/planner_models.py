"""Planner proposal models and conversions."""

from __future__ import annotations

from typing import Any, Literal

from acp.schema import PlanEntry
from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

from kagan.core.adapters.db.schema import Task
from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType
from kagan.core.time import utc_now


class ProposedTodo(BaseModel):
    """Planner todo entry for plan display."""

    model_config = ConfigDict(extra="ignore")

    content: str = Field(..., min_length=1, max_length=200)
    status: Literal["pending", "in_progress", "completed", "failed"] = "completed"

    @field_validator("content", mode="before")
    @classmethod
    def _clean_content(cls, v: Any) -> str:
        if v is None:
            return ""
        return str(v).strip()

    @field_validator("status", mode="before")
    @classmethod
    def _normalize_status(cls, v: Any) -> str:
        if v is None:
            return "pending"
        value = str(v).lower()
        if value in ("pending", "in_progress", "completed", "failed"):
            return value
        return "pending"


class ProposedTask(BaseModel):
    """Planner task proposal parsed from tool call arguments."""

    model_config = ConfigDict(extra="ignore")

    title: str = Field(..., min_length=1, max_length=200)
    type: Literal["AUTO", "PAIR"] = "PAIR"
    description: str = Field("", max_length=10000)
    acceptance_criteria: list[str] = Field(default_factory=list)
    priority: Literal["low", "medium", "high"] = "medium"

    @model_validator(mode="before")
    @classmethod
    def _coerce_task_type_alias(cls, value: Any) -> Any:
        if not isinstance(value, dict):
            return value
        if "task_type" not in value:
            return value

        normalized = dict(value)
        task_type = normalized.get("task_type")
        existing = normalized.get("type")
        if existing is not None and str(existing).strip().upper() != str(task_type).strip().upper():
            msg = "Conflicting task type values: 'type' and 'task_type' must match"
            raise ValueError(msg)

        normalized["type"] = task_type
        return normalized

    @field_validator("title", mode="before")
    @classmethod
    def _clean_title(cls, v: Any) -> str:
        if v is None:
            return ""
        return str(v).strip()

    @field_validator("description", mode="before")
    @classmethod
    def _clean_description(cls, v: Any) -> str:
        if v is None:
            return ""
        return str(v).strip()

    @field_validator("type", mode="before")
    @classmethod
    def _normalize_type(cls, v: Any) -> str:
        if v is None:
            return "PAIR"
        value = str(v).upper()
        return "AUTO" if value == "AUTO" else "PAIR"

    @field_validator("priority", mode="before")
    @classmethod
    def _normalize_priority(cls, v: Any) -> str:
        if v is None:
            return "medium"
        value = str(v).lower()
        if value in ("low", "medium", "high"):
            return value
        return "medium"

    @field_validator("acceptance_criteria", mode="before")
    @classmethod
    def _coerce_criteria(cls, v: Any) -> list[str]:
        if v is None:
            return []
        if isinstance(v, str):
            return [v]
        if isinstance(v, list):
            return v
        return [str(v)]

    @field_validator("acceptance_criteria")
    @classmethod
    def _clean_criteria(cls, v: list[str]) -> list[str]:
        cleaned: list[str] = []
        for item in v:
            text = str(item).strip()
            if text:
                cleaned.append(text)
        return cleaned


class PlanProposal(BaseModel):
    """Validated plan proposal from the planner tool call."""

    model_config = ConfigDict(extra="ignore")

    tasks: list[ProposedTask] = Field(..., min_length=1)
    todos: list[ProposedTodo] = Field(default_factory=list)

    @field_validator("todos", mode="before")
    @classmethod
    def _coerce_todos(cls, v: Any) -> list[Any]:
        """Coerce invalid todos input to empty list (LLM might send wrong type)."""
        if v is None:
            return []
        if isinstance(v, list):
            return v
        return []

    def to_tasks(self) -> list[Task]:
        """Convert proposed tasks into Task models."""
        from uuid import uuid4

        tasks: list[Task] = []
        for item in self.tasks:
            task_type = TaskType.AUTO if item.type == "AUTO" else TaskType.PAIR
            priority_map = {
                "low": TaskPriority.LOW,
                "medium": TaskPriority.MEDIUM,
                "high": TaskPriority.HIGH,
            }
            now = utc_now()
            tasks.append(
                Task(
                    id=uuid4().hex[:8],
                    project_id="plan",
                    title=item.title[:200],
                    description=item.description,
                    status=TaskStatus.BACKLOG,
                    priority=priority_map.get(item.priority, TaskPriority.MEDIUM),
                    task_type=task_type,
                    agent_backend=None,
                    parent_id=None,
                    acceptance_criteria=item.acceptance_criteria,
                    created_at=now,
                    updated_at=now,
                )
            )
        return tasks

    def to_plan_entries(self) -> list[PlanEntry]:
        """Convert todos to plan display entries."""
        entries: list[PlanEntry] = []
        for todo in self.todos:
            status = "completed" if todo.status == "failed" else todo.status
            entries.append(PlanEntry(content=todo.content, status=status, priority="medium"))
        return entries


__all__ = [
    "PlanProposal",
    "ProposedTask",
    "ProposedTodo",
]
