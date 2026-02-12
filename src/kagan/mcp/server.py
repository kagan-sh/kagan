"""FastMCP server setup for Kagan.

Uses MCP SDK best practices:
- Lifespan for resource management (IPCClient, CoreClientBridge)
- Context injection for built-in logging
- Pydantic models for structured responses
- Progress reporting for long operations
"""

from __future__ import annotations

import logging
from contextlib import asynccontextmanager, suppress
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, TypedDict

from mcp.server.fastmcp import Context, FastMCP
from mcp.server.session import ServerSession
from mcp.types import ToolAnnotations

from kagan.core.config import KaganConfig
from kagan.core.constants import (
    MCP_DEFAULT_FULL_CAPABILITY,
    MCP_DEFAULT_READONLY_CAPABILITY,
    MCP_DEFAULT_SESSION_ID,
    MCP_FALLBACK_CAPABILITY,
    MCP_IDENTITY_ADMIN,
    MCP_IDENTITY_DEFAULT,
)
from kagan.core.ipc.client import IPCClient
from kagan.core.ipc.discovery import CoreEndpoint, discover_core_endpoint
from kagan.core.launcher import ensure_core_running
from kagan.core.mcp_naming import get_mcp_server_name
from kagan.core.paths import get_config_path, get_core_token_path, get_database_path
from kagan.core.security import (
    CAPABILITY_PROFILES,
    CapabilityMethod,
    CapabilityProfile,
    ProtocolCapability,
    ProtocolMethod,
    protocol_call,
)
from kagan.mcp.models import (
    TaskRuntimeState,
)
from kagan.mcp.registrars import (
    JOB_CODE_JOB_TIMEOUT,
    JOB_CODE_NOT_RUNNING,
    JOB_CODE_START_BLOCKED,
    JOB_CODE_START_PENDING,
    JOB_CODE_TASK_TYPE_MISMATCH,
    JOB_NON_TERMINAL_STATUSES,
    JOB_TERMINAL_STATUSES,
    TASK_TYPE_AUTO,
    TASK_TYPE_VALUES,
    TOOL_GET_TASK,
    TOOL_JOBS_WAIT,
    TOOL_TASKS_UPDATE,
    SharedToolRegistrationContext,
    TaskStatusInput,
    TaskTypeInput,
    ToolRegistrationContext,
    register_full_mode_tools,
    register_shared_tools,
)
from kagan.mcp.tools import CoreClientBridge

if TYPE_CHECKING:
    from collections.abc import AsyncIterator, Callable

logger = logging.getLogger(__name__)

NO_SESSION_MESSAGE = (
    "No active Kagan session. "
    "This MCP server is registered globally but Kagan is not managing the current directory. "
    "Run 'kagan' to start a session, or use this tool from a Kagan-managed project."
)


@dataclass(frozen=True, slots=True)
class MCPStartupError(RuntimeError):
    """Structured startup error for deterministic core-availability failures."""

    code: str
    message: str
    hint: str

    def __str__(self) -> str:
        return f"[{self.code}] {self.message} Hint: {self.hint}"

    def as_dict(self) -> dict[str, str]:
        return {
            "code": self.code,
            "message": self.message,
            "hint": self.hint,
        }


@dataclass(frozen=True, slots=True)
class MCPRuntimeConfig:
    """Runtime configuration for endpoint/session overrides in MCP server process."""

    endpoint: str | None = None
    session_id: str | None = None
    capability_profile: str | None = None
    identity: str | None = None
    enable_internal_instrumentation: bool = False


@dataclass
class MCPLifespanContext:
    """Context available during MCP server lifetime via lifespan.

    Startup fails fast when core is unavailable, so context always has an
    active IPC client and bridge.
    """

    bridge: CoreClientBridge
    client: IPCClient


