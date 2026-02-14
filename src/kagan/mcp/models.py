"""Pydantic models for MCP tool responses.

These models provide structured, schema-documented return types for MCP tools,
improving AI client understanding of the data format.
"""

from __future__ import annotations

from pydantic import BaseModel, Field


class RecoveryResponse(BaseModel):
    """Shared recovery envelope for actionable MCP responses."""

    message: str | None = Field(default=None, description="Human-readable status message")
    code: str | None = Field(default=None, description="Machine-readable status code")
    hint: str | None = Field(default=None, description="Actionable remediation guidance")
    next_tool: str | None = Field(default=None, description="Suggested next MCP tool")
    next_arguments: dict[str, object] | None = Field(
        default=None,
        description="Suggested arguments for next_tool",
    )


class MutatingResponse(RecoveryResponse):
    """Base response for mutating MCP tools."""

    success: bool = Field(description="Whether the operation succeeded")


class TaskScopedMutatingResponse(MutatingResponse):
    """Base response for mutating MCP tools that target a specific task."""

    task_id: str = Field(description="ID of the task")


class JobScopedResponse(RecoveryResponse):
    """Base response for MCP tools scoped to a specific asynchronous job."""

    success: bool = Field(description="Whether the operation succeeded")
    job_id: str = Field(description="Core job identifier")
    task_id: str = Field(description="ID of the associated task")


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


class TaskRuntimeState(BaseModel):
    """Live runtime state for task scheduling/execution."""

    is_running: bool = Field(default=False, description="Whether an agent is currently running")
    is_reviewing: bool = Field(default=False, description="Whether the review agent is running")
    is_blocked: bool = Field(
        default=False, description="Whether scheduler has blocked auto-start for this task"
    )
    blocked_reason: str | None = Field(
        default=None, description="Human-readable block reason when blocked"
    )
    blocked_by_task_ids: list[str] = Field(
        default_factory=list,
        description="Task IDs currently blocking this task from auto-start",
    )
    overlap_hints: list[str] = Field(
        default_factory=list,
        description="Conflict-hint tokens used by scheduler for blocking decisions",
    )
    blocked_at: str | None = Field(
        default=None,
        description="ISO timestamp when the task entered blocked runtime state",
    )
    is_pending: bool = Field(
        default=False,
        description="Whether scheduler accepted start but task is pending admission",
    )
    pending_reason: str | None = Field(
        default=None,
        description="Human-readable pending reason when task is queued",
    )
    pending_at: str | None = Field(
        default=None,
        description="ISO timestamp when the task entered pending queue",
    )


class TaskContext(BaseModel):
    """Full context for working on a task. Returned by task_get(mode=context)."""

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
    runtime: TaskRuntimeState | None = Field(
        default=None,
        description="Live runtime metadata for scheduling and coordination",
    )


class TaskSummary(BaseModel):
    """Brief task summary for listings and coordination."""

    task_id: str = Field(description="Unique task identifier")
    title: str = Field(description="Task title")
    status: str | None = Field(default=None, description="Current task status")
    description: str | None = Field(default=None, description="Task description")
    scratchpad: str | None = Field(default=None, description="Agent notes")
    acceptance_criteria: list[str] | None = Field(default=None, description="Acceptance criteria")
    runtime: TaskRuntimeState | None = Field(
        default=None, description="Live runtime metadata when available"
    )


class TaskDetails(BaseModel):
    """Detailed task information. Returned by task_get."""

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
    runtime: TaskRuntimeState | None = Field(
        default=None, description="Live runtime metadata when available"
    )


class TaskWaitResponse(BaseModel):
    """Response from task_wait long-poll tool."""

    changed: bool = Field(description="Whether task status changed before timeout")
    timed_out: bool = Field(description="Whether the wait timed out without status change")
    task_id: str = Field(description="ID of the watched task")
    previous_status: str | None = Field(
        default=None, description="Task status at the start of the wait"
    )
    current_status: str | None = Field(
        default=None, description="Task status at the end of the wait"
    )
    changed_at: str | None = Field(
        default=None, description="ISO timestamp of the status change event"
    )
    task: dict[str, object] | None = Field(
        default=None, description="Compact task snapshot (no large logs/scratchpads)"
    )
    code: str | None = Field(default=None, description="Machine-readable status code")
    message: str | None = Field(default=None, description="Human-readable status message")


class PlanProposalResponse(MutatingResponse):
    """Response from plan_submit tool."""

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


class TaskListResponse(BaseModel):
    """Response from task_list tool."""

    tasks: list[TaskSummary] = Field(default_factory=list, description="List of tasks")
    count: int = Field(default=0, description="Total number of tasks returned")


