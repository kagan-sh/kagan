"""SQLModel schema for the refactored domain."""

# NOTE: Avoid `from __future__ import annotations` because SQLModel evaluates

from datetime import datetime
from typing import TYPE_CHECKING, Any, Optional
from uuid import uuid4

from pydantic import BaseModel
from sqlalchemy import JSON, Column, UniqueConstraint
from sqlmodel import Field, Relationship, SQLModel

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


def _new_id() -> str:
    return uuid4().hex[:8]


class Project(SQLModel, table=True):
    """Project container."""

    __tablename__ = "projects"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    name: str = Field(index=True)
    description: str = Field(default="")
    last_opened_at: datetime | None = Field(default=None)
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

    tasks: list["Task"] = Relationship(back_populates="project")
    workspaces: list["Workspace"] = Relationship(back_populates="project")
    project_repos: list["ProjectRepo"] = Relationship(back_populates="project")


class Repo(SQLModel, table=True):
    """Repository configuration."""

    __tablename__ = "repos"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    name: str = Field(index=True)
    path: str = Field(unique=True, index=True)
    display_name: str | None = Field(default=None)
    default_working_dir: str | None = Field(default=None)
    default_branch: str = Field(default="main")
    scripts: dict[str, str] = Field(default_factory=dict, sa_column=Column(JSON))
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

    project_repos: list["ProjectRepo"] = Relationship(back_populates="repo")
    workspace_repos: list["WorkspaceRepo"] = Relationship(back_populates="repo")


class TaskTag(SQLModel, table=True):
    """Association table for tasks and tags."""

    __tablename__ = "task_tags"  # type: ignore[bad-override]

    task_id: str = Field(foreign_key="tasks.id", primary_key=True)
    tag_id: str = Field(foreign_key="tags.id", primary_key=True)


class TaskLink(SQLModel, table=True):
    """Association table for task references (@mentions)."""

    __tablename__ = "task_links"  # type: ignore[bad-override]

    task_id: str = Field(foreign_key="tasks.id", primary_key=True)
    ref_task_id: str = Field(foreign_key="tasks.id", primary_key=True)
    created_at: datetime = Field(default_factory=datetime.now)


class Task(SQLModel, table=True):
    """Unit of work (Kanban card)."""

    __tablename__ = "tasks"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    project_id: str = Field(foreign_key="projects.id", index=True)
    parent_id: str | None = Field(default=None, foreign_key="tasks.id", index=True)
    title: str = Field(index=True)
    description: str = Field(default="")
    status: TaskStatus = Field(default=TaskStatus.BACKLOG, index=True)
    priority: TaskPriority = Field(default=TaskPriority.MEDIUM, index=True)
    task_type: TaskType = Field(default=TaskType.PAIR)
    terminal_backend: PairTerminalBackend | None = Field(default=None)
    assigned_hat: str | None = Field(default=None)
    agent_backend: str | None = Field(default=None)
    base_branch: str | None = Field(default=None)
    acceptance_criteria: list[str] = Field(default_factory=list, sa_column=Column(JSON))
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

    project: Project = Relationship(back_populates="tasks")
    parent: Optional["Task"] = Relationship(
        back_populates="children",
        sa_relationship_kwargs={"remote_side": "Task.id"},
    )
    children: list["Task"] = Relationship(back_populates="parent")
    workspaces: list["Workspace"] = Relationship(back_populates="task")
    tags: list["Tag"] = Relationship(back_populates="tasks", link_model=TaskTag)
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
        terminal_backend: PairTerminalBackend | None = None,
        status: TaskStatus = TaskStatus.BACKLOG,
        assigned_hat: str | None = None,
        parent_id: str | None = None,
        agent_backend: str | None = None,
        base_branch: str | None = None,
        acceptance_criteria: list[str] | None = None,
        *,
        project_id: str,
    ) -> "Task":
        """Create a new task with generated ID and timestamps."""
        return cls(
            project_id=project_id,
            title=title,
            description=description,
            priority=priority,
            task_type=task_type,
            terminal_backend=terminal_backend,
            status=status,
            assigned_hat=assigned_hat,
            parent_id=parent_id,
            agent_backend=agent_backend,
            base_branch=base_branch,
            acceptance_criteria=acceptance_criteria or [],
        )

    def get_agent_config(self, config: "KaganConfig") -> Any:
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