# Type alias for our Context with lifespan
MCPContext = Context[ServerSession, MCPLifespanContext]
_PROFILE_RANK: dict[CapabilityProfile, int] = {
    CapabilityProfile.VIEWER: 0,
    CapabilityProfile.PLANNER: 1,
    CapabilityProfile.PAIR_WORKER: 2,
    CapabilityProfile.OPERATOR: 3,
    CapabilityProfile.MAINTAINER: 4,
}
_IDENTITY_PROFILE_CEILING: dict[str, CapabilityProfile] = {
    MCP_IDENTITY_DEFAULT: CapabilityProfile.PAIR_WORKER,
    MCP_IDENTITY_ADMIN: CapabilityProfile.MAINTAINER,
}
_SETTINGS_UPDATE_FIELD_MAP: dict[str, str] = {
    "auto_review": "general.auto_review",
    "auto_approve": "general.auto_approve",
    "require_review_approval": "general.require_review_approval",
    "serialize_merges": "general.serialize_merges",
    "default_base_branch": "general.default_base_branch",
    "max_concurrent_agents": "general.max_concurrent_agents",
    "default_worker_agent": "general.default_worker_agent",
    "default_pair_terminal_backend": "general.default_pair_terminal_backend",
    "default_model_claude": "general.default_model_claude",
    "default_model_opencode": "general.default_model_opencode",
    "default_model_codex": "general.default_model_codex",
    "default_model_gemini": "general.default_model_gemini",
    "default_model_kimi": "general.default_model_kimi",
    "default_model_copilot": "general.default_model_copilot",
    "skip_pair_instructions": "ui.skip_pair_instructions",
}

_READ_ONLY = ToolAnnotations(readOnlyHint=True, destructiveHint=False, idempotentHint=True)
_MUTATING = ToolAnnotations(readOnlyHint=False, destructiveHint=False, idempotentHint=False)
_DESTRUCTIVE = ToolAnnotations(readOnlyHint=False, destructiveHint=True, idempotentHint=False)


def _runtime_config_or_default(runtime_config: MCPRuntimeConfig | None) -> MCPRuntimeConfig:
    """Normalize optional runtime config to a concrete value object."""
    return runtime_config if runtime_config is not None else MCPRuntimeConfig()


def _resolve_endpoint(runtime_config: MCPRuntimeConfig | None = None) -> CoreEndpoint | None:
    """Resolve the core endpoint from override or discovery."""
    config = _runtime_config_or_default(runtime_config)
    discovered = discover_core_endpoint()
    if not config.endpoint:
        return discovered

    raw = config.endpoint.strip()

    if raw.startswith("tcp://"):
        host_port = raw.removeprefix("tcp://")
        host, sep, port_raw = host_port.rpartition(":")
        if sep and port_raw.isdigit():
            return CoreEndpoint(
                transport="tcp",
                address=host,
                port=int(port_raw),
                token=_read_local_core_token(discovered),
            )
        return CoreEndpoint(
            transport="tcp",
            address=host_port,
            port=None,
            token=_read_local_core_token(discovered),
        )
    if raw.startswith("socket://"):
        return CoreEndpoint(
            transport="socket",
            address=raw.removeprefix("socket://"),
            token=_read_local_core_token(discovered),
        )

    if raw.startswith("pipe://") or raw.startswith("\\\\.\\pipe\\"):
        return None
    if raw.startswith("/") or raw.endswith(".sock"):
        return CoreEndpoint(
            transport="socket",
            address=raw,
            token=_read_local_core_token(discovered),
        )

    host, sep, port_raw = raw.rpartition(":")
    if sep and port_raw.isdigit():
        return CoreEndpoint(
            transport="tcp",
            address=host,
            port=int(port_raw),
            token=_read_local_core_token(discovered),
        )
    return CoreEndpoint(
        transport="socket",
        address=raw,
        token=_read_local_core_token(discovered),
    )


def _read_local_core_token(discovered: CoreEndpoint | None) -> str | None:
    """Read token from discovered endpoint or local runtime file."""
    if discovered is not None and discovered.token:
        return discovered.token
    with suppress(OSError):
        token = get_core_token_path().read_text(encoding="utf-8").strip()
        if token:
            return token
    return None