class TaskLogsResponse(RecoveryResponse):
    """Response from task_logs tool."""

    task_id: str = Field(description="ID of the task")
    logs: list[AgentLogEntry] = Field(default_factory=list, description="Ordered log entries")
    count: int = Field(default=0, description="Number of logs returned")
    total_runs: int = Field(default=0, description="Total runs available for this task")
    returned_runs: int = Field(default=0, description="Number of runs included in this page")
    offset: int = Field(default=0, description="Page offset used for this response")
    limit: int = Field(default=0, description="Page limit used for this response")
    has_more: bool = Field(default=False, description="Whether additional runs are available")
    next_offset: int | None = Field(default=None, description="Offset for the next page")
    truncated: bool = Field(
        default=False,
        description="Whether log content was reduced for transport safety",
    )


class TaskCreateResponse(TaskScopedMutatingResponse):
    """Response from task_create tool."""

    title: str = Field(description="Task title")
    status: str = Field(description="Initial status (usually 'backlog')")


class JobResponse(JobScopedResponse):
    """Response from job_start, job_poll, and job_cancel tools."""

    action: str | None = Field(default=None, description="Submitted job action")
    status: str | None = Field(default=None, description="Current job status")
    timed_out: bool | None = Field(
        default=None,
        description="Whether job_poll(wait=true) returned before terminal status due to timeout",
    )
    timeout_metadata: dict[str, object] | None = Field(
        default=None,
        description="Structured timeout metadata from core when available",
    )
    created_at: str | None = Field(default=None, description="Job creation timestamp")
    updated_at: str | None = Field(default=None, description="Job last update timestamp")
    result: dict[str, object] | None = Field(
        default=None,
        description="Terminal action result payload when available",
    )
    runtime: TaskRuntimeState | None = Field(
        default=None,
        description="Runtime metadata extracted from result payload when available",
    )
    current_task_type: str | None = Field(
        default=None,
        description="Current task execution mode when relevant (AUTO or PAIR)",
    )


class JobEvent(BaseModel):
    """A single event emitted during asynchronous job execution."""

    job_id: str | None = Field(default=None, description="Core job identifier")
    task_id: str | None = Field(default=None, description="Associated task ID")
    status: str | None = Field(default=None, description="Job status at the time of this event")
    timestamp: str | None = Field(default=None, description="ISO timestamp for this event")
    message: str | None = Field(default=None, description="Human-readable event summary")
    code: str | None = Field(
        default=None,
        description="Machine-readable event code when available",
    )


class JobEventsResponse(JobScopedResponse):
    """Response from job_poll(events=true) tool."""

    events: list[JobEvent] = Field(
        default_factory=list,
        description="Ordered list of job events",
    )
    total_events: int = Field(default=0, description="Total events available for this job")
    returned_events: int = Field(default=0, description="Number of events in this page")
    offset: int = Field(default=0, description="Page offset used for this response")
    limit: int = Field(default=0, description="Page limit used for this response")
    has_more: bool = Field(default=False, description="Whether additional events are available")
    next_offset: int | None = Field(default=None, description="Offset for the next page")


class TaskDeleteResponse(TaskScopedMutatingResponse):
    """Response from task_delete tool."""


class ProjectInfo(BaseModel):
    """Summary of a project."""

    project_id: str = Field(description="Unique project identifier")
    name: str = Field(description="Project name")
    description: str | None = Field(default=None, description="Project description")


class ProjectListResponse(BaseModel):
    """Response from project_list tool."""

    projects: list[ProjectInfo] = Field(default_factory=list, description="List of projects")
    count: int = Field(default=0, description="Total number of projects returned")


class ProjectOpenResponse(MutatingResponse):
    """Response from project_open tool."""

    project_id: str = Field(description="ID of the opened project")
    name: str = Field(description="Project name")


class RepoListItem(BaseModel):
    """Summary of a repository in a project."""

    repo_id: str = Field(description="Unique repository identifier")
    name: str = Field(description="Repository name")
    display_name: str | None = Field(default=None, description="Human-readable display name")
    path: str = Field(description="Path to the repository")


class RepoListResponse(BaseModel):
    """Response from repo_list tool."""

    repos: list[RepoListItem] = Field(default_factory=list, description="List of repositories")
    count: int = Field(default=0, description="Total number of repos returned")


class ReviewActionResponse(TaskScopedMutatingResponse):
    """Response from the review tool (approve, reject, merge, rebase actions)."""


