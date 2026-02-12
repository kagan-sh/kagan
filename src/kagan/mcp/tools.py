"""MCP tool implementations for Kagan -- CoreClientBridge over IPC."""

from __future__ import annotations

import asyncio
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
_SUMMARY_LOG_ENTRY_LIMIT = 2_500
_FULL_LOG_ENTRY_LIMIT = 10_000
_SUMMARY_LOG_ENTRIES = 3
_FULL_LOG_ENTRIES = 10
_SUMMARY_LOG_BUDGET = 7_500
_FULL_LOG_BUDGET = 24_000
_QUERY_UNAVAILABLE_CODES = {"UNKNOWN_METHOD", "UNAUTHORIZED"}


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
    ) -> None:
        self._client = client
        self._session_id = session_id
        self._capability_profile = capability_profile
        self._session_origin = session_origin
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
        per_entry_overhead = 256
        for log in reversed(logs):
            remaining = budget_chars - used - per_entry_overhead
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
            used += len(content) + per_entry_overhead

        return list(reversed(trimmed_newest_first))

    @staticmethod
    def _truncate_text(value: str | None, *, limit: int) -> str | None:
        if value is None or len(value) <= limit:
            return value
        omitted_chars = len(value) - limit
        return f"{value[:limit]}\n\n[truncated {omitted_chars} chars]"

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
        self, capability: str, method: str, params: dict[str, Any] | None = None
    ) -> dict[str, Any]:
        """Send a query (read-only) request to the core."""
        return await self._send_request("query", capability, method, params=params)

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
    ) -> dict[str, Any]:
        max_attempts = 3
        for attempt in range(max_attempts):
            try:
                resp = await self._client.request(
                    session_id=self._session_id,
                    session_profile=self._capability_profile,
                    session_origin=self._session_origin,
                    capability=capability,
                    method=method,
                    params=params,
                )
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

            if resp.ok:
                return resp.result or {}

            code = resp.error.code if resp.error else "UNKNOWN"
            message = resp.error.message if resp.error else "Unknown error"
            if code == _AUTH_FAILED_CODE and attempt < max_attempts - 1:
                if await self._recover_client(refresh_endpoint=True):
                    continue
                code = "AUTH_STALE_TOKEN"
                message = (
                    "MCP session token became stale after core restart; "
                    "restart MCP or reconnect client."
                )
            raise MCPBridgeError.core_failure(
                kind=kind,
                capability=capability,
                method=method,
                code=code,
                message=message,
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
            "description": task_data.get("description"),
            "acceptance_criteria": task_data.get("acceptance_criteria"),
            "runtime": task_data.get("runtime"),
        }

        if include_scratchpad:
            scratchpad_result = await self._query("tasks", "scratchpad", {"task_id": task_id})
            response["scratchpad"] = self._truncate_text(
                scratchpad_result.get("content"),
                limit=text_limit,
            )

        if include_logs:
            logs: list[dict[str, Any]] = []
            try:
                logs_result = await self._query("tasks", "logs", {"task_id": task_id})
            except MCPBridgeError as exc:
                if not self._is_query_unavailable(exc):
                    raise
                logger.debug(
                    "tasks.logs unavailable; returning empty logs list for task %s", task_id
                )
            else:
                max_entries = _FULL_LOG_ENTRIES if mode_name == "full" else _SUMMARY_LOG_ENTRIES
                entry_limit = (
                    _FULL_LOG_ENTRY_LIMIT if mode_name == "full" else _SUMMARY_LOG_ENTRY_LIMIT
                )
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
            response["logs"] = logs

        if include_review:
            # Review feedback not yet available via core; return None
            response["review_feedback"] = None

        return response

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
        acceptance_criteria: list[str] | None = None,
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