def _is_core_autostart_enabled() -> bool:
    """Return whether MCP should auto-start core when no endpoint is available."""
    try:
        config = KaganConfig.load(get_config_path())
    except Exception:
        return True
    return config.general.core_autostart


async def _resolve_or_autostart_endpoint(
    runtime_config: MCPRuntimeConfig | None = None,
) -> CoreEndpoint | None:
    """Resolve endpoint, auto-starting core when discovery fails and autostart is enabled."""
    config = _runtime_config_or_default(runtime_config)
    endpoint = _resolve_endpoint(config)
    if endpoint is not None:
        return endpoint
    if config.endpoint:
        return None
    if not _is_core_autostart_enabled():
        return None
    try:
        return await ensure_core_running(
            config_path=get_config_path(),
            db_path=get_database_path(),
        )
    except Exception:
        logger.warning("Failed to auto-start core endpoint", exc_info=True)
        return None


def _endpoint_description(endpoint: CoreEndpoint) -> str:
    if endpoint.transport == "tcp" and endpoint.port is not None:
        return f"tcp://{endpoint.address}:{endpoint.port}"
    return f"{endpoint.transport}://{endpoint.address}"


@asynccontextmanager
async def _mcp_lifespan(
    mcp: FastMCP,
    runtime_config: MCPRuntimeConfig | None = None,
) -> AsyncIterator[MCPLifespanContext]:
    """Lifespan manager for MCP server resources.

    Discovers the core endpoint (or uses the CLI override), connects an
    IPCClient, and wraps it in a CoreClientBridge.

    Startup fails when core is unavailable so MCP cannot bypass core
    coordination with degraded behavior.
    """
    config = _runtime_config_or_default(runtime_config)
    endpoint = await _resolve_or_autostart_endpoint(config)

    if endpoint is None:
        if config.endpoint:
            raise MCPStartupError(
                code="NO_ENDPOINT",
                message=f"Configured core endpoint override is unavailable: {config.endpoint!r}.",
                hint=(
                    "Provide a reachable tcp://host:port or socket://path endpoint, or "
                    "remove --endpoint and run `kagan core start`."
                ),
            )
        raise MCPStartupError(
            code="NO_ENDPOINT",
            message="No active Kagan core endpoint was discovered.",
            hint="Start Kagan or run `kagan core start`, then reconnect MCP.",
        )

    session_id = config.session_id or MCP_DEFAULT_SESSION_ID
    capability_profile = config.capability_profile or MCP_FALLBACK_CAPABILITY
    session_origin = config.identity or MCP_IDENTITY_DEFAULT
    client = IPCClient(endpoint)

    try:
        await client.connect()
    except Exception as exc:
        logger.warning(
            "Failed to connect to core at %s",
            _endpoint_description(endpoint),
            exc_info=True,
        )
        raise MCPStartupError(
            code="DISCONNECTED",
            message=(f"Kagan core is unreachable at {_endpoint_description(endpoint)}: {exc}"),
            hint="Ensure core is running and reachable, then reconnect MCP.",
        ) from exc

    bridge = CoreClientBridge(
        client,
        session_id,
        capability_profile=capability_profile,
        session_origin=session_origin,
    )
    try:
        yield MCPLifespanContext(bridge=bridge, client=client)
    finally:
        await client.close()


def _get_bridge(ctx: MCPContext) -> CoreClientBridge | None:
    """Extract CoreClientBridge from request context.

    Returns ``None`` when no lifespan context is present.
    """
    lifespan_ctx = ctx.request_context.lifespan_context
    if lifespan_ctx is None:
        return None
    return lifespan_ctx.bridge


def _require_bridge(ctx: MCPContext | None) -> CoreClientBridge:
    """Return an active bridge or raise a user-facing no-session error."""
    if ctx is None:
        raise ValueError(f"[NO_CONTEXT] {NO_SESSION_MESSAGE}")
    bridge = _get_bridge(ctx)
    if bridge is None:
        raise ValueError(f"[NO_SESSION] {NO_SESSION_MESSAGE}")
    return bridge


