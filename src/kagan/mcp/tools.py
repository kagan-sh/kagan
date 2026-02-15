"""MCP tool implementations for Kagan -- CoreClientBridge over IPC."""

from __future__ import annotations

import asyncio
import json
import logging
from contextlib import suppress
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from kagan.core.ipc.client import IPCClient

logger = logging.getLogger(__name__)
_AUTH_FAILED_CODE = "AUTH_FAILED"
_SUMMARY_TEXT_LIMIT = 8_000
_FULL_TEXT_LIMIT = 32_000
_SUMMARY_DESCRIPTION_LIMIT = 2_000
_FULL_DESCRIPTION_LIMIT = 8_000
_SUMMARY_ACCEPTANCE_ITEM_LIMIT = 400
_FULL_ACCEPTANCE_ITEM_LIMIT = 1_000
_SUMMARY_ACCEPTANCE_ITEMS = 20
_FULL_ACCEPTANCE_ITEMS = 50
_SUMMARY_LOG_ENTRY_LIMIT = 2_500
_FULL_LOG_ENTRY_LIMIT = 10_000
_SUMMARY_LOG_ENTRIES = 3
_FULL_LOG_ENTRIES = 10
_SUMMARY_LOG_BUDGET = 7_500
_FULL_LOG_BUDGET = 24_000
_SUMMARY_RESPONSE_BUDGET = 12_000
_FULL_RESPONSE_BUDGET = 24_000
_SUMMARY_TITLE_LIMIT = 400
_FULL_TITLE_LIMIT = 1_000
_SUMMARY_SCRATCHPAD_FETCH_LIMIT = 6_000
_FULL_SCRATCHPAD_FETCH_LIMIT = 14_000
_SUMMARY_LOG_FETCH_ENTRY_LIMIT = 2_000
_FULL_LOG_FETCH_ENTRY_LIMIT = 6_000
_SUMMARY_LOG_FETCH_BUDGET = 6_000
_FULL_LOG_FETCH_BUDGET = 18_000
_QUERY_UNAVAILABLE_CODES = {"UNKNOWN_METHOD", "UNAUTHORIZED"}
_TASK_WAIT_IPC_DEFAULT_TIMEOUT_SECONDS = 3600.0
_TASK_WAIT_IPC_TIMEOUT_BUFFER_SECONDS = 5.0
_LOG_ENTRY_OVERHEAD_CHARS = 256
_COMPACT_ACCEPTANCE_ITEMS_FULL = 20
_COMPACT_ACCEPTANCE_ITEMS_SUMMARY = 8
_COMPACT_ACCEPTANCE_ITEM_LIMIT_FULL = 320
_COMPACT_ACCEPTANCE_ITEM_LIMIT_SUMMARY = 160
_COMPACT_RUNTIME_REASON_LIMIT_FULL = 600
_COMPACT_RUNTIME_REASON_LIMIT_SUMMARY = 240
_COMPACT_RUNTIME_HINT_LIMIT_FULL = 240
_COMPACT_RUNTIME_HINT_LIMIT_SUMMARY = 120
_COMPACT_RUNTIME_BLOCKED_IDS_FULL = 16
_COMPACT_RUNTIME_BLOCKED_IDS_SUMMARY = 8
_COMPACT_RUNTIME_OVERLAP_HINTS_FULL = 12
_COMPACT_RUNTIME_OVERLAP_HINTS_SUMMARY = 6
_MINIMAL_TITLE_LIMIT = 120
_MINIMAL_TITLE_HARD_LIMIT = 32


@dataclass(frozen=True, slots=True)
class MCPBridgeError(ValueError):
    """Structured bridge error with machine-readable code and request context."""

    code: str
    message: str
    kind: str | None = None
    capability: str | None = None
    method: str | None = None
    hint: str | None = None

    def __str__(self) -> str:
        if self.kind is not None and self.capability is not None and self.method is not None:
            return (
                f"Core {self.kind} {self.capability}.{self.method} failed "
                f"[{self.code}]: {self.message}"
            )
        return f"[{self.code}] {self.message}"

    @classmethod
    def core_failure(
        cls,
        *,
        kind: str,
        capability: str,
        method: str,
        code: str,
        message: str,
        hint: str | None = None,
    ) -> MCPBridgeError:
        return cls(
            code=code,
            message=message,
            kind=kind,
            capability=capability,
            method=method,
            hint=hint,
        )

    @classmethod
    def task_not_found(cls, task_id: str) -> MCPBridgeError:
        return cls(
            code="TASK_NOT_FOUND",
            message=f"Task not found: {task_id}",
            capability="tasks",
            method="get",
        )


