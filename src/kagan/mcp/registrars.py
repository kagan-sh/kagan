"""MCP tool registrar — shared, task, job, session, and admin tools."""

# ruff: noqa: UP040
from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Final, Literal, TypeAlias

from mcp.server.fastmcp import Context
from mcp.server.session import ServerSession

from kagan.core.agents import planner as planner_models
from kagan.core.commands.job_action_executor import SUPPORTED_JOB_ACTIONS
from kagan.core.models.enums import TaskStatus, TaskType
from kagan.core.security import (
    AuditMethod,
    CapabilityProfile,
    DiagnosticsMethod,
    JobsMethod,
    PlanMethod,
    ProjectsMethod,
    ProtocolCapability,
    ReviewMethod,
    SessionsMethod,
    SettingsMethod,
    TasksMethod,
    protocol_call,
)
from kagan.mcp.models import (
    AgentLogEntry,
    AuditEvent,
    AuditTailResponse,
    InstrumentationSnapshotResponse,
    JobActionsResponse,
    JobEvent,
    JobEventsResponse,
    JobResponse,
    LinkedTask,
    PlanProposalResponse,
    ProjectCreateResponse,
    ProjectInfo,
    ProjectListResponse,
    ProjectOpenResponse,
    RepoInfo,
    RepoListItem,
    RepoListResponse,
    ReviewActionResponse,
    ReviewResponse,
    ScratchpadUpdateResponse,
    SessionCreateResponse,
    SessionExistsResponse,
    SessionKillResponse,
    SettingsGetResponse,
    SettingsUpdateResponse,
    TaskContext,
    TaskCreateResponse,
    TaskDeleteResponse,
    TaskDetails,
    TaskListResponse,
    TaskMoveResponse,
    TaskRuntimeState,
    TaskSummary,
    TaskUpdateResponse,
    TaskWaitResponse,
)

if TYPE_CHECKING:
    from collections.abc import Callable

    from mcp.server.fastmcp import FastMCP
    from mcp.types import ToolAnnotations

    from kagan.mcp.tools import CoreClientBridge

# ---------------------------------------------------------------------------
# Type aliases
# ---------------------------------------------------------------------------

MCPContext: TypeAlias = Context[ServerSession, Any]

WorkflowStatusInput: TypeAlias = Literal[
    "BACKLOG",
    "IN_PROGRESS",
    "REVIEW",
    "DONE",
    "backlog",
    "in_progress",
    "review",
    "done",
]
TaskTypeInput: TypeAlias = Literal["AUTO", "PAIR", "auto", "pair"]
TaskStatusInput: TypeAlias = Literal[
    "BACKLOG",
    "IN_PROGRESS",
    "REVIEW",
    "DONE",
    "AUTO",
    "PAIR",
    "backlog",
    "in_progress",
    "review",
    "done",
    "auto",
    "pair",
]
TaskPriorityInput: TypeAlias = Literal[
    "LOW",
    "MED",
    "MEDIUM",
    "HIGH",
    "low",
    "med",
    "medium",
    "high",
]
JobActionInput: TypeAlias = Literal["start_agent", "stop_agent"]
TerminalBackendInput: TypeAlias = Literal["tmux", "vscode", "cursor"]
ReviewActionInput: TypeAlias = Literal["approve", "reject", "merge", "rebase"]
RejectionActionInput: TypeAlias = Literal["reopen", "return", "in_progress", "backlog"]

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

TASK_TYPE_AUTO: Final[str] = TaskType.AUTO.value
TASK_TYPE_PAIR: Final[str] = TaskType.PAIR.value
TASK_TYPE_VALUES: Final[frozenset[str]] = frozenset({TASK_TYPE_AUTO, TASK_TYPE_PAIR})

TASK_CODE_STATUS_WAS_TASK_TYPE: Final[str] = "STATUS_WAS_TASK_TYPE"
TASK_CODE_TASK_TYPE_VALUE_IN_STATUS: Final[str] = "TASK_TYPE_VALUE_IN_STATUS"

STATUS_ERROR: Final[str] = "error"

JOB_NON_TERMINAL_STATUSES: Final[frozenset[str]] = frozenset({"queued", "running"})
JOB_TERMINAL_STATUSES: Final[frozenset[str]] = frozenset({"succeeded", "failed", "cancelled"})

JOB_CODE_UNSUPPORTED_ACTION: Final[str] = "UNSUPPORTED_ACTION"
JOB_CODE_JOB_TIMEOUT: Final[str] = "JOB_TIMEOUT"
JOB_CODE_TASK_TYPE_MISMATCH: Final[str] = "TASK_TYPE_MISMATCH"
JOB_CODE_START_BLOCKED: Final[str] = "START_BLOCKED"
JOB_CODE_START_PENDING: Final[str] = "START_PENDING"
JOB_CODE_NOT_RUNNING: Final[str] = "NOT_RUNNING"

TOOL_GET_TASK: Final[str] = "get_task"
TOOL_JOBS_LIST_ACTIONS: Final[str] = "jobs_list_actions"
TOOL_JOBS_WAIT: Final[str] = "jobs_wait"
TOOL_TASKS_UPDATE: Final[str] = "tasks_update"

# ---------------------------------------------------------------------------
# Protocol call constants
# ---------------------------------------------------------------------------

# Shared / read-only
_PLAN_PROPOSE = protocol_call(ProtocolCapability.PLAN, PlanMethod.PROPOSE)
_TASKS_GET = protocol_call(ProtocolCapability.TASKS, TasksMethod.GET)
_TASKS_SCRATCHPAD = protocol_call(ProtocolCapability.TASKS, TasksMethod.SCRATCHPAD)
_TASKS_LIST = protocol_call(ProtocolCapability.TASKS, TasksMethod.LIST)
_PROJECTS_LIST = protocol_call(ProtocolCapability.PROJECTS, ProjectsMethod.LIST)
_PROJECTS_REPOS = protocol_call(ProtocolCapability.PROJECTS, ProjectsMethod.REPOS)
_AUDIT_LIST = protocol_call(ProtocolCapability.AUDIT, AuditMethod.LIST)

# Task CRUD
_TASKS_WAIT = protocol_call(ProtocolCapability.TASKS, TasksMethod.WAIT)
_TASKS_UPDATE_SCRATCHPAD = protocol_call(ProtocolCapability.TASKS, TasksMethod.UPDATE_SCRATCHPAD)
_TASKS_CREATE = protocol_call(ProtocolCapability.TASKS, TasksMethod.CREATE)
_TASKS_UPDATE = protocol_call(ProtocolCapability.TASKS, TasksMethod.UPDATE)
_TASKS_MOVE = protocol_call(ProtocolCapability.TASKS, TasksMethod.MOVE)
_TASKS_DELETE = protocol_call(ProtocolCapability.TASKS, TasksMethod.DELETE)
_PROJECTS_CREATE = protocol_call(ProtocolCapability.PROJECTS, ProjectsMethod.CREATE)
_PROJECTS_OPEN = protocol_call(ProtocolCapability.PROJECTS, ProjectsMethod.OPEN)