def _build_server_instructions(readonly: bool) -> str:
    """Build MCP server instructions tailored to readonly vs full mode."""
    base = [
        "Kagan is a Kanban-style task management system for AI-assisted development.",
        "",
        "The task_id is provided in your system prompt when Kagan assigns you work.",
        "Use get_task to inspect any task (with include_logs=true for execution history).",
        "Use tasks_list to coordinate with other agents.",
        "Important: status is Kanban column, task_type is execution mode (AUTO/PAIR).",
        "Use jobs_submit/jobs_wait/jobs_get/jobs_events/jobs_cancel for async automation control.",
        "If a tool returns next_tool/next_arguments, use them for deterministic recovery.",
    ]
    if readonly:
        base.extend(
            [
                "",
                "You are running in READ-ONLY mode.",
                "Available tools are capability-scoped read-only/planning tools.",
            ]
        )
    else:
        base.extend(
            [
                "",
                "When assigned a task, follow this workflow:",
                "1. Call get_context with your task_id to get requirements and codebase context",
                "2. Use update_scratchpad to record progress, decisions, and blockers",
                "3. Call jobs_list_actions to discover valid job actions.",
                "4. For automation runs: set task_type='AUTO', then call jobs_submit.",
                "5. Track progress with jobs_wait/jobs_get and inspect timeline with jobs_events.",
                "6. Use jobs_cancel only to stop in-flight work.",
                "7. Call request_review when implementation is complete",
            ]
        )
    return "\n".join(base)


def _runtime_state_from_raw(raw: dict[str, Any] | None) -> TaskRuntimeState | None:
    """Build TaskRuntimeState from raw response payload when present."""
    if raw is None or not isinstance(raw, dict):
        return None
    return TaskRuntimeState(
        is_running=bool(raw.get("is_running", False)),
        is_reviewing=bool(raw.get("is_reviewing", False)),
        is_blocked=bool(raw.get("is_blocked", False)),
        blocked_reason=raw.get("blocked_reason"),
        blocked_by_task_ids=[str(task_id) for task_id in raw.get("blocked_by_task_ids", [])],
        overlap_hints=[str(hint) for hint in raw.get("overlap_hints", [])],
        blocked_at=raw.get("blocked_at"),
        is_pending=bool(raw.get("is_pending", False)),
        pending_reason=raw.get("pending_reason"),
        pending_at=raw.get("pending_at"),
    )


def _resolve_effective_profile(
    default_profile: CapabilityProfile,
    default_identity: str,
    runtime_config: MCPRuntimeConfig | None = None,
) -> str:
    """Resolve effective capability profile after identity ceiling."""
    config = _runtime_config_or_default(runtime_config)
    raw_profile = (config.capability_profile or str(default_profile)).strip().lower()
    raw_identity = (config.identity or default_identity).strip().lower()
    try:
        requested = CapabilityProfile(raw_profile)
    except ValueError:
        requested = CapabilityProfile.VIEWER
    ceiling = _IDENTITY_PROFILE_CEILING.get(raw_identity, CapabilityProfile.PAIR_WORKER)
    effective = requested if _PROFILE_RANK[requested] <= _PROFILE_RANK[ceiling] else ceiling
    return str(effective)


def _is_allowed(
    profile: str,
    capability: ProtocolCapability | str,
    method: ProtocolMethod | str,
) -> bool:
    """Return whether profile may call capability.method."""
    if profile == str(CapabilityProfile.MAINTAINER):
        return True
    try:
        normalized_profile = CapabilityProfile(profile)
    except ValueError:
        return False
    return protocol_call(capability, method) in CAPABILITY_PROFILES.get(
        normalized_profile, frozenset()
    )


def _normalized_mode(value: str | None) -> str | None:
    """Return normalized task mode token (AUTO/PAIR) when value matches."""
    if value is None:
        return None
    normalized = value.strip().upper()
    if normalized in TASK_TYPE_VALUES:
        return normalized
    return None


