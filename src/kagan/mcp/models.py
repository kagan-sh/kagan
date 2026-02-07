"""Pydantic models for MCP tool responses.

These models provide structured, schema-documented return types for MCP tools,
improving AI client understanding of the data format.
"""

from __future__ import annotations

from pydantic import BaseModel, Field


class RepoInfo(BaseModel):
    """Information about a repository in the workspace."""

    repo_id: str = Field(description="Unique repository identifier")
    name: str = Field(description="Repository name")
    path: str = Field(description="Path to the repository root")
    worktree_path: str | None = Field(default=None, description="Path to git worktree if active")
    target_branch: str | None = Field(default=None, description="Target branch for merging")
    has_changes: bool | None = Field(
        default=None, description="Whether repo has uncommitted changes"
    )
    diff_stats: str | None = Field(default=None, description="Summary of changes (e.g., '+10 -5')")


class LinkedTask(BaseModel):
    """Summary of a linked task referenced via @mention."""

    task_id: str = Field(description="Unique task identifier")
    title: str = Field(description="Task title")
    status: str = Field(description="Current status (backlog, in_progress, review, done)")
    description: str | None = Field(default=None, description="Task description")


class AgentLogEntry(BaseModel):
    """A single agent execution log entry."""

    run: int = Field(description="Run number (1 = first run)")
    content: str = Field(description="Log content")
    created_at: str = Field(description="ISO timestamp of log creation")


class TaskContext(BaseModel):
    """Full context for working on a task. Returned by get_context."""

    task_id: str = Field(description="Unique task identifier")
    title: str = Field(description="Task title")
    description: str | None = Field(default=None, description="Detailed task description")
    acceptance_criteria: list[str] | None = Field(
        default=None, description="List of criteria that must be met"
    )
    scratchpad: str | None = Field(default=None, description="Agent notes and progress tracking")
    workspace_id: str | None = Field(default=None, description="Active workspace ID if any")
    workspace_branch: str | None = Field(
        default=None, description="Git branch name for the workspace"
    )
    workspace_path: str | None = Field(default=None, description="Path to workspace directory")
    working_dir: str | None = Field(default=None, description="Primary working directory for agent")
    repos: list[RepoInfo] = Field(default_factory=list, description="Repositories in workspace")
    repo_count: int = Field(default=0, description="Number of repositories")
    linked_tasks: list[LinkedTask] = Field(
        default_factory=list, description="Tasks referenced via @mentions"
    )


class TaskSummary(BaseModel):
    """Brief task summary for listings and coordination."""

    task_id: str = Field(description="Unique task identifier")
    title: str = Field(description="Task title")
    status: str | None = Field(default=None, description="Current task status")
    description: str | None = Field(default=None, description="Task description")
    scratchpad: str | None = Field(default=None, description="Agent notes")
    acceptance_criteria: list[str] | None = Field(default=None, description="Acceptance criteria")


class TaskDetails(BaseModel):
    """Detailed task information. Returned by get_task."""

    task_id: str = Field(description="Unique task identifier")
    title: str = Field(description="Task title")
    status: str = Field(description="Current status")
    description: str | None = Field(default=None, description="Task description")
    acceptance_criteria: list[str] | None = Field(default=None, description="Acceptance criteria")
    scratchpad: str | None = Field(default=None, description="Agent notes (if requested)")
    review_feedback: str | None = Field(
        default=None, description="Review feedback (if requested and available)"
    )
    logs: list[AgentLogEntry] | None = Field(
        default=None, description="Agent execution logs (if requested)"
    )


class ReviewResponse(BaseModel):
    """Response from request_review tool."""

    status: str = Field(description="'review' for success, 'error' for failure")
    message: str = Field(description="Human-readable status message")


class PlanProposalResponse(BaseModel):
    """Response from propose_plan tool."""

    status: str = Field(description="'received' when plan was accepted")
    task_count: int = Field(description="Number of tasks in the proposal")
    todo_count: int = Field(description="Number of todos in the proposal")
    tasks: list[dict[str, object]] | None = Field(
        default=None,
        description="Echoed normalized task payload for ACP clients that need robust parsing",
    )
    todos: list[dict[str, object]] | None = Field(
        default=None,
        description="Echoed normalized todo payload for ACP clients that need robust parsing",
    )
