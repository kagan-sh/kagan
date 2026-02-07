"""SQLModel schema for the refactored domain."""

# NOTE: Avoid `from __future__ import annotations` because SQLModel evaluates
# type hints at runtime for relationships.

from datetime import datetime
from typing import TYPE_CHECKING, Any, Optional
from uuid import uuid4

from sqlalchemy import JSON, Column
from sqlmodel import Field, Relationship, SQLModel

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


def _new_id() -> str:
    return uuid4().hex[:8]


class Project(SQLModel, table=True):
    """Project container."""

    __tablename__ = "projects"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    name: str = Field(index=True)
    description: str = Field(default="")
    default_repo_id: str | None = Field(default=None, foreign_key="repos.id")
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

    repos: list["Repo"] = Relationship(
        back_populates="project",
        sa_relationship_kwargs={"foreign_keys": "Repo.project_id"},
    )
    tasks: list["Task"] = Relationship(back_populates="project")
    workspaces: list["Workspace"] = Relationship(back_populates="project")


class Repo(SQLModel, table=True):
    """Repository configuration."""

    __tablename__ = "repos"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    project_id: str = Field(foreign_key="projects.id", index=True)
    name: str = Field(index=True)
    path: str
    default_branch: str = Field(default="main")
    scripts: dict[str, str] = Field(default_factory=dict, sa_column=Column(JSON))
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

    project: Project = Relationship(
        back_populates="repos",
        sa_relationship_kwargs={"foreign_keys": "Repo.project_id"},
    )
    tasks: list["Task"] = Relationship(back_populates="repo")
    workspaces: list["Workspace"] = Relationship(back_populates="repo")


class TaskTag(SQLModel, table=True):
    """Association table for tasks and tags."""

    __tablename__ = "task_tags"  # type: ignore[bad-override]

    task_id: str = Field(foreign_key="tasks.id", primary_key=True)
    tag_id: str = Field(foreign_key="tags.id", primary_key=True)


class Task(SQLModel, table=True):
    """Unit of work (Kanban card)."""

    __tablename__ = "tasks"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    project_id: str = Field(foreign_key="projects.id", index=True)
    repo_id: str | None = Field(default=None, foreign_key="repos.id", index=True)
    parent_id: str | None = Field(default=None, foreign_key="tasks.id", index=True)
    title: str = Field(index=True)
    description: str = Field(default="")
    status: TaskStatus = Field(default=TaskStatus.BACKLOG, index=True)
    priority: TaskPriority = Field(default=TaskPriority.MEDIUM, index=True)
    task_type: TaskType = Field(default=TaskType.PAIR)
    assigned_hat: str | None = Field(default=None)
    agent_backend: str | None = Field(default=None)
    acceptance_criteria: list[str] = Field(default_factory=list, sa_column=Column(JSON))
    review_summary: str | None = Field(default=None)
    checks_passed: bool | None = Field(default=None)
    session_active: bool = Field(default=False)
    total_iterations: int = Field(default=0)
    merge_failed: bool = Field(default=False)
    merge_error: str | None = Field(default=None)
    merge_readiness: MergeReadiness = Field(default=MergeReadiness.RISK)
    last_error: str | None = Field(default=None)
    block_reason: str | None = Field(default=None)
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

    project: Project = Relationship(back_populates="tasks")
    repo: Repo | None = Relationship(back_populates="tasks")
    parent: Optional["Task"] = Relationship(
        back_populates="children",
        sa_relationship_kwargs={"remote_side": "Task.id"},
    )
    children: list["Task"] = Relationship(back_populates="parent")
    workspaces: list["Workspace"] = Relationship(back_populates="task")
    executions: list["ExecutionProcess"] = Relationship(back_populates="task")
    merges: list["Merge"] = Relationship(back_populates="task")
    tags: list["Tag"] = Relationship(back_populates="tasks", link_model=TaskTag)
    scratch: Optional["Scratch"] = Relationship(
        back_populates="task",
        sa_relationship_kwargs={"cascade": "all, delete-orphan"},
    )
    images: list["Image"] = Relationship(back_populates="task")

    @property
    def short_id(self) -> str:
        """Return shortened ID for display."""
        return (self.id or "")[:8]

    @property
    def priority_label(self) -> str:
        """Return human-readable priority label."""
        return self.priority.label

    @classmethod
    def create(
        cls,
        title: str,
        description: str = "",
        priority: TaskPriority = TaskPriority.MEDIUM,
        task_type: TaskType = TaskType.PAIR,
        status: TaskStatus = TaskStatus.BACKLOG,
        assigned_hat: str | None = None,
        parent_id: str | None = None,
        agent_backend: str | None = None,
        acceptance_criteria: list[str] | None = None,
        review_summary: str | None = None,
        checks_passed: bool | None = None,
        session_active: bool = False,
        total_iterations: int = 0,
        merge_failed: bool = False,
        merge_error: str | None = None,
        merge_readiness: MergeReadiness = MergeReadiness.RISK,
        last_error: str | None = None,
        block_reason: str | None = None,
        *,
        project_id: str,
        repo_id: str | None = None,
    ) -> "Task":
        """Create a new task with generated ID and timestamps."""
        return cls(
            project_id=project_id,
            repo_id=repo_id,
            title=title,
            description=description,
            priority=priority,
            task_type=task_type,
            status=status,
            assigned_hat=assigned_hat,
            parent_id=parent_id,
            agent_backend=agent_backend,
            acceptance_criteria=acceptance_criteria or [],
            review_summary=review_summary,
            checks_passed=checks_passed,
            session_active=session_active,
            total_iterations=total_iterations,
            merge_failed=merge_failed,
            merge_error=merge_error,
            merge_readiness=merge_readiness,
            last_error=last_error,
            block_reason=block_reason,
        )

    def get_agent_config(self, config: "KaganConfig") -> Any:
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