# Jobs
_JOBS_SUBMIT = protocol_call(ProtocolCapability.JOBS, JobsMethod.SUBMIT)
_JOBS_GET = protocol_call(ProtocolCapability.JOBS, JobsMethod.GET)
_JOBS_WAIT = protocol_call(ProtocolCapability.JOBS, JobsMethod.WAIT)
_JOBS_EVENTS = protocol_call(ProtocolCapability.JOBS, JobsMethod.EVENTS)
_JOBS_CANCEL = protocol_call(ProtocolCapability.JOBS, JobsMethod.CANCEL)
_SESSIONS_CREATE = protocol_call(ProtocolCapability.SESSIONS, SessionsMethod.CREATE)
_SESSIONS_EXISTS = protocol_call(ProtocolCapability.SESSIONS, SessionsMethod.EXISTS)
_SESSIONS_KILL = protocol_call(ProtocolCapability.SESSIONS, SessionsMethod.KILL)

# Admin / review
_REVIEW_REQUEST = protocol_call(ProtocolCapability.REVIEW, ReviewMethod.REQUEST)
_SETTINGS_GET = protocol_call(ProtocolCapability.SETTINGS, SettingsMethod.GET)
_SETTINGS_UPDATE = protocol_call(ProtocolCapability.SETTINGS, SettingsMethod.UPDATE)
_DIAGNOSTICS_INSTRUMENTATION = protocol_call(
    ProtocolCapability.DIAGNOSTICS, DiagnosticsMethod.INSTRUMENTATION
)
_REVIEW_APPROVE = protocol_call(ProtocolCapability.REVIEW, ReviewMethod.APPROVE)
_REVIEW_REJECT = protocol_call(ProtocolCapability.REVIEW, ReviewMethod.REJECT)
_REVIEW_MERGE = protocol_call(ProtocolCapability.REVIEW, ReviewMethod.MERGE)
_REVIEW_REBASE = protocol_call(ProtocolCapability.REVIEW, ReviewMethod.REBASE)

# ---------------------------------------------------------------------------
# Registration context dataclasses
# ---------------------------------------------------------------------------


@dataclass(frozen=True, slots=True)
class ToolRegistrationContext:
    """Callbacks required by full-mode tool registration."""

    require_bridge: Callable[[MCPContext | None], CoreClientBridge]
    runtime_state_from_raw: Callable[[dict[str, Any] | None], TaskRuntimeState | None]
    normalize_status_task_type_inputs: Callable[..., tuple[str | None, str | None, str | None]]
    envelope_fields: Callable[..., Any]
    envelope_with_code_override: Callable[..., Any]
    envelope_status_fields: Callable[[Any], Any]
    envelope_recovery_fields: Callable[[Any], Any]
    project_settings_update_fields: Callable[[dict[str, object | None]], dict[str, object]]
    normalized_mode: Callable[[str | None], str | None]
    derive_job_get_recovery: Callable[
        ...,
        tuple[str | None, dict[str, object] | None, str | None],
    ]
    str_or_none: Callable[[object], str | None]
    dict_or_none: Callable[[object], dict[str, object] | None]
    is_allowed: Callable[[str, str, str], bool]


@dataclass(frozen=True, slots=True)
class SharedToolRegistrationContext:
    """Callbacks required by shared tool registration."""

    require_bridge: Callable[[MCPContext | None], CoreClientBridge]
    runtime_state_from_raw: Callable[[dict[str, Any] | None], TaskRuntimeState | None]


# ---------------------------------------------------------------------------
# Shared / read-only tool registration
# ---------------------------------------------------------------------------