class Workspace(SQLModel, table=True):
    """Worktree + branch pairing for a task."""

    __tablename__ = "workspaces"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    project_id: str = Field(foreign_key="projects.id", index=True)
    task_id: str | None = Field(default=None, foreign_key="tasks.id", index=True)
    branch_name: str
    path: str
    status: WorkspaceStatus = Field(default=WorkspaceStatus.ACTIVE, index=True)
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

    project: Project = Relationship(back_populates="workspaces")
    task: Task | None = Relationship(back_populates="workspaces")
    sessions: list["Session"] = Relationship(back_populates="workspace")
    merges: list["Merge"] = Relationship(back_populates="workspace")
    workspace_repos: list["WorkspaceRepo"] = Relationship(back_populates="workspace")


class Session(SQLModel, table=True):
    """Session for an execution backend (tmux/ACP/etc.)."""

    __tablename__ = "sessions"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    workspace_id: str = Field(foreign_key="workspaces.id", index=True)
    session_type: SessionType = Field(index=True)
    status: SessionStatus = Field(default=SessionStatus.ACTIVE, index=True)
    external_id: str | None = Field(default=None, index=True)
    started_at: datetime = Field(default_factory=datetime.now)
    ended_at: datetime | None = Field(default=None)

    workspace: Workspace = Relationship(back_populates="sessions")
    executions: list["ExecutionProcess"] = Relationship(back_populates="session")


class ExecutionProcess(SQLModel, table=True):
    """Single execution run for a workspace session."""

    __tablename__ = "execution_processes"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    session_id: str = Field(foreign_key="sessions.id", index=True)
    run_reason: ExecutionRunReason = Field(default=ExecutionRunReason.CODINGAGENT, index=True)
    executor_action: dict[str, Any] = Field(default_factory=dict, sa_column=Column(JSON))
    status: ExecutionStatus = Field(default=ExecutionStatus.RUNNING, index=True)
    exit_code: int | None = Field(default=None)
    dropped: bool = Field(default=False, index=True)
    started_at: datetime = Field(default_factory=datetime.now)
    completed_at: datetime | None = Field(default=None)
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)
    error: str | None = Field(default=None)
    metadata_: dict[str, Any] = Field(default_factory=dict, sa_column=Column("metadata", JSON))

    session: Session | None = Relationship(back_populates="executions")
    logs: list["ExecutionProcessLog"] = Relationship(back_populates="execution")
    turns: list["CodingAgentTurn"] = Relationship(back_populates="execution")
    repo_states: list["ExecutionProcessRepoState"] = Relationship(back_populates="execution")

    def sqlmodel_update(
        self,
        obj: BaseModel | dict[str, Any],
        *,
        update: dict[str, Any] | None = None,
    ) -> "ExecutionProcess":
        """Handle metadata aliasing for updates."""
        if isinstance(obj, dict):
            update_data = dict(obj)
            if "metadata" in update_data and "metadata_" not in update_data:
                update_data["metadata_"] = update_data.pop("metadata")
            return super().sqlmodel_update(update_data, update=update)
        return super().sqlmodel_update(obj, update=update)


class ExecutionProcessLog(SQLModel, table=True):
    """JSONL log stream for an execution process."""

    __tablename__ = "execution_process_logs"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    execution_process_id: str = Field(foreign_key="execution_processes.id", index=True)
    logs: str
    byte_size: int
    inserted_at: datetime = Field(default_factory=datetime.now, index=True)

    execution: ExecutionProcess = Relationship(back_populates="logs")