def _normalize_status_task_type_inputs(
    *,
    status: TaskStatusInput | None,
    task_type: TaskTypeInput | None,
) -> tuple[str | None, str | None, str | None]:
    """Normalize common status-vs-task_type confusion.

    When callers pass status=AUTO/PAIR, interpret that as task_type if possible.
    Returns: (normalized_status, normalized_task_type, normalization_message).
    """
    mode_from_status = _normalized_mode(str(status) if status is not None else None)
    normalized_task_type = _normalized_mode(str(task_type) if task_type is not None else None)
    if mode_from_status is None:
        normalized_status = str(status) if status is not None else None
        normalized_task_type_str = str(task_type) if task_type is not None else None
        return normalized_status, normalized_task_type_str, None
    if normalized_task_type and normalized_task_type != mode_from_status:
        message = (
            f"Ignored status={status!r} because task_type={task_type!r} is already set. "
            "Use status for BACKLOG/IN_PROGRESS/REVIEW/DONE only."
        )
        return None, normalized_task_type, message
    message = (
        f"Interpreted status={status!r} as task_type={mode_from_status!r}. "
        "Use status for BACKLOG/IN_PROGRESS/REVIEW/DONE only."
    )
    return None, mode_from_status, message


def _envelope_fields(
    raw: dict[str, object],
    *,
    default_success: bool,
    default_message: str | None = None,
) -> MutatingEnvelope:
    """Extract common mutating-tool envelope fields from a core response."""
    return MutatingEnvelope(
        success=bool(raw.get("success", default_success)),
        message=_coerce_string_field(raw, "message", default=default_message),
        code=_coerce_string_field(raw, "code"),
        hint=_coerce_string_field(raw, "hint"),
        next_tool=_coerce_string_field(raw, "next_tool"),
        next_arguments=_dict_or_none(raw.get("next_arguments")),
    )


def _envelope_with_code_override(
    raw: dict[str, object],
    *,
    default_success: bool,
    default_message: str | None,
    fallback_code: str | None,
) -> MutatingEnvelope:
    envelope = _envelope_fields(
        raw, default_success=default_success, default_message=default_message
    )
    if envelope.code is not None:
        return envelope
    return MutatingEnvelope(
        success=envelope.success,
        message=envelope.message,
        code=fallback_code,
        hint=envelope.hint,
        next_tool=envelope.next_tool,
        next_arguments=envelope.next_arguments,
    )


@dataclass(frozen=True)
class MutatingEnvelope:
    success: bool
    message: str | None
    code: str | None
    hint: str | None
    next_tool: str | None
    next_arguments: dict[str, object] | None


class _EnvelopeStatusFields(TypedDict):
    success: bool
    message: str | None
    code: str | None


class _EnvelopeRecoveryFields(_EnvelopeStatusFields):
    hint: str | None
    next_tool: str | None
    next_arguments: dict[str, object] | None


def _envelope_status_fields(envelope: MutatingEnvelope) -> _EnvelopeStatusFields:
    return {
        "success": envelope.success,
        "message": envelope.message,
        "code": envelope.code,
    }


def _envelope_recovery_fields(envelope: MutatingEnvelope) -> _EnvelopeRecoveryFields:
    return {
        "success": envelope.success,
        "message": envelope.message,
        "code": envelope.code,
        "hint": envelope.hint,
        "next_tool": envelope.next_tool,
        "next_arguments": envelope.next_arguments,
    }


def _project_settings_update_fields(values: dict[str, object | None]) -> dict[str, object]:
    """Map tool argument names to allowlisted dotted settings paths."""
    fields: dict[str, object] = {}
    for input_name, dotted_path in _SETTINGS_UPDATE_FIELD_MAP.items():
        value = values.get(input_name)
        if value is not None:
            fields[dotted_path] = value
    return fields


def _dict_or_none(value: object) -> dict[str, object] | None:
    if isinstance(value, dict):
        return {str(key): value[key] for key in value}
    return None