def register_shared_tools(
    mcp: FastMCP,
    *,
    allows_all: Callable[..., bool],
    effective_profile: str,
    helpers: SharedToolRegistrationContext,
    read_only_annotation: ToolAnnotations,
    mutating_annotation: ToolAnnotations,
) -> None:
    """Register planner/read-only/shared MCP tools."""
    _require_bridge = helpers.require_bridge
    _runtime_state_from_raw = helpers.runtime_state_from_raw
    _READ_ONLY = read_only_annotation
    _MUTATING = mutating_annotation
    if allows_all(_PLAN_PROPOSE) and effective_profile == str(CapabilityProfile.PLANNER):

        @mcp.tool(annotations=_MUTATING)
        async def propose_plan(
            tasks: list[planner_models.ProposedTask],
            todos: list[planner_models.ProposedTodo] | None = None,
            ctx: MCPContext | None = None,
        ) -> PlanProposalResponse:
            """Submit a structured plan proposal for planner mode.

            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            if ctx:
                await ctx.info(f"Receiving plan proposal with {len(tasks)} tasks")

            proposal = planner_models.PlanProposal.model_validate(
                {"tasks": tasks, "todos": todos or []}
            )

            if ctx:
                await ctx.debug(
                    f"Plan validated: {len(proposal.tasks)} tasks, {len(proposal.todos)} todos"
                )

            return PlanProposalResponse(
                success=True,
                status="received",
                message="Plan proposal received",
                task_count=len(proposal.tasks),
                todo_count=len(proposal.todos),
                tasks=[task.model_dump(mode="json") for task in proposal.tasks],
                todos=[todo.model_dump(mode="json") for todo in proposal.todos],
            )

    if allows_all(_TASKS_GET, _TASKS_SCRATCHPAD):

        @mcp.tool(annotations=_READ_ONLY)
        async def get_task(
            task_id: str,
            include_scratchpad: bool | None = None,
            include_logs: bool | None = None,
            include_review: bool | None = None,
            mode: str = "summary",
            ctx: MCPContext | None = None,
        ) -> TaskDetails:
            """Get task details with optional extended context.

            Args:
                task_id: The task to retrieve
                include_scratchpad: Include agent notes
                include_logs: Include execution logs from previous runs
                include_review: Include review feedback
                mode: 'summary' or 'full'
            """
            if ctx:
                await ctx.info(f"Fetching task details for {task_id}")

            bridge = _require_bridge(ctx)

            raw = await bridge.get_task(
                task_id,
                include_scratchpad=include_scratchpad,
                include_logs=include_logs,
                include_review=include_review,
                mode=mode,
            )

            logs = None
            if include_logs:
                raw_logs = raw.get("logs") or []
                logs = [
                    AgentLogEntry(
                        run=log["run"],
                        content=log["content"],
                        created_at=log["created_at"],
                    )
                    for log in raw_logs
                ]

            if ctx:
                await ctx.debug(f"Task retrieved: status={raw['status']}")

            return TaskDetails(
                task_id=raw["task_id"],
                title=raw["title"],
                status=raw["status"],
                description=raw.get("description"),
                acceptance_criteria=raw.get("acceptance_criteria"),
                scratchpad=raw.get("scratchpad"),
                review_feedback=raw.get("review_feedback"),
                logs=logs,
                runtime=_runtime_state_from_raw(raw.get("runtime")),
            )

    if allows_all(_TASKS_LIST):

        @mcp.tool(annotations=_READ_ONLY)
        async def tasks_list(
            project_id: str | None = None,
            filter: str | None = None,
            exclude_task_ids: list[str] | None = None,
            include_scratchpad: bool = False,
            ctx: MCPContext | None = None,
        ) -> TaskListResponse:
            """List tasks with optional coordination filters."""
            bridge = _require_bridge(ctx)

            raw = await bridge.list_tasks(
                project_id=project_id,
                filter=filter,
                exclude_task_ids=exclude_task_ids,
                include_scratchpad=include_scratchpad,
            )
            tasks = [
                TaskSummary(
                    task_id=t["id"],
                    title=t["title"],
                    status=t.get("status"),
                    description=t.get("description"),
                    scratchpad=t.get("scratchpad"),
                    acceptance_criteria=t.get("acceptance_criteria"),
                    runtime=_runtime_state_from_raw(t.get("runtime")),
                )
                for t in raw.get("tasks", [])
            ]
            return TaskListResponse(tasks=tasks, count=raw.get("count", len(tasks)))

    if allows_all(_TASKS_WAIT):

        @mcp.tool(annotations=_READ_ONLY)
        async def tasks_wait(
            task_id: str,
            timeout_seconds: float | str | None = None,
            wait_for_status: list[str] | str | None = None,
            from_updated_at: str | None = None,
            ctx: MCPContext | None = None,
        ) -> TaskWaitResponse:
            """Wait for task status change or timeout (long-poll).

            Blocks until the target task changes status, reaches a specific
            status from wait_for_status, or the timeout elapses.

            Use from_updated_at for race-safe resumption after reconnect.

            Args:
                task_id: The task to watch.
                timeout_seconds: Max wait time (default: server configured, max: server configured).
                wait_for_status: Target statuses (list, CSV string, or JSON list string).
                from_updated_at: ISO timestamp cursor for race-safe resume (no lost wakeups).
            """
            bridge = _require_bridge(ctx)
            raw = await bridge.wait_task(
                task_id,
                timeout_seconds=timeout_seconds,
                wait_for_status=wait_for_status,
                from_updated_at=from_updated_at,
            )
            return TaskWaitResponse(
                changed=bool(raw.get("changed", False)),
                timed_out=bool(raw.get("timed_out", False)),
                task_id=raw.get("task_id", task_id),
                previous_status=raw.get("previous_status"),
                current_status=raw.get("current_status"),
                changed_at=raw.get("changed_at"),
                task=raw.get("task"),
                code=raw.get("code"),
                message=raw.get("message"),
            )

    if allows_all(_PROJECTS_LIST):

        @mcp.tool(annotations=_READ_ONLY)
        async def projects_list(
            limit: int = 10,
            ctx: MCPContext | None = None,
        ) -> ProjectListResponse:
            """List recent projects."""
            bridge = _require_bridge(ctx)

            raw = await bridge.list_projects(limit=limit)
            projects = [
                ProjectInfo(
                    project_id=p["id"],
                    name=p["name"],
                    description=p.get("description"),
                )
                for p in raw.get("projects", [])
            ]
            return ProjectListResponse(projects=projects, count=raw.get("count", len(projects)))

    if allows_all(_PROJECTS_REPOS):

        @mcp.tool(annotations=_READ_ONLY)
        async def repos_list(
            project_id: str,
            ctx: MCPContext | None = None,
        ) -> RepoListResponse:
            """List repos for a project."""
            bridge = _require_bridge(ctx)

            raw = await bridge.list_repos(project_id)
            repos = [
                RepoListItem(
                    repo_id=r["id"],
                    name=r["name"],
                    display_name=r.get("display_name"),
                    path=str(r.get("path", "")),
                )
                for r in raw.get("repos", [])
            ]
            return RepoListResponse(repos=repos, count=raw.get("count", len(repos)))

    if allows_all(_AUDIT_LIST):

        @mcp.tool(annotations=_READ_ONLY)
        async def audit_tail(
            capability: str | None = None,
            limit: int = 50,
            ctx: MCPContext | None = None,
        ) -> AuditTailResponse:
            """List recent audit events."""
            bridge = _require_bridge(ctx)

            raw = await bridge.tail_audit(capability=capability, limit=limit)
            events = [
                AuditEvent(
                    event_id=e.get("id"),
                    occurred_at=e.get("occurred_at"),
                    actor_type=e.get("actor_type"),
                    actor_id=e.get("actor_id"),
                    capability=e.get("capability"),
                    command_name=e.get("command_name"),
                    success=e.get("success"),
                )
                for e in raw.get("events", [])
            ]
            return AuditTailResponse(events=events, count=raw.get("count", len(events)))


# ---------------------------------------------------------------------------
# Task CRUD tool registration
# ---------------------------------------------------------------------------


def register_task_tools(
    mcp: FastMCP,
    *,
    allows_all: Callable[..., bool],
    helpers: ToolRegistrationContext,
    read_only_annotation: ToolAnnotations,
    mutating_annotation: ToolAnnotations,
    destructive_annotation: ToolAnnotations,
) -> None:
    """Register task CRUD MCP tools."""
    _require_bridge = helpers.require_bridge
    _runtime_state_from_raw = helpers.runtime_state_from_raw
    _normalize_status_task_type_inputs = helpers.normalize_status_task_type_inputs
    _envelope_fields = helpers.envelope_fields
    _envelope_with_code_override = helpers.envelope_with_code_override
    _envelope_recovery_fields = helpers.envelope_recovery_fields
    _normalized_mode = helpers.normalized_mode
    _READ_ONLY = read_only_annotation
    _MUTATING = mutating_annotation
    _DESTRUCTIVE = destructive_annotation

    if allows_all(_TASKS_GET, _TASKS_SCRATCHPAD):

        @mcp.tool(annotations=_READ_ONLY)
        async def get_context(
            task_id: str,
            ctx: MCPContext | None = None,
        ) -> TaskContext:
            """Get task context for AI tools.

            Returns comprehensive context including task details, workspace info,
            repository state, and linked tasks.
            """
            if ctx:
                await ctx.info(f"Fetching context for task {task_id}")
                await ctx.report_progress(0.1, 1.0, "Loading task details")

            bridge = _require_bridge(ctx)

            raw = await bridge.get_context(task_id)

            if ctx:
                await ctx.report_progress(0.5, 1.0, "Processing workspace info")

            # Convert raw repos to RepoInfo models
            repos = [
                RepoInfo(
                    repo_id=r["repo_id"],
                    name=r["name"],
                    path=r["path"],
                    worktree_path=r.get("worktree_path"),
                    target_branch=r.get("target_branch"),
                    has_changes=r.get("has_changes"),
                    diff_stats=r.get("diff_stats"),
                )
                for r in raw.get("repos", [])
            ]

            # Convert linked tasks
            linked_tasks = [
                LinkedTask(
                    task_id=lt["task_id"],
                    title=lt["title"],
                    status=lt["status"],
                    description=lt.get("description"),
                )
                for lt in raw.get("linked_tasks", [])
            ]

            if ctx:
                await ctx.report_progress(1.0, 1.0, "Context ready")
                await ctx.debug(
                    f"Context loaded: {len(repos)} repos, {len(linked_tasks)} linked tasks"
                )

            return TaskContext(
                task_id=raw["task_id"],
                title=raw["title"],
                description=raw.get("description"),
                acceptance_criteria=raw.get("acceptance_criteria"),
                scratchpad=raw.get("scratchpad"),
                workspace_id=raw.get("workspace_id"),
                workspace_branch=raw.get("workspace_branch"),
                workspace_path=raw.get("workspace_path"),
                working_dir=raw.get("working_dir"),
                repos=repos,
                repo_count=raw.get("repo_count", len(repos)),
                linked_tasks=linked_tasks,
                runtime=_runtime_state_from_raw(raw.get("runtime")),
            )

    if allows_all(_TASKS_UPDATE_SCRATCHPAD):

        @mcp.tool(annotations=_MUTATING)
        async def update_scratchpad(
            task_id: str,
            content: str,
            ctx: MCPContext | None = None,
        ) -> ScratchpadUpdateResponse:
            """Append to task scratchpad.

            Use to record progress, decisions, blockers, and notes during implementation.
            Content is appended to existing scratchpad.

            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            if ctx:
                await ctx.info(f"Updating scratchpad for task {task_id}")
                await ctx.debug(f"Content length: {len(content)} chars")

            bridge = _require_bridge(ctx)

            raw = await bridge.update_scratchpad(task_id, content)
            envelope = _envelope_fields(
                raw,
                default_success=True,
                default_message="Scratchpad updated",
            )

            if ctx:
                if raw.get("success"):
                    await ctx.debug("Scratchpad updated successfully")
                else:
                    await ctx.warning(str(raw.get("message", "Scratchpad update failed")))

            return ScratchpadUpdateResponse(
                task_id=task_id,
                **_envelope_recovery_fields(envelope),
            )

    if allows_all(_TASKS_CREATE):

        @mcp.tool(annotations=_MUTATING)
        async def tasks_create(
            title: str,
            description: str = "",
            project_id: str | None = None,
            status: TaskStatusInput | None = None,
            priority: TaskPriorityInput | None = None,
            task_type: TaskTypeInput | None = None,
            terminal_backend: TerminalBackendInput | None = None,
            agent_backend: str | None = None,
            parent_id: str | None = None,
            base_branch: str | None = None,
            acceptance_criteria: list[str] | str | None = None,
            created_by: str | None = None,
            ctx: MCPContext | None = None,
        ) -> TaskCreateResponse:
            """Create a new task.

            Args:
                status: Kanban workflow state.
                task_type: Execution mode used by automation/session flows (AUTO or PAIR).
                priority: Priority lane (LOW/MEDIUM/HIGH).

            Use this for new tasks. Use tasks_update to modify existing tasks.
            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            bridge = _require_bridge(ctx)
            normalized_status, normalized_task_type, normalization_message = (
                _normalize_status_task_type_inputs(status=status, task_type=task_type)
            )

            raw = await bridge.create_task(
                title=title,
                description=description,
                project_id=project_id,
                status=normalized_status,
                priority=priority,
                task_type=normalized_task_type,
                terminal_backend=terminal_backend,
                agent_backend=agent_backend,
                parent_id=parent_id,
                base_branch=base_branch,
                acceptance_criteria=acceptance_criteria,
                created_by=created_by,
            )
            envelope = _envelope_with_code_override(
                raw,
                default_success=True,
                default_message=normalization_message,
                fallback_code=TASK_CODE_STATUS_WAS_TASK_TYPE if normalization_message else None,
            )
            return TaskCreateResponse(
                task_id=raw["task_id"],
                title=raw.get("title", title),
                status=raw.get("status", TaskStatus.BACKLOG.value),
                **_envelope_recovery_fields(envelope),
            )

    if allows_all(_TASKS_UPDATE):

        @mcp.tool(annotations=_MUTATING)
        async def tasks_update(
            task_id: str,
            title: str | None = None,
            description: str | None = None,
            priority: TaskPriorityInput | None = None,
            task_type: TaskTypeInput | None = None,
            status: TaskStatusInput | None = None,
            terminal_backend: TerminalBackendInput | None = None,
            agent_backend: str | None = None,
            parent_id: str | None = None,
            project_id: str | None = None,
            base_branch: str | None = None,
            acceptance_criteria: list[str] | str | None = None,
            ctx: MCPContext | None = None,
        ) -> TaskUpdateResponse:
            """Update task fields.

            Args:
                status: Kanban workflow state.
                task_type: Execution mode used by automation/session flows (AUTO or PAIR).

            Use this to change execution mode before agent operations.
            Example recovery: tasks_update(task_id=..., task_type="AUTO").
            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            bridge = _require_bridge(ctx)
            normalized_status, normalized_task_type, normalization_message = (
                _normalize_status_task_type_inputs(status=status, task_type=task_type)
            )

            fields: dict[str, object] = {}
            if title is not None:
                fields["title"] = title
            if description is not None:
                fields["description"] = description
            if priority is not None:
                fields["priority"] = priority
            if normalized_task_type is not None:
                fields["task_type"] = normalized_task_type
            if normalized_status is not None:
                fields["status"] = normalized_status
            if terminal_backend is not None:
                fields["terminal_backend"] = terminal_backend
            if agent_backend is not None:
                fields["agent_backend"] = agent_backend
            if parent_id is not None:
                fields["parent_id"] = parent_id
            if project_id is not None:
                fields["project_id"] = project_id
            if base_branch is not None:
                fields["base_branch"] = base_branch
            if acceptance_criteria is not None:
                fields["acceptance_criteria"] = acceptance_criteria

            raw = await bridge.update_task(task_id, **fields)
            envelope = _envelope_with_code_override(
                raw,
                default_success=True,
                default_message=normalization_message,
                fallback_code=TASK_CODE_STATUS_WAS_TASK_TYPE if normalization_message else None,
            )
            return TaskUpdateResponse(
                task_id=raw.get("task_id", task_id),
                **_envelope_recovery_fields(envelope),
                current_task_type=raw.get("current_task_type"),
            )

    if allows_all(_TASKS_MOVE):

        @mcp.tool(annotations=_MUTATING)
        async def tasks_move(
            task_id: str,
            status: WorkflowStatusInput,
            ctx: MCPContext | None = None,
        ) -> TaskMoveResponse:
            """Move task to a specific Kanban column.

            Use this for status transitions only.
            Do not use this to change execution mode; use tasks_update(task_type=...).
            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            bridge = _require_bridge(ctx)
            normalized_mode = _normalized_mode(str(status))
            if normalized_mode is not None:
                return TaskMoveResponse(
                    success=False,
                    task_id=task_id,
                    new_status=None,
                    message=(
                        f"Invalid status value {status!r}. "
                        "AUTO/PAIR are task_type values, not status values."
                    ),
                    code=TASK_CODE_TASK_TYPE_VALUE_IN_STATUS,
                    hint=(
                        "Call tasks_update(task_id=..., task_type='AUTO'|'PAIR') "
                        "to set execution mode."
                    ),
                    next_tool=TOOL_TASKS_UPDATE,
                    next_arguments={"task_id": task_id, "task_type": normalized_mode},
                )

            raw = await bridge.move_task(task_id, status)
            envelope = _envelope_fields(raw, default_success=True)
            return TaskMoveResponse(
                task_id=raw.get("task_id", task_id),
                new_status=raw.get("new_status"),
                **_envelope_recovery_fields(envelope),
            )

    if allows_all(_TASKS_DELETE):

        @mcp.tool(annotations=_DESTRUCTIVE)
        async def tasks_delete(
            task_id: str,
            ctx: MCPContext | None = None,
        ) -> TaskDeleteResponse:
            """Delete a task.

            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            bridge = _require_bridge(ctx)

            raw = await bridge.delete_task(task_id)
            envelope = _envelope_fields(raw, default_success=False)
            return TaskDeleteResponse(
                task_id=raw.get("task_id", task_id),
                **_envelope_recovery_fields(envelope),
            )

    if allows_all(_PROJECTS_CREATE):

        @mcp.tool(annotations=_MUTATING)
        async def projects_create(
            name: str,
            description: str = "",
            repo_paths: list[str] | None = None,
            ctx: MCPContext | None = None,
        ) -> ProjectCreateResponse:
            """Create a new project with optional repositories."""
            bridge = _require_bridge(ctx)

            raw = await bridge.create_project(
                name=name,
                description=description,
                repo_paths=repo_paths,
            )
            envelope = _envelope_fields(raw, default_success=True)
            return ProjectCreateResponse(
                project_id=raw.get("project_id", ""),
                name=raw.get("name", name),
                description=raw.get("description", description),
                repo_count=raw.get("repo_count", 0),
                **_envelope_recovery_fields(envelope),
            )

    if allows_all(_PROJECTS_OPEN):

        @mcp.tool(annotations=_MUTATING)
        async def projects_open(
            project_id: str,
            ctx: MCPContext | None = None,
        ) -> ProjectOpenResponse:
            """Open/switch to a project.

            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            bridge = _require_bridge(ctx)

            raw = await bridge.open_project(project_id)
            envelope = _envelope_fields(raw, default_success=True)
            return ProjectOpenResponse(
                project_id=raw.get("project_id", project_id),
                name=raw.get("name", ""),
                **_envelope_recovery_fields(envelope),
            )


# ---------------------------------------------------------------------------
# Job tool helpers and registration
# ---------------------------------------------------------------------------


def _job_timed_out(
    raw: dict[str, object],
    *,
    result: dict[str, object] | None = None,
) -> bool | None:
    for source in (raw,) if result is None else (raw, result):
        val = source.get("timed_out")
        if isinstance(val, bool):
            return val
    return None


def _int_or_none(v: object) -> int | None:
    return v if isinstance(v, int) and not isinstance(v, bool) else None


def _job_timeout_metadata(
    raw: dict[str, object],
    *,
    result: dict[str, object] | None = None,
    dict_or_none: Callable[[object], dict[str, object] | None],
) -> dict[str, object] | None:
    timeout_payload = dict_or_none(raw.get("timeout"))
    if timeout_payload is not None:
        return timeout_payload
    if result is not None:
        result_timeout_payload = dict_or_none(result.get("timeout"))
        if result_timeout_payload is not None:
            return result_timeout_payload
    timeout_fields: dict[str, object] = {}
    for source in (raw, result or {}):
        for key, value in source.items():
            if key.startswith("timeout_"):
                timeout_fields[key] = value
    return timeout_fields or None


def register_job_tools(
    mcp: FastMCP,
    *,
    allows_all: Callable[..., bool],
    helpers: ToolRegistrationContext,
    read_only_annotation: ToolAnnotations,
    mutating_annotation: ToolAnnotations,
) -> None:
    """Register asynchronous job MCP tools."""
    _require_bridge = helpers.require_bridge
    _runtime_state_from_raw = helpers.runtime_state_from_raw
    _envelope_fields = helpers.envelope_fields
    _envelope_with_code_override = helpers.envelope_with_code_override
    _derive_job_get_recovery = helpers.derive_job_get_recovery
    _str_or_none = helpers.str_or_none
    _dict_or_none = helpers.dict_or_none
    _READ_ONLY = read_only_annotation
    _MUTATING = mutating_annotation

    def _extract_job_payload(
        raw: dict[str, object],
    ) -> tuple[dict[str, object] | None, dict[str, object] | None, str | None]:
        result = _dict_or_none(raw.get("result"))
        runtime_raw = _dict_or_none(raw.get("runtime"))
        if runtime_raw is None and result is not None:
            runtime_raw = _dict_or_none(result.get("runtime"))
        current_task_type = _str_or_none(raw.get("current_task_type"))
        if current_task_type is None and result is not None:
            current_task_type = _str_or_none(result.get("current_task_type"))
        return result, runtime_raw, current_task_type

    def _build_job_response(
        *,
        raw: dict[str, object],
        envelope: Any,
        job_id: str,
        task_id: str,
        action: str | None,
        result: dict[str, object] | None,
        runtime_raw: dict[str, object] | None,
        current_task_type: str | None,
        message: str | None = None,
        hint: str | None = None,
        next_tool: str | None = None,
        next_arguments: dict[str, object] | None = None,
    ) -> JobResponse:
        return JobResponse(
            success=envelope.success,
            message=envelope.message if message is None else message,
            code=envelope.code,
            hint=hint,
            next_tool=next_tool,
            next_arguments=next_arguments,
            job_id=job_id,
            task_id=task_id,
            action=action,
            status=_str_or_none(raw.get("status")),
            timed_out=_job_timed_out(raw, result=result),
            timeout_metadata=_job_timeout_metadata(raw, result=result, dict_or_none=_dict_or_none),
            created_at=_str_or_none(raw.get("created_at")),
            updated_at=_str_or_none(raw.get("updated_at")),
            result=result,
            runtime=_runtime_state_from_raw(runtime_raw),
            current_task_type=current_task_type,
        )

    def _build_job_poll_response(
        *,
        raw: dict[str, object],
        job_id: str,
        task_id: str,
    ) -> JobResponse:
        result, runtime_raw, current_task_type = _extract_job_payload(raw)
        result_code = _str_or_none(result.get("code")) if result is not None else None
        envelope = _envelope_with_code_override(
            raw,
            default_success=False,
            default_message=None,
            fallback_code=result_code,
        )
        message = envelope.message
        if message is None and result is not None:
            message = _str_or_none(result.get("message"))
        timed_out = _job_timed_out(raw, result=result)
        next_tool = _str_or_none(raw.get("next_tool"))
        next_arguments = _dict_or_none(raw.get("next_arguments"))
        hint = _str_or_none(raw.get("hint"))
        if next_tool is None:
            derived_tool, derived_args, derived_hint = _derive_job_get_recovery(
                job_id=job_id,
                task_id=task_id,
                status=_str_or_none(raw.get("status")),
                code=envelope.code,
                timed_out=timed_out,
                runtime=runtime_raw,
            )
            next_tool = derived_tool
            next_arguments = derived_args
            if hint is None:
                hint = derived_hint
        return _build_job_response(
            raw=raw,
            envelope=envelope,
            message=message,
            hint=hint,
            next_tool=next_tool,
            next_arguments=next_arguments,
            job_id=_str_or_none(raw.get("job_id")) or job_id,
            task_id=_str_or_none(raw.get("task_id")) or task_id,
            action=_str_or_none(raw.get("action")),
            result=result,
            runtime_raw=runtime_raw,
            current_task_type=current_task_type,
        )

    if allows_all(_JOBS_SUBMIT):

        @mcp.tool(annotations=_READ_ONLY)
        async def jobs_list_actions(ctx: MCPContext | None = None) -> JobActionsResponse:
            """List valid actions accepted by jobs_submit."""
            if ctx:
                await ctx.debug("Returning supported jobs_submit actions")
            return JobActionsResponse(actions=sorted(SUPPORTED_JOB_ACTIONS))

        @mcp.tool(annotations=_MUTATING)
        async def jobs_submit(
            task_id: str,
            action: JobActionInput,
            arguments: dict[str, object] | None = None,
            ctx: MCPContext | None = None,
        ) -> JobResponse:
            """Submit an asynchronous core job.

            Use jobs_list_actions to discover valid action names.
            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            bridge = _require_bridge(ctx)
            raw = await bridge.submit_job(task_id=task_id, action=action, arguments=arguments)
            envelope = _envelope_fields(raw, default_success=False, default_message="")
            job_id = _str_or_none(raw.get("job_id")) or ""
            returned_task_id = _str_or_none(raw.get("task_id")) or task_id
            result, runtime_raw, current_task_type = _extract_job_payload(raw)
            next_tool = _str_or_none(raw.get("next_tool"))
            next_arguments: dict[str, object] | None = _dict_or_none(raw.get("next_arguments"))
            hint = _str_or_none(raw.get("hint"))
            if (
                not envelope.success
                and envelope.code == JOB_CODE_UNSUPPORTED_ACTION
                and next_tool is None
            ):
                next_tool = TOOL_JOBS_LIST_ACTIONS
                next_arguments = {}
                if hint is None:
                    hint = "Call jobs_list_actions and retry jobs_submit with a listed action."
            if envelope.success and next_tool is None and job_id:
                next_tool = TOOL_JOBS_WAIT
                next_arguments = {
                    "job_id": job_id,
                    "task_id": returned_task_id,
                    "timeout_seconds": 1.5,
                }
                if hint is None:
                    hint = (
                        "Call jobs_wait to confirm the spawn succeeded, then call "
                        "tasks_wait(task_id, wait_for_status=['REVIEW','DONE'], "
                        "timeout_seconds=900) to long-poll until the agent completes."
                    )
            return _build_job_response(
                raw=raw,
                envelope=envelope,
                hint=hint,
                next_tool=next_tool,
                next_arguments=next_arguments,
                job_id=job_id,
                task_id=returned_task_id,
                action=_str_or_none(raw.get("action")) or str(action),
                result=result,
                runtime_raw=runtime_raw,
                current_task_type=current_task_type,
            )

    if allows_all(_JOBS_GET):

        @mcp.tool(annotations=_READ_ONLY)
        async def jobs_get(
            job_id: str,
            task_id: str,
            ctx: MCPContext | None = None,
        ) -> JobResponse:
            """Get job details and latest terminal result when available.

            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            bridge = _require_bridge(ctx)
            raw = await bridge.get_job(job_id=job_id, task_id=task_id)
            return _build_job_poll_response(raw=raw, job_id=job_id, task_id=task_id)

    if allows_all(_JOBS_WAIT):

        @mcp.tool(annotations=_READ_ONLY)
        async def jobs_wait(
            job_id: str,
            task_id: str,
            timeout_seconds: float = 1.5,
            ctx: MCPContext | None = None,
        ) -> JobResponse:
            """Wait for job progress until terminal status or timeout.

            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            bridge = _require_bridge(ctx)
            raw = await bridge.wait_job(
                job_id=job_id,
                task_id=task_id,
                timeout_seconds=timeout_seconds,
            )
            return _build_job_poll_response(raw=raw, job_id=job_id, task_id=task_id)

    if allows_all(_JOBS_EVENTS):

        @mcp.tool(annotations=_READ_ONLY)
        async def jobs_events(
            job_id: str,
            task_id: str,
            limit: int = 50,
            offset: int = 0,
            ctx: MCPContext | None = None,
        ) -> JobEventsResponse:
            """List paginated events emitted by a submitted core job.

            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            bridge = _require_bridge(ctx)
            raw = await bridge.list_job_events(
                job_id=job_id,
                task_id=task_id,
                limit=limit,
                offset=offset,
            )
            envelope = _envelope_fields(raw, default_success=False, default_message=None)
            events: list[JobEvent] = []
            events_raw = raw.get("events")
            if isinstance(events_raw, list):
                for raw_event in events_raw:
                    if not isinstance(raw_event, dict):
                        continue
                    events.append(
                        JobEvent(
                            job_id=_str_or_none(raw_event.get("job_id")),
                            task_id=_str_or_none(raw_event.get("task_id")),
                            status=_str_or_none(raw_event.get("status")),
                            timestamp=_str_or_none(raw_event.get("timestamp")),
                            message=_str_or_none(raw_event.get("message")),
                            code=_str_or_none(raw_event.get("code")),
                        )
                    )
            total_events = _int_or_none(raw.get("total_events"))
            returned_events = _int_or_none(raw.get("returned_events"))
            page_offset = _int_or_none(raw.get("offset"))
            page_limit = _int_or_none(raw.get("limit"))
            next_offset = _int_or_none(raw.get("next_offset"))
            has_more_value = raw.get("has_more")
            has_more = (
                has_more_value if isinstance(has_more_value, bool) else next_offset is not None
            )
            return JobEventsResponse(
                success=envelope.success,
                message=envelope.message,
                code=envelope.code,
                hint=envelope.hint,
                next_tool=envelope.next_tool,
                next_arguments=envelope.next_arguments,
                job_id=_str_or_none(raw.get("job_id")) or job_id,
                task_id=_str_or_none(raw.get("task_id")) or task_id,
                events=events,
                total_events=total_events if total_events is not None else len(events),
                returned_events=returned_events if returned_events is not None else len(events),
                offset=page_offset if page_offset is not None else offset,
                limit=page_limit if page_limit is not None else limit,
                has_more=has_more,
                next_offset=next_offset,
            )

    if allows_all(_JOBS_CANCEL):

        @mcp.tool(annotations=_MUTATING)
        async def jobs_cancel(
            job_id: str,
            task_id: str,
            ctx: MCPContext | None = None,
        ) -> JobResponse:
            """Cancel a submitted job.

            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            bridge = _require_bridge(ctx)
            raw = await bridge.cancel_job(job_id=job_id, task_id=task_id)
            envelope = _envelope_fields(raw, default_success=False, default_message="")
            result, runtime_raw, current_task_type = _extract_job_payload(raw)
            next_tool = _str_or_none(raw.get("next_tool"))
            next_arguments: dict[str, object] | None = _dict_or_none(raw.get("next_arguments"))
            hint = _str_or_none(raw.get("hint"))
            if envelope.success and next_tool is None:
                next_tool = TOOL_JOBS_WAIT
                next_arguments = {"job_id": job_id, "task_id": task_id, "timeout_seconds": 1.5}
                if hint is None:
                    hint = "Use jobs_wait to confirm terminal status."
            return _build_job_response(
                raw=raw,
                envelope=envelope,
                hint=hint,
                next_tool=next_tool,
                next_arguments=next_arguments,
                job_id=_str_or_none(raw.get("job_id")) or job_id,
                task_id=_str_or_none(raw.get("task_id")) or task_id,
                action=_str_or_none(raw.get("action")),
                result=result,
                runtime_raw=runtime_raw,
                current_task_type=current_task_type,
            )


