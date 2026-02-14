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
    GitHubConnectionMetadata,
    GitHubConnectRepoResponse,
    GitHubContractProbeResponse,
    GitHubSyncIssuesResponse,
    GitHubSyncStats,
    InstrumentationSnapshotResponse,
    JobEvent,
    JobEventsResponse,
    JobResponse,
    PlanProposalResponse,
    ProjectInfo,
    ProjectListResponse,
    ProjectOpenResponse,
    RepoListItem,
    RepoListResponse,
    ReviewActionResponse,
    SettingsGetResponse,
    SettingsUpdateResponse,
    TaskCreateResponse,
    TaskDeleteResponse,
    TaskListResponse,
    TaskLogsResponse,
    TaskRuntimeState,
    TaskSummary,
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
    "backlog",
    "in_progress",
    "review",
    "done",
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

TASK_CODE_TASK_TYPE_VALUE_IN_STATUS: Final[str] = "TASK_TYPE_VALUE_IN_STATUS"

STATUS_ERROR: Final[str] = "error"

JOB_NON_TERMINAL_STATUSES: Final[frozenset[str]] = frozenset({"queued", "running"})
JOB_TERMINAL_STATUSES: Final[frozenset[str]] = frozenset({"succeeded", "failed", "cancelled"})

JOB_CODE_UNSUPPORTED_ACTION: Final[str] = "UNSUPPORTED_ACTION"
JOB_CODE_JOB_TIMEOUT: Final[str] = "JOB_TIMEOUT"
JOB_CODE_TASK_TYPE_MISMATCH: Final[str] = "TASK_TYPE_MISMATCH"
JOB_CODE_START_PENDING: Final[str] = "START_PENDING"
JOB_CODE_NOT_RUNNING: Final[str] = "NOT_RUNNING"
DEFAULT_JOB_POLL_WAIT_TIMEOUT_SECONDS: Final[float] = 1.5

TOOL_TASK_GET: Final[str] = "task_get"
TOOL_TASK_WAIT: Final[str] = "task_wait"
TOOL_TASK_PATCH: Final[str] = "task_patch"
TOOL_TASK_CREATE: Final[str] = "task_create"
TOOL_JOB_START: Final[str] = "job_start"
TOOL_JOB_POLL: Final[str] = "job_poll"
TOOL_SESSION_MANAGE: Final[str] = "session_manage"

# ---------------------------------------------------------------------------
# Protocol call constants
# ---------------------------------------------------------------------------

