"""Core domain entities."""

from __future__ import annotations

from datetime import datetime  # noqa: TC003 - Pydantic needs runtime access
from typing import TYPE_CHECKING, Any

from pydantic import BaseModel, ConfigDict, Field

from kagan.core.models.enums import (
    ExecutionRunReason,
    ExecutionStatus,
    MergeStatus,
    MergeType,
    PairTerminalBackend,
    ScratchType,
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
    """Project container."""

    id: str
    name: str
    description: str = ""
    last_opened_at: datetime | None = None
    created_at: datetime
    updated_at: datetime


class Repo(DomainModel):
    """Repository configuration."""

    id: str
    name: str
    path: str
    display_name: str | None = None
    default_branch: str = "main"
    default_working_dir: str | None = None
    scripts: dict[str, str] = Field(default_factory=dict)
    created_at: datetime
    updated_at: datetime


class Task(DomainModel):
    """Unit of work (Kanban card)."""

    id: str
    project_id: str
    title: str
    description: str = ""
    status: TaskStatus = TaskStatus.BACKLOG
    priority: TaskPriority = TaskPriority.MEDIUM
    task_type: TaskType = TaskType.PAIR
    terminal_backend: PairTerminalBackend | None = None
    assigned_hat: str | None = None
    agent_backend: str | None = None
    base_branch: str | None = None
    parent_id: str | None = None
    acceptance_criteria: list[str] = Field(default_factory=list)
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
        from kagan.builtin_agents import get_builtin_agent
        from kagan.config import get_fallback_agent_config

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
    """Single execution run for a workspace session."""

    id: str
    session_id: str
    run_reason: ExecutionRunReason
    executor_action: dict[str, Any] = Field(default_factory=dict)
    status: ExecutionStatus = ExecutionStatus.RUNNING
    exit_code: int | None = None
    dropped: bool = False
    started_at: datetime
    completed_at: datetime | None = None
    created_at: datetime
    updated_at: datetime
    error: str | None = None
    metadata: dict[str, Any] = Field(default_factory=dict)


class ExecutionProcessLog(DomainModel):
    """JSONL log stream for an execution process."""

    execution_id: str
    logs: str
    byte_size: int
    inserted_at: datetime


class CodingAgentTurn(DomainModel):
    """Prompt/summary data for an agent run."""

    id: str
    execution_process_id: str
    agent_session_id: str | None = None
    prompt: str | None = None
    summary: str | None = None
    seen: bool = False
    agent_message_id: str | None = None
    created_at: datetime
    updated_at: datetime


class ExecutionProcessRepoState(DomainModel):
    """Per-repo state snapshot for an execution."""

    id: str
    execution_process_id: str
    repo_id: str
    before_head_commit: str | None = None
    after_head_commit: str | None = None
    merge_commit: str | None = None
    created_at: datetime
    updated_at: datetime


class Merge(DomainModel):
    """Merge action and result."""

    id: str
    workspace_id: str
    repo_id: str
    merge_type: MergeType
    target_branch_name: str
    merge_commit: str | None = None
    pr_url: str | None = None
    pr_number: int | None = None
    pr_status: MergeStatus = MergeStatus.OPEN
    pr_merged_at: datetime | None = None
    pr_merge_commit_sha: str | None = None
    created_at: datetime


class Tag(DomainModel):
    """Label for grouping tasks."""

    id: str
    name: str
    color: str | None = None
    created_at: datetime


class Scratch(DomainModel):
    """Scratch payload storage."""

    id: str
    scratch_type: ScratchType
    payload: dict[str, Any]
    created_at: datetime
    updated_at: datetime


class Image(DomainModel):
    """Image attachment for a task."""

    id: str
    task_id: str
    uri: str
    caption: str | None = None
    created_at: datetime