# ---------------------------------------------------------------------------
# Session tool registration
# ---------------------------------------------------------------------------


def _register_session_tools(
    mcp: FastMCP,
    *,
    allows_all: Callable[..., bool],
    helpers: ToolRegistrationContext,
    read_only_annotation: ToolAnnotations,
    mutating_annotation: ToolAnnotations,
) -> None:
    """Register PAIR session lifecycle MCP tools."""
    _require_bridge = helpers.require_bridge
    _envelope_fields = helpers.envelope_fields
    _envelope_recovery_fields = helpers.envelope_recovery_fields
    _READ_ONLY = read_only_annotation
    _MUTATING = mutating_annotation

    if allows_all(_SESSIONS_CREATE):

        @mcp.tool(annotations=_MUTATING)
        async def sessions_create(
            task_id: str,
            reuse_if_exists: bool = True,
            worktree_path: str | None = None,
            ctx: MCPContext | None = None,
        ) -> SessionCreateResponse:
            """Create/reuse a PAIR session and return human handoff instructions.

            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            bridge = _require_bridge(ctx)
            raw = await bridge.create_session(
                task_id,
                reuse_if_exists=reuse_if_exists,
                worktree_path=worktree_path,
            )
            envelope = _envelope_fields(raw, default_success=False)
            return SessionCreateResponse(
                task_id=raw.get("task_id", task_id),
                session_name=raw.get("session_name", ""),
                backend=raw.get("backend", ""),
                already_exists=raw.get("already_exists", False),
                worktree_path=raw.get("worktree_path", ""),
                prompt_path=raw.get("prompt_path", ""),
                primary_command=raw.get("primary_command", ""),
                commands=raw.get("commands", []),
                links=raw.get("links", {}),
                instructions=raw.get("instructions", ""),
                next_step=raw.get("next_step", ""),
                **_envelope_recovery_fields(envelope),
                current_task_type=raw.get("current_task_type"),
            )

    if allows_all(_SESSIONS_EXISTS):

        @mcp.tool(annotations=_READ_ONLY)
        async def sessions_exists(
            task_id: str,
            ctx: MCPContext | None = None,
        ) -> SessionExistsResponse:
            """Check whether a PAIR session exists for a task."""
            bridge = _require_bridge(ctx)
            raw = await bridge.session_exists(task_id)
            return SessionExistsResponse(
                task_id=raw.get("task_id", task_id),
                exists=raw.get("exists", False),
                session_name=raw.get("session_name", f"kagan-{task_id}"),
                backend=raw.get("backend"),
                worktree_path=raw.get("worktree_path"),
                prompt_path=raw.get("prompt_path"),
            )

    if allows_all(_SESSIONS_KILL):

        @mcp.tool(annotations=_MUTATING)
        async def sessions_kill(
            task_id: str,
            ctx: MCPContext | None = None,
        ) -> SessionKillResponse:
            """Terminate a PAIR session for a task.

            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            bridge = _require_bridge(ctx)
            raw = await bridge.kill_session(task_id)
            envelope = _envelope_fields(raw, default_success=False, default_message="")
            return SessionKillResponse(
                task_id=raw.get("task_id", task_id),
                **_envelope_recovery_fields(envelope),
            )


