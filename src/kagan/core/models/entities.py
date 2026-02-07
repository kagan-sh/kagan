"""Core domain entities.

These models are intentionally light on persistence concerns. Services and
repositories should map to/from these entities.
"""

from __future__ import annotations

from datetime import datetime  # noqa: TC003 - Pydantic needs runtime access
from typing import TYPE_CHECKING, Any

from pydantic import BaseModel, ConfigDict, Field

from kagan.core.models.enums import (
    AgentTurnKind,
    ExecutionStatus,
    MergeReadiness,
    MergeStatus,
    SessionStatus,
    SessionType,
    TaskPriority,
    TaskStatus,
    TaskType,
    WorkspaceStatus,
)

if TYPE_CHECKING:
    from kagan.config import KaganConfig


class DomainModel(BaseModel):
    """Base model with common config."""

    model_config = ConfigDict(from_attributes=True)


class Project(DomainModel):
    """Project container.

    Relationships: repos, tasks, workspaces.
    """

    id: str
    name: str
    description: str = ""
    default_repo_id: str | None = None
    created_at: datetime
    updated_at: datetime


class Repo(DomainModel):
    """Repository configuration.

    Relationships: project, tasks, workspaces.
    """

    id: str
    project_id: str
    name: str
    path: str
    default_branch: str = "main"
    scripts: dict[str, str] = Field(default_factory=dict)
    created_at: datetime
    updated_at: datetime


class Task(DomainModel):
    """Unit of work (Kanban card).

    Relationships: project, repo, parent task, workspaces, executions, merge.
    """

    id: str
    project_id: str
    repo_id: str | None = None
    title: str
    description: str = ""
    status: TaskStatus = TaskStatus.BACKLOG
    priority: TaskPriority = TaskPriority.MEDIUM
    task_type: TaskType = TaskType.PAIR
    assigned_hat: str | None = None
    agent_backend: str | None = None
    parent_id: str | None = None
    acceptance_criteria: list[str] = Field(default_factory=list)
    review_summary: str | None = None
    checks_passed: bool | None = None
    session_active: bool = False
    total_iterations: int = 0
    merge_failed: bool = False
    merge_error: str | None = None
    merge_readiness: MergeReadiness = MergeReadiness.RISK
    last_error: str | None = None
    block_reason: str | None = None
    created_at: datetime
    updated_at: datetime

    @property
    def short_id(self) -> str:
        """Return shortened ID for display."""
        return self.id[:8]

    @property
    def priority_label(self) -> str:
        """Return human-readable priority label."""
        return self.priority.label

    def get_agent_config(self, config: KaganConfig) -> Any:
        """Resolve agent config with priority order."""
        from kagan.config import get_fallback_agent_config
        from kagan.data.builtin_agents import get_builtin_agent

        if self.agent_backend:
            if builtin := get_builtin_agent(self.agent_backend):
                return builtin.config
            if agent_config := config.get_agent(self.agent_backend):
                return agent_config

        default_agent = config.general.default_worker_agent
        if builtin := get_builtin_agent(default_agent):
            return builtin.config
        if agent_config := config.get_agent(default_agent):
            return agent_config

        return get_fallback_agent_config()


class Workspace(DomainModel):
    """Worktree + branch pairing for a task."""

    id: str
    project_id: str
    repo_id: str
    task_id: str | None = None
    branch_name: str
    path: str
    status: WorkspaceStatus = WorkspaceStatus.ACTIVE
    created_at: datetime
    updated_at: datetime


class Session(DomainModel):
    """Session for an execution backend (tmux/ACP/etc.)."""

    id: str
    workspace_id: str
    session_type: SessionType
    status: SessionStatus = SessionStatus.ACTIVE
    external_id: str | None = None
    started_at: datetime
    ended_at: datetime | None = None


class ExecutionProcess(DomainModel):
    """Single execution run for a task."""

    id: str
    task_id: str
    workspace_id: str | None = None
    session_id: str | None = None
    status: ExecutionStatus = ExecutionStatus.PENDING
    executor: str
    started_at: datetime | None = None
    finished_at: datetime | None = None
    exit_code: int | None = None
    error: str | None = None
    metadata: dict[str, Any] = Field(default_factory=dict)


class AgentTurn(DomainModel):
    """Prompt/response/log/event data for an execution."""

    id: str
    execution_id: str
    kind: AgentTurnKind
    sequence: int = 0
    source: str | None = None
    content: str
    created_at: datetime
    external_id: str | None = None


class Merge(DomainModel):
    """Merge action and result."""

    id: str
    task_id: str
    workspace_id: str | None = None
    status: MergeStatus = MergeStatus.PENDING
    readiness: MergeReadiness = MergeReadiness.RISK
    pr_url: str | None = None
    pr_number: int | None = None
    error: str | None = None
    merged_at: datetime | None = None
    created_at: datetime
    updated_at: datetime


class Tag(DomainModel):
    """Label for grouping tasks."""

    id: str
    name: str
    color: str | None = None
    created_at: datetime


class Scratch(DomainModel):
    """Scratchpad content tied to a task."""

    task_id: str
    content: str = ""
    updated_at: datetime


class Image(DomainModel):
    """Image attachment for a task."""

    id: str
    task_id: str
    uri: str
    caption: str | None = None
    created_at: datetime