# Shared / read-only
_PLAN_PROPOSE = protocol_call(ProtocolCapability.PLAN, PlanMethod.PROPOSE)
_TASKS_GET = protocol_call(ProtocolCapability.TASKS, TasksMethod.GET)
_TASKS_SCRATCHPAD = protocol_call(ProtocolCapability.TASKS, TasksMethod.SCRATCHPAD)
_TASKS_LIST = protocol_call(ProtocolCapability.TASKS, TasksMethod.LIST)
_TASKS_LOGS = protocol_call(ProtocolCapability.TASKS, TasksMethod.LOGS)
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
        async def plan_submit(
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
        async def task_get(
            task_id: str,
            include_scratchpad: bool | None = None,
            include_logs: bool | None = None,
            include_review: bool | None = None,
            mode: str = "summary",
            ctx: MCPContext | None = None,
        ) -> dict[str, object]:
            """Get task details (summary/full) or bounded full context.

            Args:
                task_id: The task to retrieve
                include_scratchpad: Include agent notes
                include_logs: Include execution logs from previous runs
                include_review: Include review feedback
                mode: 'summary', 'full', or 'context'
            """
            if ctx:
                await ctx.info(f"Fetching task details for {task_id}")

            bridge = _require_bridge(ctx)
            if mode == "context":
                raw = await bridge.get_context(task_id)
                if ctx:
                    await ctx.debug(
                        f"Task context loaded: repos={len(raw.get('repos', []))} "
                        f"linked={len(raw.get('linked_tasks', []))}"
                    )
                raw_runtime = raw.get("runtime")
                if isinstance(raw_runtime, dict):
                    raw["runtime"] = _runtime_state_from_raw(raw_runtime)
                return raw

            raw = await bridge.get_task(
                task_id,
                include_scratchpad=include_scratchpad,
                include_logs=include_logs,
                include_review=include_review,
                mode=mode,
            )
            if ctx:
                await ctx.debug(f"Task retrieved: status={raw.get('status')}")
            raw_runtime = raw.get("runtime")
            if isinstance(raw_runtime, dict):
                raw["runtime"] = _runtime_state_from_raw(raw_runtime)
            if include_logs:
                raw_logs = raw.get("logs")
                if isinstance(raw_logs, list):
                    raw["logs"] = _normalize_agent_log_entries(raw_logs)
            return raw

    if allows_all(_TASKS_LOGS):

        @mcp.tool(annotations=_READ_ONLY)
        async def task_logs(
            task_id: str,
            limit: int = 5,
            offset: int = 0,
            ctx: MCPContext | None = None,
        ) -> TaskLogsResponse:
            """Get paginated task logs."""
            bridge = _require_bridge(ctx)
            raw = await bridge.list_task_logs(task_id=task_id, limit=limit, offset=offset)
            normalized_logs = _normalize_agent_log_entries(raw.get("logs"))

            total_runs = _int_or_none(raw.get("total_runs"))
            returned_runs = _int_or_none(raw.get("returned_runs"))
            page_offset = _int_or_none(raw.get("offset"))
            page_limit = _int_or_none(raw.get("limit"))
            next_offset = _int_or_none(raw.get("next_offset"))
            has_more_raw = raw.get("has_more")
            has_more = has_more_raw if isinstance(has_more_raw, bool) else next_offset is not None
            task_id_value = raw.get("task_id")
            message = raw.get("message")
            code = raw.get("code")
            hint = raw.get("hint")
            next_tool = raw.get("next_tool")
            next_arguments = raw.get("next_arguments")
            return TaskLogsResponse(
                task_id=task_id_value if isinstance(task_id_value, str) else task_id,
                logs=normalized_logs,
                count=_int_or_none(raw.get("count")) or len(normalized_logs),
                total_runs=total_runs if total_runs is not None else len(normalized_logs),
                returned_runs=returned_runs if returned_runs is not None else len(normalized_logs),
                offset=page_offset if page_offset is not None else offset,
                limit=page_limit if page_limit is not None else limit,
                has_more=has_more,
                next_offset=next_offset,
                truncated=bool(raw.get("truncated", False)),
                message=message if isinstance(message, str) else None,
                code=code if isinstance(code, str) else None,
                hint=hint if isinstance(hint, str) else None,
                next_tool=next_tool if isinstance(next_tool, str) else None,
                next_arguments=next_arguments if isinstance(next_arguments, dict) else None,
            )

    if allows_all(_TASKS_LIST):

        @mcp.tool(annotations=_READ_ONLY)
        async def task_list(
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
        async def task_wait(
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
        async def project_list(
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
        async def repo_list(
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
        async def audit_list(
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
    """Register task and project MCP tools."""
    _require_bridge = helpers.require_bridge
    _envelope_fields = helpers.envelope_fields
    _envelope_recovery_fields = helpers.envelope_recovery_fields
    _normalized_mode = helpers.normalized_mode
    _MUTATING = mutating_annotation
    _DESTRUCTIVE = destructive_annotation

    can_patch_note = allows_all(_TASKS_UPDATE_SCRATCHPAD)
    can_patch_fields = allows_all(_TASKS_UPDATE)
    can_patch_status = allows_all(_TASKS_MOVE)
    can_request_review = allows_all(_REVIEW_REQUEST)

    if allows_all(_TASKS_CREATE):

        @mcp.tool(annotations=_MUTATING)
        async def task_create(
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
            """Create a new task."""
            bridge = _require_bridge(ctx)
            mode_from_status = _normalized_mode(str(status) if status is not None else None)
            if mode_from_status is not None:
                return TaskCreateResponse(
                    success=False,
                    message=(
                        f"Invalid status value {status!r}. "
                        "AUTO/PAIR are task_type values, not status values."
                    ),
                    code=TASK_CODE_TASK_TYPE_VALUE_IN_STATUS,
                    hint="Pass task_type explicitly and keep status in Kanban states.",
                    next_tool=TOOL_TASK_CREATE,
                    next_arguments={
                        "title": title,
                        "description": description,
                        "project_id": project_id,
                        "task_type": mode_from_status,
                        "priority": priority,
                        "terminal_backend": terminal_backend,
                        "agent_backend": agent_backend,
                        "parent_id": parent_id,
                        "base_branch": base_branch,
                        "acceptance_criteria": acceptance_criteria,
                        "created_by": created_by,
                    },
                    task_id="",
                    title=title,
                    status=TaskStatus.BACKLOG.value,
                )

            raw = await bridge.create_task(
                title=title,
                description=description,
                project_id=project_id,
                status=status,
                priority=priority,
                task_type=task_type,
                terminal_backend=terminal_backend,
                agent_backend=agent_backend,
                parent_id=parent_id,
                base_branch=base_branch,
                acceptance_criteria=acceptance_criteria,
                created_by=created_by,
            )
            envelope = _envelope_fields(raw, default_success=True)
            return TaskCreateResponse(
                task_id=raw["task_id"],
                title=raw.get("title", title),
                status=raw.get("status", TaskStatus.BACKLOG.value),
                **_envelope_recovery_fields(envelope),
            )

    if can_patch_note or can_patch_fields or can_patch_status or can_request_review:

        @mcp.tool(annotations=_MUTATING)
        async def task_patch(
            task_id: str,
            set: dict[str, object] | None = None,
            transition: str | None = None,
            append_note: str | None = None,
            ctx: MCPContext | None = None,
        ) -> dict[str, object]:
            """Apply partial task changes, transitions, and scratchpad notes.

            Note: direct move/update to DONE is rejected. Use review completion
            flows (for example review_apply(action="merge") or close no-change flow).
            """
            bridge = _require_bridge(ctx)
            if set is None and transition is None and append_note is None:
                return {
                    "success": False,
                    "task_id": task_id,
                    "message": "At least one of set, transition, or append_note is required.",
                    "code": "INVALID_PATCH",
                }

            fields = dict(set) if isinstance(set, dict) else {}
            if set is not None and not isinstance(set, dict):
                return {
                    "success": False,
                    "task_id": task_id,
                    "message": "set must be an object map",
                    "code": "INVALID_SET",
                }

            def _as_response(
                raw: dict[str, object], *, default_success: bool, default_message: str | None = None
            ) -> dict[str, object]:
                envelope = _envelope_fields(
                    raw,
                    default_success=default_success,
                    default_message=default_message,
                )
                response: dict[str, object] = {"task_id": raw.get("task_id", task_id)}
                response.update(_envelope_recovery_fields(envelope))
                if "new_status" in raw:
                    response["new_status"] = raw.get("new_status")
                if "status" in raw:
                    response["status"] = raw.get("status")
                if "current_task_type" in raw:
                    response["current_task_type"] = raw.get("current_task_type")
                return response

            if append_note is not None:
                if not can_patch_note:
                    return {
                        "success": False,
                        "task_id": task_id,
                        "message": "append_note is not allowed for this capability profile.",
                        "code": "ACTION_NOT_ALLOWED",
                    }
                note_raw = await bridge.update_scratchpad(task_id, append_note)
                note_response = _as_response(
                    note_raw,
                    default_success=True,
                    default_message="Scratchpad updated",
                )
                if not bool(note_response.get("success", False)):
                    return note_response

            status_for_move = fields.pop("status", None)
            task_type_for_update = fields.get("task_type")

            if transition == "set_status":
                if status_for_move is None:
                    status_for_move = fields.pop("new_status", None)
                if not isinstance(status_for_move, str) or not status_for_move.strip():
                    return {
                        "success": False,
                        "task_id": task_id,
                        "message": "set_status requires set.status",
                        "code": "INVALID_TRANSITION",
                    }
                mode_value = _normalized_mode(status_for_move)
                if mode_value is not None:
                    return {
                        "success": False,
                        "task_id": task_id,
                        "message": (
                            f"Invalid status value {status_for_move!r}. "
                            "AUTO/PAIR are task_type values, not status values."
                        ),
                        "code": TASK_CODE_TASK_TYPE_VALUE_IN_STATUS,
                        "hint": "Use transition='set_task_type' with set.task_type.",
                        "next_tool": TOOL_TASK_PATCH,
                        "next_arguments": {
                            "task_id": task_id,
                            "transition": "set_task_type",
                            "set": {"task_type": mode_value},
                        },
                    }
                if not can_patch_status:
                    return {
                        "success": False,
                        "task_id": task_id,
                        "message": "set_status is not allowed for this capability profile.",
                        "code": "ACTION_NOT_ALLOWED",
                    }
                move_raw = await bridge.move_task(task_id, status_for_move)
                return _as_response(move_raw, default_success=True)

            if transition == "set_task_type":
                task_type_value = task_type_for_update
                if not isinstance(task_type_value, str) or not task_type_value.strip():
                    return {
                        "success": False,
                        "task_id": task_id,
                        "message": "set_task_type requires set.task_type",
                        "code": "INVALID_TRANSITION",
                    }
                if not can_patch_fields:
                    return {
                        "success": False,
                        "task_id": task_id,
                        "message": "set_task_type is not allowed for this capability profile.",
                        "code": "ACTION_NOT_ALLOWED",
                    }
                update_raw = await bridge.update_task(task_id, task_type=task_type_value)
                return _as_response(update_raw, default_success=True)

            if transition == "request_review":
                if not can_request_review:
                    return {
                        "success": False,
                        "task_id": task_id,
                        "message": "request_review is not allowed for this capability profile.",
                        "code": "ACTION_NOT_ALLOWED",
                    }
                summary_raw = fields.pop("summary", "")
                summary = summary_raw if isinstance(summary_raw, str) else ""
                review_raw = await bridge.request_review(task_id, summary)
                return _as_response(
                    review_raw,
                    default_success=bool(review_raw.get("status") != STATUS_ERROR),
                    default_message=str(review_raw.get("message", "")),
                )

            if transition is not None:
                return {
                    "success": False,
                    "task_id": task_id,
                    "message": f"Unsupported transition {transition!r}",
                    "code": "INVALID_TRANSITION",
                }

            if status_for_move is not None:
                if not isinstance(status_for_move, str) or not status_for_move.strip():
                    return {
                        "success": False,
                        "task_id": task_id,
                        "message": "set.status must be a non-empty string",
                        "code": "INVALID_STATUS",
                    }
                if not can_patch_status:
                    return {
                        "success": False,
                        "task_id": task_id,
                        "message": "status patch is not allowed for this capability profile.",
                        "code": "ACTION_NOT_ALLOWED",
                    }
                move_raw = await bridge.move_task(task_id, status_for_move)
                move_response = _as_response(move_raw, default_success=True)
                if not bool(move_response.get("success", False)):
                    return move_response

            if fields:
                if not can_patch_fields:
                    return {
                        "success": False,
                        "task_id": task_id,
                        "message": "field patch is not allowed for this capability profile.",
                        "code": "ACTION_NOT_ALLOWED",
                    }
                update_raw = await bridge.update_task(task_id, **fields)
                update_response = _as_response(update_raw, default_success=True)
                if not bool(update_response.get("success", False)):
                    return update_response
                return update_response

            return {
                "success": True,
                "task_id": task_id,
                "message": "Patch applied",
            }

    if allows_all(_TASKS_DELETE):

        @mcp.tool(annotations=_DESTRUCTIVE)
        async def task_delete(
            task_id: str,
            ctx: MCPContext | None = None,
        ) -> TaskDeleteResponse:
            """Delete a task."""
            bridge = _require_bridge(ctx)
            raw = await bridge.delete_task(task_id)
            envelope = _envelope_fields(raw, default_success=False)
            return TaskDeleteResponse(
                task_id=raw.get("task_id", task_id),
                **_envelope_recovery_fields(envelope),
            )

    if allows_all(_PROJECTS_OPEN):

        @mcp.tool(annotations=_MUTATING)
        async def project_open(
            project_id: str,
            ctx: MCPContext | None = None,
        ) -> ProjectOpenResponse:
            """Open/switch to a project."""
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


def _normalize_agent_log_entries(raw_logs: object) -> list[AgentLogEntry]:
    if not isinstance(raw_logs, list):
        return []
    normalized_logs: list[AgentLogEntry] = []
    for log in raw_logs:
        if not isinstance(log, dict):
            continue
        run = log.get("run")
        content = log.get("content")
        created_at = log.get("created_at")
        if isinstance(run, int) and isinstance(content, str) and isinstance(created_at, str):
            normalized_logs.append(AgentLogEntry(run=run, content=content, created_at=created_at))
    return normalized_logs


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

        @mcp.tool(annotations=_MUTATING)
        async def job_start(
            task_id: str,
            action: JobActionInput,
            arguments: dict[str, object] | None = None,
            ctx: MCPContext | None = None,
        ) -> JobResponse:
            """Submit an asynchronous core job."""
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
                next_tool = TOOL_JOB_START
                next_arguments = {"task_id": task_id, "action": sorted(SUPPORTED_JOB_ACTIONS)[0]}
                if hint is None:
                    hint = f"Use one of: {', '.join(sorted(SUPPORTED_JOB_ACTIONS))}"
            if envelope.success and next_tool is None and job_id:
                next_tool = TOOL_JOB_POLL
                next_arguments = {
                    "job_id": job_id,
                    "task_id": returned_task_id,
                    "wait": True,
                    "timeout_seconds": DEFAULT_JOB_POLL_WAIT_TIMEOUT_SECONDS,
                }
                if hint is None:
                    hint = (
                        "Call job_poll(wait=true) to confirm spawn, then use "
                        "task_wait(task_id, wait_for_status=['REVIEW','DONE']) "
                        "to long-poll completion."
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

    if allows_all(_JOBS_GET) or allows_all(_JOBS_WAIT) or allows_all(_JOBS_EVENTS):

        @mcp.tool(annotations=_READ_ONLY)
        async def job_poll(
            job_id: str,
            task_id: str,
            wait: bool = False,
            timeout_seconds: float = DEFAULT_JOB_POLL_WAIT_TIMEOUT_SECONDS,
            events: bool = False,
            limit: int = 50,
            offset: int = 0,
            ctx: MCPContext | None = None,
        ) -> JobResponse | JobEventsResponse:
            """Read job state, optionally waiting or paging events."""
            bridge = _require_bridge(ctx)
            if events:
                if not allows_all(_JOBS_EVENTS):
                    return JobResponse(
                        success=False,
                        message="events view is not allowed for this capability profile",
                        code="ACTION_NOT_ALLOWED",
                        job_id=job_id,
                        task_id=task_id,
                    )
                raw = await bridge.list_job_events(
                    job_id=job_id,
                    task_id=task_id,
                    limit=limit,
                    offset=offset,
                )
                envelope = _envelope_fields(raw, default_success=False, default_message=None)
                event_items: list[JobEvent] = []
                events_raw = raw.get("events")
                if isinstance(events_raw, list):
                    for raw_event in events_raw:
                        if not isinstance(raw_event, dict):
                            continue
                        event_items.append(
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
                    events=event_items,
                    total_events=total_events if total_events is not None else len(event_items),
                    returned_events=(
                        returned_events if returned_events is not None else len(event_items)
                    ),
                    offset=page_offset if page_offset is not None else offset,
                    limit=page_limit if page_limit is not None else limit,
                    has_more=has_more,
                    next_offset=next_offset,
                )

            if wait:
                if not allows_all(_JOBS_WAIT):
                    return JobResponse(
                        success=False,
                        message="wait mode is not allowed for this capability profile",
                        code="ACTION_NOT_ALLOWED",
                        job_id=job_id,
                        task_id=task_id,
                    )
                raw = await bridge.wait_job(
                    job_id=job_id,
                    task_id=task_id,
                    timeout_seconds=timeout_seconds,
                )
            else:
                if not allows_all(_JOBS_GET):
                    return JobResponse(
                        success=False,
                        message="poll mode is not allowed for this capability profile",
                        code="ACTION_NOT_ALLOWED",
                        job_id=job_id,
                        task_id=task_id,
                    )
                raw = await bridge.get_job(job_id=job_id, task_id=task_id)
            return _build_job_poll_response(raw=raw, job_id=job_id, task_id=task_id)

    if allows_all(_JOBS_CANCEL):

        @mcp.tool(annotations=_MUTATING)
        async def job_cancel(
            job_id: str,
            task_id: str,
            ctx: MCPContext | None = None,
        ) -> JobResponse:
            """Cancel a submitted job."""
            bridge = _require_bridge(ctx)
            raw = await bridge.cancel_job(job_id=job_id, task_id=task_id)
            envelope = _envelope_fields(raw, default_success=False, default_message="")
            result, runtime_raw, current_task_type = _extract_job_payload(raw)
            next_tool = _str_or_none(raw.get("next_tool"))
            next_arguments: dict[str, object] | None = _dict_or_none(raw.get("next_arguments"))
            hint = _str_or_none(raw.get("hint"))
            if envelope.success and next_tool is None:
                next_tool = TOOL_JOB_POLL
                next_arguments = {
                    "job_id": job_id,
                    "task_id": task_id,
                    "wait": True,
                    "timeout_seconds": DEFAULT_JOB_POLL_WAIT_TIMEOUT_SECONDS,
                }
                if hint is None:
                    hint = "Use job_poll(wait=true) to confirm terminal status."
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
    """Register consolidated PAIR session lifecycle MCP tool."""
    _require_bridge = helpers.require_bridge
    _envelope_fields = helpers.envelope_fields
    _envelope_recovery_fields = helpers.envelope_recovery_fields
    _MUTATING = mutating_annotation

    if allows_all(_SESSIONS_CREATE) or allows_all(_SESSIONS_EXISTS) or allows_all(_SESSIONS_KILL):

        @mcp.tool(annotations=_MUTATING)
        async def session_manage(
            action: Literal["open", "read", "close"],
            task_id: str,
            reuse_if_exists: bool = True,
            worktree_path: str | None = None,
            ctx: MCPContext | None = None,
        ) -> dict[str, object]:
            """Manage PAIR sessions with a single action-oriented interface."""
            bridge = _require_bridge(ctx)
            if action == "open":
                if not allows_all(_SESSIONS_CREATE):
                    return {
                        "success": False,
                        "task_id": task_id,
                        "code": "ACTION_NOT_ALLOWED",
                        "message": "open is not allowed for this capability profile.",
                    }
                raw = await bridge.create_session(
                    task_id,
                    reuse_if_exists=reuse_if_exists,
                    worktree_path=worktree_path,
                )
                envelope = _envelope_fields(raw, default_success=False)
                response: dict[str, object] = {
                    "action": action,
                    "task_id": raw.get("task_id", task_id),
                }
                response.update(_envelope_recovery_fields(envelope))
                response.update(
                    {
                        "session_name": raw.get("session_name", ""),
                        "backend": raw.get("backend", ""),
                        "already_exists": raw.get("already_exists", False),
                        "worktree_path": raw.get("worktree_path", ""),
                        "prompt_path": raw.get("prompt_path", ""),
                        "primary_command": raw.get("primary_command", ""),
                        "commands": raw.get("commands", []),
                        "links": raw.get("links", {}),
                        "instructions": raw.get("instructions", ""),
                        "next_step": raw.get("next_step", ""),
                        "current_task_type": raw.get("current_task_type"),
                    }
                )
                return response

            if action == "read":
                if not allows_all(_SESSIONS_EXISTS):
                    return {
                        "success": False,
                        "task_id": task_id,
                        "code": "ACTION_NOT_ALLOWED",
                        "message": "read is not allowed for this capability profile.",
                    }
                raw = await bridge.session_exists(task_id)
                return {
                    "success": True,
                    "action": action,
                    "task_id": raw.get("task_id", task_id),
                    "exists": raw.get("exists", False),
                    "session_name": raw.get("session_name", f"kagan-{task_id}"),
                    "backend": raw.get("backend"),
                    "worktree_path": raw.get("worktree_path"),
                    "prompt_path": raw.get("prompt_path"),
                }

            if action == "close":
                if not allows_all(_SESSIONS_KILL):
                    return {
                        "success": False,
                        "task_id": task_id,
                        "code": "ACTION_NOT_ALLOWED",
                        "message": "close is not allowed for this capability profile.",
                    }
                raw = await bridge.kill_session(task_id)
                envelope = _envelope_fields(raw, default_success=False, default_message="")
                response = {"action": action, "task_id": raw.get("task_id", task_id)}
                response.update(_envelope_recovery_fields(envelope))
                return response

            return {
                "success": False,
                "task_id": task_id,
                "code": "INVALID_ACTION",
                "message": f"Unsupported session action {action!r}.",
            }


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
        async def settings_set(
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
        async def review_apply(
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
                    "approve" records approval state only; it is non-terminal.
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
# GitHub Plugin MCP Tools (V1 Contract)
# ---------------------------------------------------------------------------

# V1 contract tool names - frozen as stable interface
GITHUB_TOOL_CONTRACT_PROBE = "kagan_github_contract_probe"
GITHUB_TOOL_CONNECT_REPO = "kagan_github_connect_repo"
GITHUB_TOOL_SYNC_ISSUES = "kagan_github_sync_issues"
GITHUB_MCP_V1_TOOLS = (
    GITHUB_TOOL_CONTRACT_PROBE,
    GITHUB_TOOL_CONNECT_REPO,
    GITHUB_TOOL_SYNC_ISSUES,
)


def register_github_tools(
    mcp: FastMCP,
    *,
    effective_profile: str,
    helpers: ToolRegistrationContext,
    read_only_annotation: ToolAnnotations,
    mutating_annotation: ToolAnnotations,
) -> None:
    """Register GitHub plugin admin MCP tools (V1 contract).

    All GitHub admin tools require MAINTAINER profile. The tools delegate
    to the kagan_github plugin capability via CoreClientBridge.
    """
    _require_bridge = helpers.require_bridge
    _envelope_fields = helpers.envelope_fields
    _envelope_recovery_fields = helpers.envelope_recovery_fields
    _READ_ONLY = read_only_annotation
    _MUTATING = mutating_annotation

    # Only expose GitHub tools to MAINTAINER profile
    if effective_profile != str(CapabilityProfile.MAINTAINER):
        return

    @mcp.tool(annotations=_READ_ONLY)
    async def kagan_github_contract_probe(
        echo: str | None = None,
        ctx: MCPContext | None = None,
    ) -> GitHubContractProbeResponse:
        """Probe the GitHub plugin contract for verification (V1 contract).

        Returns plugin metadata including contract version and canonical methods.
        This is a read-only operation that does not modify any state.

        Args:
            echo: Optional value to echo back for round-trip verification.
        """
        bridge = _require_bridge(ctx)
        raw = await bridge.github_contract_probe(echo=echo)
        envelope = _envelope_fields(raw, default_success=True, default_message=None)
        return GitHubContractProbeResponse(
            **_envelope_recovery_fields(envelope),
            plugin_id=raw.get("plugin_id", ""),
            contract_version=raw.get("contract_version", ""),
            capability=raw.get("capability", ""),
            method=raw.get("method", ""),
            canonical_methods=raw.get("canonical_methods", []),
            canonical_scope=raw.get("canonical_scope", "plugin_capability"),
            mcp_v1_tools=list(GITHUB_MCP_V1_TOOLS),
            echo=raw.get("echo"),
        )

    @mcp.tool(annotations=_MUTATING)
    async def kagan_github_connect_repo(
        project_id: str,
        repo_id: str | None = None,
        ctx: MCPContext | None = None,
    ) -> GitHubConnectRepoResponse:
        """Connect a repository to GitHub with preflight checks (V1 contract).

        Performs preflight verification (gh CLI auth, repo access) and persists
        GitHub connection metadata for the target repository.

        Args:
            project_id: Required project ID.
            repo_id: Optional repo ID (required for multi-repo projects).
        """
        bridge = _require_bridge(ctx)
        raw = await bridge.github_connect_repo(project_id=project_id, repo_id=repo_id)
        envelope = _envelope_fields(raw, default_success=False, default_message=None)

        connection: GitHubConnectionMetadata | None = None
        connection_raw = raw.get("connection")
        if isinstance(connection_raw, dict):
            connection = GitHubConnectionMetadata(
                full_name=connection_raw.get("full_name", ""),
                owner=connection_raw.get("owner", ""),
                repo=connection_raw.get("repo", ""),
                default_branch=connection_raw.get("default_branch"),
                visibility=connection_raw.get("visibility"),
                connected_at=connection_raw.get("connected_at"),
            )

        return GitHubConnectRepoResponse(
            **_envelope_recovery_fields(envelope),
            connection=connection,
        )

    @mcp.tool(annotations=_MUTATING)
    async def kagan_github_sync_issues(
        project_id: str,
        repo_id: str | None = None,
        ctx: MCPContext | None = None,
    ) -> GitHubSyncIssuesResponse:
        """Sync GitHub issues to Kagan task projections (V1 contract).

        Fetches issues from GitHub and creates/updates corresponding Kagan tasks.
        Supports incremental sync via checkpoint tracking.

        Args:
            project_id: Required project ID.
            repo_id: Optional repo ID (required for multi-repo projects).
        """
        bridge = _require_bridge(ctx)
        raw = await bridge.github_sync_issues(project_id=project_id, repo_id=repo_id)
        envelope = _envelope_fields(raw, default_success=False, default_message=None)

        stats: GitHubSyncStats | None = None
        stats_raw = raw.get("stats")
        if isinstance(stats_raw, dict):
            stats = GitHubSyncStats(
                total=stats_raw.get("total", 0),
                inserted=stats_raw.get("inserted", 0),
                updated=stats_raw.get("updated", 0),
                reopened=stats_raw.get("reopened", 0),
                closed=stats_raw.get("closed", 0),
                no_change=stats_raw.get("no_change", 0),
                errors=stats_raw.get("errors", 0),
            )

        return GitHubSyncIssuesResponse(
            **_envelope_recovery_fields(envelope),
            stats=stats,
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

    register_github_tools(
        mcp,
        effective_profile=effective_profile,
        helpers=helpers,
        read_only_annotation=read_only_annotation,
        mutating_annotation=mutating_annotation,
    )


__all__ = [
    "SharedToolRegistrationContext",
    "ToolRegistrationContext",
    "register_full_mode_tools",
    "register_shared_tools",
]