def register_automation_tools(
    mcp: FastMCP,
    *,
    allows_all: Callable[..., bool],
    helpers: ToolRegistrationContext,
    read_only_annotation: ToolAnnotations,
    mutating_annotation: ToolAnnotations,
) -> None:
    """Register job and session MCP tools."""
    register_job_tools(
        mcp,
        allows_all=allows_all,
        helpers=helpers,
        read_only_annotation=read_only_annotation,
        mutating_annotation=mutating_annotation,
    )
    _register_session_tools(
        mcp,
        allows_all=allows_all,
        helpers=helpers,
        read_only_annotation=read_only_annotation,
        mutating_annotation=mutating_annotation,
    )


# ---------------------------------------------------------------------------
# Admin / review / settings tool registration
# ---------------------------------------------------------------------------


def register_admin_tools(
    mcp: FastMCP,
    *,
    allows_all: Callable[..., bool],
    allows_any: Callable[..., bool],
    effective_profile: str,
    enable_internal_instrumentation: bool,
    helpers: ToolRegistrationContext,
    read_only_annotation: ToolAnnotations,
    mutating_annotation: ToolAnnotations,
    destructive_annotation: ToolAnnotations,
) -> None:
    """Register settings, review, audit, and diagnostics MCP tools."""
    _require_bridge = helpers.require_bridge
    _envelope_fields = helpers.envelope_fields
    _envelope_status_fields = helpers.envelope_status_fields
    _envelope_recovery_fields = helpers.envelope_recovery_fields
    _project_settings_update_fields = helpers.project_settings_update_fields
    _str_or_none = helpers.str_or_none
    _dict_or_none = helpers.dict_or_none
    _is_allowed = helpers.is_allowed
    _READ_ONLY = read_only_annotation
    _MUTATING = mutating_annotation
    _DESTRUCTIVE = destructive_annotation

    if allows_all(_REVIEW_REQUEST):

        @mcp.tool(annotations=_MUTATING)
        async def request_review(
            task_id: str,
            summary: str,
            ctx: MCPContext | None = None,
        ) -> ReviewResponse:
            """Mark task ready for review.

            Call this when implementation is complete. The task will move to REVIEW status.
            Include a summary of what was implemented.

            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            if ctx:
                await ctx.info(f"Requesting review for task {task_id}")
                await ctx.report_progress(0.2, 1.0, "Preparing review request")

            bridge = _require_bridge(ctx)

            raw = await bridge.request_review(task_id, summary)
            next_tool = _str_or_none(raw.get("next_tool"))
            next_arguments = _dict_or_none(raw.get("next_arguments"))
            hint = _str_or_none(raw.get("hint"))
            if raw.get("status") == STATUS_ERROR and next_tool is None:
                next_tool = TOOL_GET_TASK
                next_arguments = {
                    "task_id": task_id,
                    "include_logs": True,
                    "mode": "summary",
                }
                if hint is None:
                    hint = "Inspect task runtime/logs before retrying request_review."

            if ctx:
                await ctx.report_progress(1.0, 1.0, "Review request complete")
                if raw["status"] == STATUS_ERROR:
                    await ctx.warning(f"Review request failed: {raw['message']}")
                else:
                    await ctx.debug("Task moved to REVIEW status")

            envelope = _envelope_fields(
                raw,
                default_success=bool(raw.get("status") != STATUS_ERROR),
                default_message=str(raw.get("message", "")),
            )
            return ReviewResponse(
                status=raw["status"],
                **_envelope_status_fields(envelope),
                hint=hint,
                next_tool=next_tool,
                next_arguments=next_arguments,
            )

    if allows_all(_SETTINGS_GET):

        @mcp.tool(annotations=_READ_ONLY)
        async def settings_get(
            ctx: MCPContext | None = None,
        ) -> SettingsGetResponse:
            """Get admin-exposed settings snapshot."""
            bridge = _require_bridge(ctx)

            raw = await bridge.get_settings()
            return SettingsGetResponse(settings=raw.get("settings", {}))

    if enable_internal_instrumentation and allows_all(_DIAGNOSTICS_INSTRUMENTATION):

        @mcp.tool(annotations=_READ_ONLY)
        async def diagnostics_instrumentation(
            ctx: MCPContext | None = None,
        ) -> InstrumentationSnapshotResponse:
            """Get internal in-memory core instrumentation snapshot.

            This tool is disabled by default and must be explicitly enabled for diagnostics.
            """
            bridge = _require_bridge(ctx)
            raw = await bridge.get_instrumentation_snapshot()

            counters_raw = raw.get("counters", {})
            counters: dict[str, int] = {}
            if isinstance(counters_raw, dict):
                for key, value in counters_raw.items():
                    if isinstance(value, int):
                        counters[str(key)] = value

            timings_raw = raw.get("timings", {})
            timings: dict[str, dict[str, float | int]] = {}
            if isinstance(timings_raw, dict):
                for metric_name, stats in timings_raw.items():
                    if not isinstance(stats, dict):
                        continue
                    normalized_stats: dict[str, float | int] = {}
                    for field_name, field_value in stats.items():
                        if isinstance(field_value, int | float):
                            normalized_stats[str(field_name)] = field_value
                    timings[str(metric_name)] = normalized_stats

            return InstrumentationSnapshotResponse(
                enabled=bool(raw.get("enabled", False)),
                log_events=bool(raw.get("log_events", False)),
                counters=counters,
                timings=timings,
            )

    if allows_all(_SETTINGS_UPDATE):

        @mcp.tool(annotations=_MUTATING)
        async def settings_update(
            auto_review: bool | None = None,
            auto_approve: bool | None = None,
            require_review_approval: bool | None = None,
            serialize_merges: bool | None = None,
            worktree_base_ref_strategy: str | None = None,
            max_concurrent_agents: int | None = None,
            default_worker_agent: str | None = None,
            default_pair_terminal_backend: str | None = None,
            default_model_claude: str | None = None,
            default_model_opencode: str | None = None,
            default_model_codex: str | None = None,
            default_model_gemini: str | None = None,
            default_model_kimi: str | None = None,
            default_model_copilot: str | None = None,
            tasks_wait_default_timeout_seconds: int | None = None,
            tasks_wait_max_timeout_seconds: int | None = None,
            skip_pair_instructions: bool | None = None,
            ctx: MCPContext | None = None,
        ) -> SettingsUpdateResponse:
            """Update allowlisted settings fields (maintainer/admin lane).

            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            bridge = _require_bridge(ctx)
            fields = _project_settings_update_fields(
                {
                    "auto_review": auto_review,
                    "auto_approve": auto_approve,
                    "require_review_approval": require_review_approval,
                    "serialize_merges": serialize_merges,
                    "worktree_base_ref_strategy": worktree_base_ref_strategy,
                    "max_concurrent_agents": max_concurrent_agents,
                    "default_worker_agent": default_worker_agent,
                    "default_pair_terminal_backend": default_pair_terminal_backend,
                    "default_model_claude": default_model_claude,
                    "default_model_opencode": default_model_opencode,
                    "default_model_codex": default_model_codex,
                    "default_model_gemini": default_model_gemini,
                    "default_model_kimi": default_model_kimi,
                    "default_model_copilot": default_model_copilot,
                    "tasks_wait_default_timeout_seconds": tasks_wait_default_timeout_seconds,
                    "tasks_wait_max_timeout_seconds": tasks_wait_max_timeout_seconds,
                    "skip_pair_instructions": skip_pair_instructions,
                }
            )

            raw = await bridge.update_settings(fields)
            envelope = _envelope_fields(raw, default_success=False, default_message="")
            return SettingsUpdateResponse(
                **_envelope_recovery_fields(envelope),
                updated=raw.get("updated", {}),
                settings=raw.get("settings", {}),
            )

    if allows_any(
        _REVIEW_APPROVE,
        _REVIEW_REJECT,
        _REVIEW_MERGE,
        _REVIEW_REBASE,
    ):

        @mcp.tool(annotations=_DESTRUCTIVE)
        async def review(
            task_id: str,
            action: ReviewActionInput,
            feedback: str = "",
            rejection_action: RejectionActionInput = "reopen",
            ctx: MCPContext | None = None,
        ) -> ReviewActionResponse:
            """Perform a review action on a task.

            Args:
                task_id: The task to act on.
                action: One of "approve", "reject", "merge", "rebase".
                feedback: Rejection feedback (only used when action is "reject").
                rejection_action: What to do after rejection
                    (only used when action is "reject").

            rejection_action values:
            - backlog: move task to BACKLOG
            - return/in_progress/reopen: move task to IN_PROGRESS
            Recovery policy: if response includes next_tool and next_arguments,
            call that tool exactly once before any retry.
            """
            bridge = _require_bridge(ctx)
            if not _is_allowed(effective_profile, ProtocolCapability.REVIEW, action):
                return ReviewActionResponse(
                    success=False,
                    task_id=task_id,
                    message=f"Action '{action}' is not allowed for this capability profile.",
                    code="ACTION_NOT_ALLOWED",
                    hint="Use one of the actions permitted by your current capability profile.",
                )

            raw = await bridge.review_action(
                task_id,
                action=action,
                feedback=feedback,
                rejection_action=rejection_action,
            )
            envelope = _envelope_fields(raw, default_success=False, default_message="")
            return ReviewActionResponse(
                task_id=raw.get("task_id", task_id),
                **_envelope_recovery_fields(envelope),
            )