class AuditEvent(BaseModel):
    """A single audit event entry."""

    event_id: str | None = Field(default=None, description="Unique event identifier")
    occurred_at: str | None = Field(default=None, description="ISO timestamp of the event")
    actor_type: str | None = Field(default=None, description="Type of actor (user, agent, system)")
    actor_id: str | None = Field(default=None, description="Identifier of the actor")
    capability: str | None = Field(default=None, description="Capability that produced the event")
    command_name: str | None = Field(default=None, description="Command that was executed")
    success: bool | None = Field(default=None, description="Whether the command succeeded")


class AuditTailResponse(BaseModel):
    """Response from audit_list tool."""

    events: list[AuditEvent] = Field(default_factory=list, description="List of audit events")
    count: int = Field(default=0, description="Total number of events returned")


class InstrumentationSnapshotResponse(BaseModel):
    """Response from internal diagnostics instrumentation tool."""

    enabled: bool = Field(description="Whether instrumentation collection is enabled")
    log_events: bool = Field(description="Whether structured instrumentation logs are enabled")
    counters: dict[str, int] = Field(
        default_factory=dict,
        description="Counter aggregates keyed by metric name",
    )
    timings: dict[str, dict[str, float | int]] = Field(
        default_factory=dict,
        description="Timing aggregates keyed by metric name",
    )


class SettingsGetResponse(BaseModel):
    """Response from settings_get tool."""

    settings: dict[str, object] = Field(
        default_factory=dict,
        description="Allowlisted settings snapshot keyed by dotted paths",
    )


class SettingsUpdateResponse(MutatingResponse):
    """Response from settings_set tool."""

    updated: dict[str, object] = Field(
        default_factory=dict,
        description="Fields accepted and applied in this update request",
    )
    settings: dict[str, object] = Field(
        default_factory=dict,
        description="Settings snapshot after update (or current snapshot on failure)",
    )


# ---------------------------------------------------------------------------
# GitHub Plugin MCP Models (V1 contract)
# ---------------------------------------------------------------------------


class GitHubContractProbeResponse(MutatingResponse):
    """Response from kagan_github_contract_probe tool (V1 contract).

    This is a read-only scaffold verification probe that returns plugin
    metadata and echoes back any provided echo parameter.
    """

    plugin_id: str = Field(description="Plugin identifier (official.github)")
    contract_version: str = Field(description="Semantic version of the plugin contract")
    capability: str = Field(description="Capability name (kagan_github)")
    method: str = Field(description="Method name (contract_probe)")
    canonical_methods: list[str] = Field(
        default_factory=list,
        description="Canonical plugin capability methods (not limited to MCP V1 tools)",
    )
    canonical_scope: str = Field(
        default="plugin_capability",
        description=(
            "Scope of canonical_methods; plugin_capability indicates non-MCP operations may appear"
        ),
    )
    mcp_v1_tools: list[str] = Field(
        default_factory=list,
        description="MCP V1 tool names exposed by this server profile",
    )
    echo: str | None = Field(
        default=None,
        description="Echoed value from request for round-trip verification",
    )


class GitHubConnectionMetadata(BaseModel):
    """Connection metadata for a GitHub-connected repository."""

    full_name: str = Field(description="GitHub repository full name (owner/repo)")
    owner: str = Field(description="Repository owner")
    repo: str = Field(description="Repository name")
    default_branch: str | None = Field(default=None, description="Default branch name")
    visibility: str | None = Field(default=None, description="Repository visibility")
    connected_at: str | None = Field(default=None, description="ISO timestamp of connection")


class GitHubConnectRepoResponse(MutatingResponse):
    """Response from kagan_github_connect_repo tool (V1 contract).

    Connects a project repository to GitHub with preflight verification.
    Returns connection metadata on success or error with remediation hint.
    """

    connection: GitHubConnectionMetadata | None = Field(
        default=None,
        description="Connection metadata when successful",
    )


class GitHubSyncStats(BaseModel):
    """Statistics from a GitHub issue sync operation."""

    total: int = Field(default=0, description="Total issues processed")
    inserted: int = Field(default=0, description="New tasks created from issues")
    updated: int = Field(default=0, description="Existing tasks updated")
    reopened: int = Field(default=0, description="Tasks reopened from closed state")
    closed: int = Field(default=0, description="Tasks closed from open state")
    no_change: int = Field(default=0, description="Issues with no changes needed")
    errors: int = Field(default=0, description="Issues that failed to sync")


class GitHubSyncIssuesResponse(MutatingResponse):
    """Response from kagan_github_sync_issues tool (V1 contract).

    Synchronizes GitHub issues to Kagan task projections with incremental
    checkpoint support.
    """

    stats: GitHubSyncStats | None = Field(
        default=None,
        description="Sync statistics when successful",
    )