class CoreClientBridge:
    """Bridge between MCP tool functions and the Kagan core via IPC.

    Each public method translates to a ``client.request()`` call against
    the core host's command/query registries.  The bridge is stateless
    apart from the client reference and session ID.
    """

    def __init__(
        self,
        client: IPCClient,
        session_id: str,
        capability_profile: str | None = None,
        session_origin: str | None = None,
        client_version: str | None = None,
    ) -> None:
        self._client = client
        self._session_id = session_id
        self._capability_profile = capability_profile
        self._session_origin = session_origin
        self._client_version = client_version
        self._recover_lock = asyncio.Lock()

    @staticmethod
    def _is_query_unavailable(exc: Exception) -> bool:
        if isinstance(exc, MCPBridgeError):
            return exc.code in _QUERY_UNAVAILABLE_CODES
        message = str(exc)
        return "[UNKNOWN_METHOD]" in message or "[UNAUTHORIZED]" in message

    @staticmethod
    def _envelope(
        raw: dict[str, Any],
        *,
        default_success: bool,
        default_message: str | None,
    ) -> dict[str, Any]:
        return {
            "success": bool(raw.get("success", default_success)),
            "message": raw.get("message", default_message),
            "code": raw.get("code"),
            "hint": raw.get("hint"),
            "next_tool": raw.get("next_tool"),
            "next_arguments": raw.get("next_arguments"),
        }

    @staticmethod
    def _normalize_mode(mode: str) -> str:
        return "full" if mode.lower() == "full" else "summary"

    @staticmethod
    def _trim_logs_to_budget(
        logs: list[dict[str, Any]], *, budget_chars: int
    ) -> list[dict[str, Any]]:
        """Keep newest log entries within an overall size budget."""
        if budget_chars <= 0 or not logs:
            return []

        trimmed_newest_first: list[dict[str, Any]] = []
        used = 0
        for log in reversed(logs):
            remaining = budget_chars - used - _LOG_ENTRY_OVERHEAD_CHARS
            if remaining <= 0:
                break

            content = str(log.get("content", ""))
            if len(content) > remaining:
                content = content[-remaining:]

            trimmed_newest_first.append(
                {
                    "run": int(log["run"]),
                    "content": content,
                    "created_at": str(log["created_at"]),
                }
            )
            used += len(content) + _LOG_ENTRY_OVERHEAD_CHARS

        return list(reversed(trimmed_newest_first))

    @staticmethod
    def _truncate_text(value: str | None, *, limit: int) -> str | None:
        if value is None or len(value) <= limit:
            return value
        omitted_chars = len(value) - limit
        return f"{value[:limit]}\n\n[truncated {omitted_chars} chars]"

    @staticmethod
    def _is_transport_oversize_error(exc: Exception) -> bool:
        if not isinstance(exc, MCPBridgeError):
            return False
        message = f"{exc.code} {exc.message}".lower()
        markers = (
            "chunk exceed the limit",
            "chunk is longer than limit",
            "separator is not found",
            "separator is found, but chunk",
        )
        return any(marker in message for marker in markers)

    @classmethod
    def _truncate_acceptance_criteria(cls, value: object, *, mode: str) -> list[str] | None:
        if not isinstance(value, list):
            return None

        item_limit = (
            _FULL_ACCEPTANCE_ITEM_LIMIT if mode == "full" else _SUMMARY_ACCEPTANCE_ITEM_LIMIT
        )
        max_items = _FULL_ACCEPTANCE_ITEMS if mode == "full" else _SUMMARY_ACCEPTANCE_ITEMS

        criteria = [cls._truncate_text(str(item), limit=item_limit) or "" for item in value]
        if len(criteria) <= max_items:
            return criteria

        omitted_count = len(criteria) - max_items
        return [*criteria[:max_items], f"[truncated {omitted_count} criteria]"]

    @classmethod
    def _fit_task_payload_budget(cls, payload: dict[str, Any], *, mode: str) -> dict[str, Any]:
        budget = _FULL_RESPONSE_BUDGET if mode == "full" else _SUMMARY_RESPONSE_BUDGET
        if cls._serialized_size(payload) <= budget:
            return payload

        trimmed = dict(payload)
        if isinstance(trimmed.get("scratchpad"), str):
            scratchpad_limit = max(0, budget // 3)
            trimmed["scratchpad"] = cls._truncate_text(
                trimmed["scratchpad"],
                limit=scratchpad_limit,
            )

        if isinstance(trimmed.get("description"), str):
            description_limit = max(0, budget // 4)
            trimmed["description"] = cls._truncate_text(
                trimmed["description"],
                limit=description_limit,
            )

        if cls._serialized_size(trimmed) <= budget:
            return trimmed

        if isinstance(trimmed.get("logs"), list):
            logs_budget = max(0, budget // 3)
            trimmed["logs"] = cls._trim_logs_to_budget(trimmed["logs"], budget_chars=logs_budget)

        if isinstance(trimmed.get("title"), str):
            title_limit = _FULL_TITLE_LIMIT if mode == "full" else _SUMMARY_TITLE_LIMIT
            trimmed["title"] = cls._truncate_text(trimmed["title"], limit=title_limit) or ""

        if isinstance(trimmed.get("runtime"), dict):
            trimmed["runtime"] = cls._compact_runtime(trimmed["runtime"], mode=mode)

        if isinstance(trimmed.get("acceptance_criteria"), list):
            if mode == "full":
                max_items = _COMPACT_ACCEPTANCE_ITEMS_FULL
                item_limit = _COMPACT_ACCEPTANCE_ITEM_LIMIT_FULL
            else:
                max_items = _COMPACT_ACCEPTANCE_ITEMS_SUMMARY
                item_limit = _COMPACT_ACCEPTANCE_ITEM_LIMIT_SUMMARY
            trimmed["acceptance_criteria"] = cls._compact_string_list(
                trimmed["acceptance_criteria"],
                max_items=max_items,
                item_limit=item_limit,
                label="criteria",
            )

        if cls._serialized_size(trimmed) <= budget:
            return trimmed

        # Last-resort safety valve for transport framing limits.
        for key in ("logs", "scratchpad", "runtime", "acceptance_criteria"):
            trimmed.pop(key, None)
            if cls._serialized_size(trimmed) <= budget:
                return trimmed

        if isinstance(trimmed.get("description"), str):
            trimmed["description"] = cls._truncate_text(
                trimmed["description"],
                limit=max(0, budget // 10),
            )
            if cls._serialized_size(trimmed) <= budget:
                return trimmed

        title_value = trimmed.get("title")
        status_value = trimmed.get("status")
        minimal = {
            "task_id": str(trimmed.get("task_id", "")),
            "title": cls._truncate_text(
                str(title_value) if title_value is not None else "",
                limit=_MINIMAL_TITLE_LIMIT,
            )
            or "",
            "status": str(status_value) if status_value is not None else "",
        }
        if cls._serialized_size(minimal) <= budget:
            return minimal
        minimal["title"] = minimal["title"][:_MINIMAL_TITLE_HARD_LIMIT]
        return minimal

    @staticmethod
    def _serialized_size(payload: dict[str, Any]) -> int:
        return len(json.dumps(payload, ensure_ascii=True, default=str))

    @classmethod
    def _compact_string_list(
        cls,
        values: list[object],
        *,
        max_items: int,
        item_limit: int,
        label: str,
    ) -> list[str]:
        normalized = [cls._truncate_text(str(value), limit=item_limit) or "" for value in values]
        if len(normalized) <= max_items:
            return normalized
        omitted = len(normalized) - max_items
        return [*normalized[:max_items], f"[truncated {omitted} {label}]"]

    @classmethod
    def _compact_runtime(cls, runtime: dict[str, Any], *, mode: str) -> dict[str, Any]:
        compact = dict(runtime)
        reason_limit = (
            _COMPACT_RUNTIME_REASON_LIMIT_FULL
            if mode == "full"
            else _COMPACT_RUNTIME_REASON_LIMIT_SUMMARY
        )
        hint_limit = (
            _COMPACT_RUNTIME_HINT_LIMIT_FULL
            if mode == "full"
            else _COMPACT_RUNTIME_HINT_LIMIT_SUMMARY
        )
        blocked_ids = (
            _COMPACT_RUNTIME_BLOCKED_IDS_FULL
            if mode == "full"
            else _COMPACT_RUNTIME_BLOCKED_IDS_SUMMARY
        )
        overlap_hints = (
            _COMPACT_RUNTIME_OVERLAP_HINTS_FULL
            if mode == "full"
            else _COMPACT_RUNTIME_OVERLAP_HINTS_SUMMARY
        )

        for key in ("blocked_reason", "pending_reason"):
            value = compact.get(key)
            if isinstance(value, str):
                compact[key] = cls._truncate_text(value, limit=reason_limit)

        blocked = compact.get("blocked_by_task_ids")
        if isinstance(blocked, list):
            compact["blocked_by_task_ids"] = cls._compact_string_list(
                blocked,
                max_items=blocked_ids,
                item_limit=hint_limit,
                label="task IDs",
            )

        hints = compact.get("overlap_hints")
        if isinstance(hints, list):
            compact["overlap_hints"] = cls._compact_string_list(
                hints,
                max_items=overlap_hints,
                item_limit=hint_limit,
                label="hints",
            )

        return compact

    async def _refresh_client_from_discovery(self) -> bool:
        from kagan.core.ipc.client import IPCClient
        from kagan.core.ipc.discovery import discover_core_endpoint

        endpoint = discover_core_endpoint()
        if endpoint is None:
            return False

        new_client = IPCClient(endpoint)
        try:
            await new_client.connect()
        except Exception:
            with suppress(Exception):
                await new_client.close()
            return False

        with suppress(Exception):
            await self._client.close()
        self._client = new_client
        return True

    async def _recover_client(self, *, refresh_endpoint: bool) -> bool:
        async with self._recover_lock:
            if refresh_endpoint:
                return await self._refresh_client_from_discovery()
            try:
                await self._client.connect()
            except Exception:
                return await self._refresh_client_from_discovery()
            return bool(self._client.is_connected)

    async def _query(
        self,
        capability: str,
        method: str,
        params: dict[str, Any] | None = None,
        *,
        request_timeout_seconds: float | None = None,
    ) -> dict[str, Any]:
        """Send a query (read-only) request to the core."""
        return await self._send_request(
            "query",
            capability,
            method,
            params=params,
            request_timeout_seconds=request_timeout_seconds,
        )

    async def _command(
        self, capability: str, method: str, params: dict[str, Any] | None = None
    ) -> dict[str, Any]:
        """Send a command (mutating) request to the core."""
        return await self._send_request("command", capability, method, params=params)

    async def _send_request(
        self,
        kind: str,
        capability: str,
        method: str,
        *,
        params: dict[str, Any] | None,
        request_timeout_seconds: float | None = None,
    ) -> dict[str, Any]:
        max_attempts = 3
        for attempt in range(max_attempts):
            try:
                request_kwargs: dict[str, Any] = {
                    "session_id": self._session_id,
                    "session_profile": self._capability_profile,
                    "session_origin": self._session_origin,
                    "capability": capability,
                    "method": method,
                    "params": params,
                }
                if self._client_version is not None:
                    request_kwargs["client_version"] = self._client_version
                if request_timeout_seconds is not None:
                    request_kwargs["request_timeout_seconds"] = request_timeout_seconds
                resp = await self._client.request(**request_kwargs)
            except ConnectionError as exc:
                if attempt < max_attempts - 1 and await self._recover_client(
                    refresh_endpoint=False
                ):
                    continue
                raise MCPBridgeError.core_failure(
                    kind=kind,
                    capability=capability,
                    method=method,
                    code="DISCONNECTED",
                    message=str(exc),
                    hint="Ensure Kagan core is running and reachable, then retry.",
                ) from exc
            except Exception as exc:
                if attempt < max_attempts - 1 and await self._recover_client(
                    refresh_endpoint=False
                ):
                    continue
                raise MCPBridgeError.core_failure(
                    kind=kind,
                    capability=capability,
                    method=method,
                    code="DISCONNECTED",
                    message=str(exc),
                    hint="Ensure Kagan core is running and reachable, then retry.",
                ) from exc

            if resp.ok:
                return resp.result or {}

            code = resp.error.code if resp.error else "UNKNOWN"
            raw_message = resp.error.message if resp.error else None
            message = str(raw_message).strip() if raw_message is not None else ""
            if not message:
                message = f"{capability}.{method} request failed"
            hint: str | None = None
            if code == _AUTH_FAILED_CODE and attempt < max_attempts - 1:
                if await self._recover_client(refresh_endpoint=True):
                    continue
                code = "AUTH_STALE_TOKEN"
                message = (
                    "MCP session token became stale after core restart; "
                    "restart MCP or reconnect client."
                )
            elif code == "MCP_OUTDATED":
                hint = (
                    "Restart the MCP client/session so it reloads the currently installed "
                    "kagan build."
                )
            raise MCPBridgeError.core_failure(
                kind=kind,
                capability=capability,
                method=method,
                code=code,
                message=message,
                hint=hint,
            )

        raise MCPBridgeError.core_failure(
            kind=kind,
            capability=capability,
            method=method,
            code="UNKNOWN",
            message="unexpected retry state",
        )

    async def get_context(self, task_id: str) -> dict:
        """Get task context for AI tools."""
        return await self._query("tasks", "context", {"task_id": task_id})

    async def get_task(
        self,
        task_id: str,
        *,
        include_scratchpad: bool | None = None,
        include_logs: bool | None = None,
        include_review: bool | None = None,
        mode: str = "summary",
    ) -> dict:
        """Get task details with optional extended context."""
        result = await self._query("tasks", "get", {"task_id": task_id})
        task_data = result.get("task")
        if not result.get("found") or task_data is None:
            raise MCPBridgeError.task_not_found(task_id)
        mode_name = self._normalize_mode(mode)
        text_limit = _FULL_TEXT_LIMIT if mode_name == "full" else _SUMMARY_TEXT_LIMIT

        response: dict[str, Any] = {
            "task_id": task_data["id"],
            "title": task_data["title"],
            "status": task_data["status"],
            "description": self._truncate_text(
                task_data.get("description"),
                limit=(
                    _FULL_DESCRIPTION_LIMIT if mode_name == "full" else _SUMMARY_DESCRIPTION_LIMIT
                ),
            ),
            "acceptance_criteria": self._truncate_acceptance_criteria(
                task_data.get("acceptance_criteria"),
                mode=mode_name,
            ),
            "runtime": task_data.get("runtime"),
        }

        if include_scratchpad:
            scratchpad_fetch_limit = (
                _FULL_SCRATCHPAD_FETCH_LIMIT
                if mode_name == "full"
                else _SUMMARY_SCRATCHPAD_FETCH_LIMIT
            )
            try:
                scratchpad_result = await self._query(
                    "tasks",
                    "scratchpad",
                    {
                        "task_id": task_id,
                        "content_char_limit": scratchpad_fetch_limit,
                    },
                )
            except MCPBridgeError as exc:
                if self._is_transport_oversize_error(exc):
                    logger.warning(
                        "tasks.scratchpad response exceeded transport budget for task %s; "
                        "returning bounded placeholder",
                        task_id,
                    )
                    response["scratchpad"] = "[omitted: scratchpad exceeded transport limits]"
                    response["scratchpad_truncated"] = True
                else:
                    raise
            else:
                response["scratchpad"] = self._truncate_text(
                    scratchpad_result.get("content"),
                    limit=text_limit,
                )
                response["scratchpad_truncated"] = bool(scratchpad_result.get("truncated", False))

        if include_logs:
            logs: list[dict[str, Any]] = []
            max_entries = _FULL_LOG_ENTRIES if mode_name == "full" else _SUMMARY_LOG_ENTRIES
            entry_limit = _FULL_LOG_ENTRY_LIMIT if mode_name == "full" else _SUMMARY_LOG_ENTRY_LIMIT
            fetch_entry_limit = (
                _FULL_LOG_FETCH_ENTRY_LIMIT
                if mode_name == "full"
                else _SUMMARY_LOG_FETCH_ENTRY_LIMIT
            )
            fetch_budget = (
                _FULL_LOG_FETCH_BUDGET if mode_name == "full" else _SUMMARY_LOG_FETCH_BUDGET
            )
            try:
                logs_result = await self._query(
                    "tasks",
                    "logs",
                    {
                        "task_id": task_id,
                        "limit": max_entries,
                        "content_char_limit": fetch_entry_limit,
                        "total_char_limit": fetch_budget,
                    },
                )
            except MCPBridgeError as exc:
                if not self._is_query_unavailable(exc):
                    if self._is_transport_oversize_error(exc):
                        logger.warning(
                            "tasks.logs response exceeded transport budget for task %s; "
                            "returning empty bounded logs",
                            task_id,
                        )
                        response["logs_truncated"] = True
                    else:
                        raise
                else:
                    logger.debug(
                        "tasks.logs unavailable; returning empty logs list for task %s", task_id
                    )
            else:
                logs = [
                    {
                        "run": int(log["run"]),
                        "content": self._truncate_text(
                            str(log["content"]),
                            limit=entry_limit,
                        )
                        or "",
                        "created_at": str(log["created_at"]),
                    }
                    for log in logs_result.get("logs", [])
                    if "run" in log and "content" in log and "created_at" in log
                ]
                if len(logs) > max_entries:
                    logs = logs[-max_entries:]
                budget = _FULL_LOG_BUDGET if mode_name == "full" else _SUMMARY_LOG_BUDGET
                logs = self._trim_logs_to_budget(logs, budget_chars=budget)
                response["logs_truncated"] = bool(logs_result.get("truncated", False))
                total_runs = logs_result.get("total_runs")
                returned_runs = logs_result.get("returned_runs")
                has_more = logs_result.get("has_more")
                next_offset = logs_result.get("next_offset")
                if isinstance(total_runs, int):
                    response["logs_total_runs"] = total_runs
                if isinstance(returned_runs, int):
                    response["logs_returned_runs"] = returned_runs
                if isinstance(has_more, bool):
                    response["logs_has_more"] = has_more
                if isinstance(next_offset, int):
                    response["logs_next_offset"] = next_offset
            response["logs"] = logs

        if include_review:
            # Review feedback not yet available via core; return None
            response["review_feedback"] = None

        return self._fit_task_payload_budget(response, mode=mode_name)

    async def list_task_logs(
        self,
        task_id: str,
        *,
        limit: int = 5,
        offset: int = 0,
        content_char_limit: int | None = None,
        total_char_limit: int | None = None,
    ) -> dict[str, Any]:
        """Get paginated task logs with transport-safe bounds."""
        params: dict[str, Any] = {
            "task_id": task_id,
            "limit": limit,
            "offset": offset,
        }
        if content_char_limit is not None:
            params["content_char_limit"] = content_char_limit
        if total_char_limit is not None:
            params["total_char_limit"] = total_char_limit
        return await self._query("tasks", "logs", params)

    async def get_scratchpad(self, task_id: str) -> str:
        """Get a task's scratchpad content."""
        result = await self._query("tasks", "scratchpad", {"task_id": task_id})
        return result.get("content", "")

    async def update_scratchpad(self, task_id: str, content: str) -> dict:
        """Append to task scratchpad."""
        raw = await self._command(
            "tasks", "update_scratchpad", {"task_id": task_id, "content": content}
        )
        return {
            **self._envelope(
                raw,
                default_success=True,
                default_message="Scratchpad updated",
            ),
            "task_id": raw.get("task_id", task_id),
        }

    async def request_review(self, task_id: str, summary: str) -> dict:
        """Mark task ready for review."""
        raw = await self._command("review", "request", {"task_id": task_id, "summary": summary})
        success = bool(raw.get("success", False))
        status = str(raw.get("status") or ("review" if success else "error"))
        message = raw.get("message", "Ready for merge" if success else "Review request failed")
        return {
            **self._envelope(raw, default_success=success, default_message=message),
            "status": status,
        }

    async def list_tasks(
        self,
        project_id: str | None = None,
        filter: str | None = None,
        exclude_task_ids: list[str] | None = None,
        include_scratchpad: bool = False,
    ) -> dict:
        """List tasks with optional coordination filters."""
        params: dict[str, Any] = {}
        if project_id:
            params["project_id"] = project_id
        if filter:
            params["filter"] = filter
        if exclude_task_ids:
            params["exclude_task_ids"] = exclude_task_ids
        if include_scratchpad:
            params["include_scratchpad"] = include_scratchpad
        return await self._query("tasks", "list", params)

    async def list_projects(self, limit: int = 10) -> dict:
        """List recent projects."""
        return await self._query("projects", "list", {"limit": limit})

    async def list_repos(self, project_id: str) -> dict:
        """List repos for a project."""
        return await self._query("projects", "repos", {"project_id": project_id})

    async def tail_audit(
        self,
        capability: str | None = None,
        limit: int = 50,
    ) -> dict:
        """List recent audit events."""
        params: dict[str, Any] = {"limit": limit}
        if capability:
            params["capability"] = capability
        return await self._query("audit", "list", params)

    async def get_settings(self) -> dict:
        """Get MCP-exposed settings snapshot."""
        return await self._query("settings", "get", {})

    async def get_instrumentation_snapshot(self) -> dict:
        """Get internal core instrumentation snapshot."""
        raw = await self._query("diagnostics", "instrumentation", {})
        data = raw.get("instrumentation")
        return dict(data) if isinstance(data, dict) else {}

    async def create_task(
        self,
        title: str,
        description: str = "",
        project_id: str | None = None,
        status: str | None = None,
        priority: str | None = None,
        task_type: str | None = None,
        terminal_backend: str | None = None,
        agent_backend: str | None = None,
        parent_id: str | None = None,
        base_branch: str | None = None,
        acceptance_criteria: list[str] | str | None = None,
        created_by: str | None = None,
    ) -> dict:
        """Create a new task."""
        params: dict[str, Any] = {"title": title, "description": description}
        if project_id:
            params["project_id"] = project_id
        if status is not None:
            params["status"] = status
        if priority is not None:
            params["priority"] = priority
        if task_type is not None:
            params["task_type"] = task_type
        if terminal_backend is not None:
            params["terminal_backend"] = terminal_backend
        if agent_backend is not None:
            params["agent_backend"] = agent_backend
        if parent_id is not None:
            params["parent_id"] = parent_id
        if base_branch is not None:
            params["base_branch"] = base_branch
        if acceptance_criteria is not None:
            params["acceptance_criteria"] = acceptance_criteria
        if created_by is not None:
            params["created_by"] = created_by
        return await self._command("tasks", "create", params)

    async def update_task(self, task_id: str, **fields: Any) -> dict:
        """Update task fields."""
        params: dict[str, Any] = {"task_id": task_id, **fields}
        return await self._command("tasks", "update", params)

    async def move_task(self, task_id: str, status: str) -> dict:
        """Move task to new status column."""
        return await self._command("tasks", "move", {"task_id": task_id, "status": status})

    async def wait_task(
        self,
        task_id: str,
        *,
        timeout_seconds: float | str | None = None,
        wait_for_status: list[str] | str | None = None,
        from_updated_at: str | None = None,
    ) -> dict:
        """Block until target task changes or timeout elapses."""
        params: dict[str, Any] = {"task_id": task_id}
        effective_wait_timeout = _TASK_WAIT_IPC_DEFAULT_TIMEOUT_SECONDS
        if timeout_seconds is not None:
            if isinstance(timeout_seconds, str):
                normalized_timeout = timeout_seconds.strip()
                if normalized_timeout:
                    with suppress(ValueError):
                        parsed_timeout = float(normalized_timeout)
                        params["timeout_seconds"] = parsed_timeout
                        if parsed_timeout > 0:
                            effective_wait_timeout = parsed_timeout
                if "timeout_seconds" not in params:
                    params["timeout_seconds"] = timeout_seconds
            else:
                params["timeout_seconds"] = timeout_seconds
                if timeout_seconds > 0:
                    effective_wait_timeout = timeout_seconds
        if wait_for_status is not None:
            params["wait_for_status"] = wait_for_status
        if from_updated_at is not None:
            params["from_updated_at"] = from_updated_at
        return await self._query(
            "tasks",
            "wait",
            params,
            request_timeout_seconds=effective_wait_timeout + _TASK_WAIT_IPC_TIMEOUT_BUFFER_SECONDS,
        )

    async def submit_job(
        self,
        *,
        task_id: str,
        action: str,
        arguments: dict[str, object] | None = None,
    ) -> dict:
        """Submit a core job for asynchronous execution."""
        params: dict[str, Any] = {"task_id": task_id, "action": action}
        if arguments is not None:
            params["arguments"] = arguments
        return await self._command("jobs", "submit", params)

    async def get_job(self, *, job_id: str, task_id: str) -> dict:
        """Read current status for a submitted core job."""
        return await self._query("jobs", "get", {"job_id": job_id, "task_id": task_id})

    async def wait_job(
        self,
        *,
        job_id: str,
        task_id: str,
        timeout_seconds: float | None = None,
    ) -> dict:
        """Wait for a job to reach terminal status or timeout."""
        params: dict[str, Any] = {"job_id": job_id, "task_id": task_id}
        if timeout_seconds is not None:
            params["timeout_seconds"] = timeout_seconds
        return await self._query("jobs", "wait", params)

    async def list_job_events(
        self,
        *,
        job_id: str,
        task_id: str,
        limit: int = 50,
        offset: int = 0,
    ) -> dict:
        """List paginated events emitted for a submitted core job."""
        return await self._query(
            "jobs",
            "events",
            {"job_id": job_id, "task_id": task_id, "limit": limit, "offset": offset},
        )

    async def cancel_job(self, *, job_id: str, task_id: str) -> dict:
        """Cancel a submitted core job."""
        return await self._command("jobs", "cancel", {"job_id": job_id, "task_id": task_id})

    async def create_session(
        self,
        task_id: str,
        *,
        reuse_if_exists: bool = True,
        worktree_path: str | None = None,
    ) -> dict:
        """Create/reuse a PAIR session and return handoff instructions."""
        params: dict[str, Any] = {"task_id": task_id, "reuse_if_exists": reuse_if_exists}
        if worktree_path is not None:
            params["worktree_path"] = worktree_path
        return await self._command("sessions", "create", params)

    async def session_exists(self, task_id: str) -> dict:
        """Check whether a PAIR session exists for a task."""
        return await self._command("sessions", "exists", {"task_id": task_id})

    async def kill_session(self, task_id: str) -> dict:
        """Terminate a PAIR session for a task."""
        return await self._command("sessions", "kill", {"task_id": task_id})

    async def delete_task(self, task_id: str) -> dict:
        """Delete a task."""
        return await self._command("tasks", "delete", {"task_id": task_id})

    async def open_project(self, project_id: str) -> dict:
        """Open/switch to a project."""
        return await self._command("projects", "open", {"project_id": project_id})

    async def create_project(
        self,
        name: str,
        description: str = "",
        repo_paths: list[str] | None = None,
    ) -> dict:
        """Create a new project with optional repositories."""
        params: dict[str, Any] = {"name": name, "description": description}
        if repo_paths:
            params["repo_paths"] = repo_paths
        return await self._command("projects", "create", params)

    async def review_action(
        self,
        task_id: str,
        action: str,
        feedback: str = "",
        rejection_action: str = "reopen",
    ) -> dict:
        """Execute a review action (approve, reject, merge, rebase)."""
        params: dict[str, Any] = {"task_id": task_id}
        if action == "reject":
            params["feedback"] = feedback
            params["action"] = rejection_action
        return await self._command("review", action, params)

    async def update_settings(self, fields: dict[str, Any]) -> dict:
        """Update allowlisted settings fields."""
        return await self._command("settings", "update", {"fields": fields})

    # -------------------------------------------------------------------------
    # Plugin Operations (Generic Dispatch)
    # -------------------------------------------------------------------------

    async def invoke_plugin(
        self,
        capability: str,
        method: str,
        params: dict[str, Any] | None = None,
    ) -> dict:
        """Invoke a plugin operation by capability and method.

        Args:
            capability: Plugin capability namespace.
            method: Operation method name.
            params: Optional parameters dict.

        Returns:
            Plugin operation result dict.
        """
        return await self._command(capability, method, params or {})


def _format_review_feedback(review_result: object) -> str | None:
    """Format review result dict into a human-readable string."""
    if not isinstance(review_result, dict):
        return None
    summary = str(review_result.get("summary") or "").strip()
    status = review_result.get("status")
    approved = review_result.get("approved")
    if status is None and isinstance(approved, bool):
        status = "approved" if approved else "rejected"
    status_label = str(status).strip().lower() if status is not None else ""
    if summary:
        return f"{status_label}: {summary}" if status_label else summary
    if status_label:
        return f"Review {status_label}."
    return None


__all__ = ["CoreClientBridge", "MCPBridgeError", "_format_review_feedback"]