# ---------------------------------------------------------------------------
# Full-mode orchestrator
# ---------------------------------------------------------------------------


def register_full_mode_tools(
    mcp: FastMCP,
    *,
    allows_all: Callable[..., bool],
    allows_any: Callable[..., bool],
    effective_profile: str,
    enable_internal_instrumentation: bool,
    helpers: ToolRegistrationContext,
    read_only_annotation: ToolAnnotations,
    mutating_annotation: ToolAnnotations,
    destructive_annotation: ToolAnnotations,
) -> None:
    """Register mutating/full-mode-only MCP tools.

    Delegates to domain-grouped registration functions in sequence.
    """
    register_task_tools(
        mcp,
        allows_all=allows_all,
        helpers=helpers,
        read_only_annotation=read_only_annotation,
        mutating_annotation=mutating_annotation,
        destructive_annotation=destructive_annotation,
    )

    register_automation_tools(
        mcp,
        allows_all=allows_all,
        helpers=helpers,
        read_only_annotation=read_only_annotation,
        mutating_annotation=mutating_annotation,
    )

    register_admin_tools(
        mcp,
        allows_all=allows_all,
        allows_any=allows_any,
        effective_profile=effective_profile,
        enable_internal_instrumentation=enable_internal_instrumentation,
        helpers=helpers,
        read_only_annotation=read_only_annotation,
        mutating_annotation=mutating_annotation,
        destructive_annotation=destructive_annotation,
    )

__all__ = [
    "SharedToolRegistrationContext",
    "ToolRegistrationContext",
    "register_full_mode_tools",
    "register_shared_tools",
]