def _str_or_none(value: object) -> str | None:
    if isinstance(value, str):
        return value
    return None


def _coerce_string_field(
    raw: dict[str, object],
    key: str,
    *,
    default: str | None = None,
) -> str | None:
    value = raw.get(key)
    if value is None:
        return default
    if isinstance(value, str):
        return value
    return str(value)


def _derive_job_get_recovery(
    *,
    job_id: str,
    task_id: str,
    status: str | None,
    code: str | None,
    timed_out: bool | None,
    runtime: dict[str, object] | None,
) -> tuple[str | None, dict[str, object] | None, str | None]:
    """Provide deterministic recovery guidance when core omits next_tool hints."""
    normalized_status = status.lower() if isinstance(status, str) else None
    if timed_out or code == JOB_CODE_JOB_TIMEOUT:
        return (
            TOOL_JOBS_WAIT,
            {"job_id": job_id, "task_id": task_id, "timeout_seconds": 1.5},
            "Wait timed out before terminal status. Call jobs_wait again.",
        )
    if normalized_status in JOB_NON_TERMINAL_STATUSES:
        return (
            TOOL_JOBS_WAIT,
            {"job_id": job_id, "task_id": task_id, "timeout_seconds": 1.5},
            "Job is still in progress. Call jobs_wait until status is terminal.",
        )
    if code == JOB_CODE_TASK_TYPE_MISMATCH:
        return (
            TOOL_TASKS_UPDATE,
            {"task_id": task_id, "task_type": TASK_TYPE_AUTO},
            "Set task_type to AUTO before resubmitting jobs_submit.",
        )
    if code == JOB_CODE_START_BLOCKED:
        blocked_ids_raw: list[object] = []
        if runtime is not None:
            raw_value = runtime.get("blocked_by_task_ids", [])
            if isinstance(raw_value, list | tuple):
                blocked_ids_raw = list(raw_value)
        blocked_ids = [str(value) for value in blocked_ids_raw if str(value).strip()]
        if blocked_ids:
            return (
                TOOL_GET_TASK,
                {"task_id": blocked_ids[0], "mode": "summary"},
                "Resolve the blocking task first, then resubmit jobs_submit.",
            )
        return (
            TOOL_GET_TASK,
            {"task_id": task_id, "mode": "summary"},
            "Inspect runtime details and retry after the blocking condition clears.",
        )
    if code == JOB_CODE_START_PENDING:
        return (
            TOOL_JOBS_WAIT,
            {"job_id": job_id, "task_id": task_id, "timeout_seconds": 1.5},
            "Start is pending scheduler admission. Keep calling jobs_wait until it resolves.",
        )
    if code == JOB_CODE_NOT_RUNNING:
        return (
            TOOL_GET_TASK,
            {"task_id": task_id, "include_logs": True, "mode": "summary"},
            "Agent is not running. Inspect latest runtime/logs before retrying.",
        )
    if normalized_status in JOB_TERMINAL_STATUSES:
        return None, None, None
    return None, None, None


def _create_mcp_server(
    readonly: bool = False,
    runtime_config: MCPRuntimeConfig | None = None,
) -> FastMCP:
    """Create FastMCP instance with lifespan and tools."""
    config = _runtime_config_or_default(runtime_config)
    default_profile = CapabilityProfile.PLANNER if readonly else CapabilityProfile.MAINTAINER
    default_identity = MCP_IDENTITY_DEFAULT if readonly else MCP_IDENTITY_ADMIN
    effective_profile = _resolve_effective_profile(
        default_profile,
        default_identity,
        runtime_config=config,
    )

    def allows_all(*pairs: CapabilityMethod) -> bool:
        return all(
            _is_allowed(effective_profile, capability, method) for capability, method in pairs
        )

    def allows_any(*pairs: CapabilityMethod) -> bool:
        return any(
            _is_allowed(effective_profile, capability, method) for capability, method in pairs
        )

    @asynccontextmanager
    async def lifespan(mcp_instance: FastMCP) -> AsyncIterator[MCPLifespanContext]:
        async with _mcp_lifespan(mcp_instance, config) as lifespan_ctx:
            yield lifespan_ctx

    mcp = FastMCP(
        get_mcp_server_name(),
        instructions=_build_server_instructions(readonly),
        lifespan=lifespan,
    )

    _register_shared_tools(mcp, allows_all=allows_all, effective_profile=effective_profile)

    if not readonly:
        _register_full_mode_tools(
            mcp,
            allows_all=allows_all,
            allows_any=allows_any,
            effective_profile=effective_profile,
            enable_internal_instrumentation=config.enable_internal_instrumentation,
        )

    return mcp