class CodingAgentTurn(SQLModel, table=True):
    """Prompt/summary data for an agent run."""

    __tablename__ = "coding_agent_turns"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    execution_process_id: str = Field(foreign_key="execution_processes.id", index=True)
    agent_session_id: str | None = Field(default=None)
    prompt: str | None = Field(default=None)
    summary: str | None = Field(default=None)
    seen: bool = Field(default=False)
    agent_message_id: str | None = Field(default=None)
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

    execution: ExecutionProcess = Relationship(back_populates="turns")


class ExecutionProcessRepoState(SQLModel, table=True):
    """Per-repo state snapshot for an execution."""

    __tablename__ = "execution_process_repo_states"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    execution_process_id: str = Field(foreign_key="execution_processes.id", index=True)
    repo_id: str = Field(foreign_key="repos.id", index=True)
    before_head_commit: str | None = Field(default=None)
    after_head_commit: str | None = Field(default=None)
    merge_commit: str | None = Field(default=None)
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

    execution: ExecutionProcess = Relationship(back_populates="repo_states")


class Merge(SQLModel, table=True):
    """Merge action and result."""

    __tablename__ = "merges"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    workspace_id: str = Field(foreign_key="workspaces.id", index=True)
    repo_id: str = Field(foreign_key="repos.id", index=True)
    merge_type: MergeType = Field(default=MergeType.DIRECT, index=True)
    target_branch_name: str
    merge_commit: str | None = Field(default=None)
    pr_url: str | None = Field(default=None)
    pr_number: int | None = Field(default=None)
    pr_status: MergeStatus = Field(default=MergeStatus.OPEN, index=True)
    pr_merged_at: datetime | None = Field(default=None)
    pr_merge_commit_sha: str | None = Field(default=None)
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

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
    """Scratch payload storage."""

    __tablename__ = "scratches"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    scratch_type: ScratchType = Field(index=True)
    payload: dict[str, Any] = Field(default_factory=dict, sa_column=Column(JSON))
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)


class Image(SQLModel, table=True):
    """Image attachment for a task."""

    __tablename__ = "images"  # type: ignore[bad-override]

    id: str = Field(default_factory=_new_id, primary_key=True)
    task_id: str = Field(foreign_key="tasks.id", index=True)
    uri: str
    caption: str | None = Field(default=None)
    created_at: datetime = Field(default_factory=datetime.now)

    task: Task = Relationship(back_populates="images")


class ProjectRepo(SQLModel, table=True):
    """Junction table linking projects to repos."""

    __tablename__ = "project_repos"  # type: ignore[bad-override]
    __table_args__ = (UniqueConstraint("project_id", "repo_id"),)

    id: str = Field(default_factory=_new_id, primary_key=True)
    project_id: str = Field(foreign_key="projects.id", index=True)
    repo_id: str = Field(foreign_key="repos.id", index=True)
    is_primary: bool = Field(default=False)
    display_order: int = Field(default=0)
    created_at: datetime = Field(default_factory=datetime.now)

    project: "Project" = Relationship(back_populates="project_repos")
    repo: "Repo" = Relationship(back_populates="project_repos")


class WorkspaceRepo(SQLModel, table=True):
    """Junction table linking workspaces to repos with target branch."""

    __tablename__ = "workspace_repos"  # type: ignore[bad-override]
    __table_args__ = (UniqueConstraint("workspace_id", "repo_id"),)

    id: str = Field(default_factory=_new_id, primary_key=True)
    workspace_id: str = Field(foreign_key="workspaces.id", index=True)
    repo_id: str = Field(foreign_key="repos.id", index=True)
    target_branch: str
    worktree_path: str | None = Field(default=None)
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)

    workspace: "Workspace" = Relationship(back_populates="workspace_repos")
    repo: "Repo" = Relationship(back_populates="workspace_repos")