class Workspace(SQLModel, table=True):
    """Worktree + branch pairing for a task."""

    __tablename__ = "workspaces"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    project_id: str = Field(foreign_key="projects.id", index=True)
    repo_id: str = Field(foreign_key="repos.id", index=True)
    task_id: str | None = Field(default=None, foreign_key="tasks.id", index=True)
    branch_name: str
    path: str
    status: WorkspaceStatus = Field(default=WorkspaceStatus.ACTIVE, index=True)
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

    project: Project = Relationship(back_populates="workspaces")
    repo: Repo = Relationship(back_populates="workspaces")
    task: Task | None = Relationship(back_populates="workspaces")
    sessions: list["Session"] = Relationship(back_populates="workspace")
    executions: list["ExecutionProcess"] = Relationship(back_populates="workspace")
    merges: list["Merge"] = Relationship(back_populates="workspace")


class Session(SQLModel, table=True):
    """Session for an execution backend (tmux/ACP/etc.)."""

    __tablename__ = "sessions"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    workspace_id: str = Field(foreign_key="workspaces.id", index=True)
    session_type: SessionType = Field(index=True)
    status: SessionStatus = Field(default=SessionStatus.ACTIVE, index=True)
    external_id: str | None = Field(default=None)
    started_at: datetime = Field(default_factory=datetime.now)
    ended_at: datetime | None = Field(default=None)

    workspace: Workspace = Relationship(back_populates="sessions")
    executions: list["ExecutionProcess"] = Relationship(back_populates="session")


class ExecutionProcess(SQLModel, table=True):
    """Single execution run for a task."""

    __tablename__ = "execution_processes"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    task_id: str = Field(foreign_key="tasks.id", index=True)
    workspace_id: str | None = Field(default=None, foreign_key="workspaces.id", index=True)
    session_id: str | None = Field(default=None, foreign_key="sessions.id", index=True)
    status: ExecutionStatus = Field(default=ExecutionStatus.PENDING, index=True)
    executor: str
    started_at: datetime | None = Field(default=None)
    finished_at: datetime | None = Field(default=None)
    exit_code: int | None = Field(default=None)
    error: str | None = Field(default=None)
    metadata_: dict[str, Any] = Field(default_factory=dict, sa_column=Column("metadata", JSON))

    task: Task = Relationship(back_populates="executions")
    workspace: Workspace | None = Relationship(back_populates="executions")
    session: Session | None = Relationship(back_populates="executions")
    turns: list["AgentTurn"] = Relationship(back_populates="execution")


class AgentTurn(SQLModel, table=True):
    """Prompt/response/log/event data for an execution."""

    __tablename__ = "agent_turns"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    execution_id: str = Field(foreign_key="execution_processes.id", index=True)
    kind: AgentTurnKind = Field(index=True)
    sequence: int = Field(default=0)
    source: str | None = Field(default=None)
    content: str
    created_at: datetime = Field(default_factory=datetime.now)
    external_id: str | None = Field(default=None)

    execution: ExecutionProcess = Relationship(back_populates="turns")


class Merge(SQLModel, table=True):
    """Merge action and result."""

    __tablename__ = "merges"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    task_id: str = Field(foreign_key="tasks.id", index=True)
    workspace_id: str | None = Field(default=None, foreign_key="workspaces.id", index=True)
    status: MergeStatus = Field(default=MergeStatus.PENDING, index=True)
    readiness: MergeReadiness = Field(default=MergeReadiness.RISK, index=True)
    pr_url: str | None = Field(default=None)
    pr_number: int | None = Field(default=None)
    error: str | None = Field(default=None)
    merged_at: datetime | None = Field(default=None)
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

    task: Task = Relationship(back_populates="merges")
    workspace: Workspace | None = Relationship(back_populates="merges")


class Tag(SQLModel, table=True):
    """Label for grouping tasks."""

    __tablename__ = "tags"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    name: str = Field(index=True, unique=True)
    color: str | None = Field(default=None)
    created_at: datetime = Field(default_factory=datetime.now)

    tasks: list["Task"] = Relationship(back_populates="tags", link_model=TaskTag)


class Scratch(SQLModel, table=True):
    """Scratchpad content tied to a task."""

    __tablename__ = "scratches"  # type: ignore[bad-override]

    task_id: str = Field(primary_key=True, foreign_key="tasks.id")
    content: str = Field(default="")
    updated_at: datetime = Field(default_factory=datetime.now)

    task: Task = Relationship(back_populates="scratch")


class Image(SQLModel, table=True):
    """Image attachment for a task."""

    __tablename__ = "images"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    task_id: str = Field(foreign_key="tasks.id", index=True)
    uri: str
    caption: str | None = Field(default=None)
    created_at: datetime = Field(default_factory=datetime.now)

    task: Task = Relationship(back_populates="images")