def _register_shared_tools(
    mcp: FastMCP,
    *,
    allows_all: Callable[..., bool],
    effective_profile: str,
) -> None:
    """Register planner/read-only/shared MCP tools."""
    register_shared_tools(
        mcp,
        allows_all=allows_all,
        effective_profile=effective_profile,
        helpers=SharedToolRegistrationContext(
            require_bridge=_require_bridge,
            runtime_state_from_raw=_runtime_state_from_raw,
        ),
        read_only_annotation=_READ_ONLY,
        mutating_annotation=_MUTATING,
    )


def _register_full_mode_tools(
    mcp: FastMCP,
    *,
    allows_all: Callable[..., bool],
    allows_any: Callable[..., bool],
    effective_profile: str,
    enable_internal_instrumentation: bool,
) -> None:
    """Register mutating/full-mode-only MCP tools."""
    register_full_mode_tools(
        mcp,
        allows_all=allows_all,
        allows_any=allows_any,
        effective_profile=effective_profile,
        enable_internal_instrumentation=enable_internal_instrumentation,
        helpers=ToolRegistrationContext(
            require_bridge=_require_bridge,
            runtime_state_from_raw=_runtime_state_from_raw,
            normalize_status_task_type_inputs=_normalize_status_task_type_inputs,
            envelope_fields=_envelope_fields,
            envelope_with_code_override=_envelope_with_code_override,
            envelope_status_fields=_envelope_status_fields,
            envelope_recovery_fields=_envelope_recovery_fields,
            project_settings_update_fields=_project_settings_update_fields,
            normalized_mode=_normalized_mode,
            derive_job_get_recovery=_derive_job_get_recovery,
            str_or_none=_str_or_none,
            dict_or_none=_dict_or_none,
            is_allowed=_is_allowed,
        ),
        read_only_annotation=_READ_ONLY,
        mutating_annotation=_MUTATING,
        destructive_annotation=_DESTRUCTIVE,
    )


def list_registered_tool_names(mcp: FastMCP) -> set[str]:
    """Return registered MCP tool names for contract tests and diagnostics."""
    return {tool.name for tool in mcp._tool_manager.list_tools()}


def get_registered_tool_annotations(mcp: FastMCP) -> dict[str, ToolAnnotations | None]:
    """Return tool annotation metadata keyed by tool name."""
    return {tool.name: tool.annotations for tool in mcp._tool_manager.list_tools()}


def main(
    readonly: bool = False,
    endpoint: str | None = None,
    session_id: str | None = None,
    capability: str | None = None,
    identity: str | None = None,
    enable_internal_instrumentation: bool = False,
) -> None:
    """Entry point for kagan-mcp command."""
    runtime_config = MCPRuntimeConfig(
        endpoint=endpoint,
        session_id=session_id,
        capability_profile=capability
        or (MCP_DEFAULT_READONLY_CAPABILITY if readonly else MCP_DEFAULT_FULL_CAPABILITY),
        identity=identity or (MCP_IDENTITY_DEFAULT if readonly else MCP_IDENTITY_ADMIN),
        enable_internal_instrumentation=enable_internal_instrumentation,
    )
    mcp = _create_mcp_server(readonly=readonly, runtime_config=runtime_config)
    try:
        mcp.run(transport="stdio")
    except MCPStartupError as exc:
        raise SystemExit(str(exc)) from exc
