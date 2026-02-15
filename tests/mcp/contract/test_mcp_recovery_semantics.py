from __future__ import annotations

from typing import Any
from unittest.mock import AsyncMock

import pytest

from kagan.mcp.server import MCPRuntimeConfig, _create_mcp_server


def _tool(mcp: object, name: str):
    tool_manager = mcp._tool_manager  # type: ignore[attr-defined]  # quality-allow-private
    return tool_manager._tools[name]  # type: ignore[attr-defined]  # quality-allow-private


def _enum_values(tool: Any, field_name: str) -> set[str]:
    schema = tool.parameters
    if not isinstance(schema, dict):
        return set()

    def _resolve_ref(ref: str) -> Any:
        if not ref.startswith("#/"):
            return None
        target: Any = schema
        for segment in ref.removeprefix("#/").split("/"):
            if not isinstance(target, dict):
                return None
            target = target.get(segment)
        return target

    enum_values: set[str] = set()
    stack: list[Any] = []
    properties = schema.get("properties", {})
    if isinstance(properties, dict):
        stack.append(properties.get(field_name))

    while stack:
        node = stack.pop()
        if isinstance(node, dict):
            enum = node.get("enum")
            if isinstance(enum, (list, tuple, set)):
                enum_values.update(value for value in enum if isinstance(value, str))

            ref = node.get("$ref")
            if isinstance(ref, str):
                stack.append(_resolve_ref(ref))

            for key in ("anyOf", "oneOf", "allOf"):
                nested = node.get(key)
                if isinstance(nested, (list, tuple)):
                    stack.extend(nested)
        elif isinstance(node, (list, tuple)):
            stack.extend(node)

    return enum_values


async def test_task_patch_returns_mode_remediation_for_invalid_status(monkeypatch) -> None:
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: object())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "task_patch")

    result = await tool.fn(
        task_id="T1",
        transition="set_status",
        set={"status": "AUTO"},
        ctx=None,
    )

    assert result["success"] is False
    assert result["code"] == "TASK_TYPE_VALUE_IN_STATUS"
    assert result["next_tool"] == "task_patch"
    assert result["next_arguments"] == {
        "task_id": "T1",
        "transition": "set_task_type",
        "set": {"task_type": "AUTO"},
    }


async def test_task_patch_sets_task_type_via_transition(monkeypatch) -> None:
    class _BridgeStub:
        def __init__(self) -> None:
            self.last_task_id: str | None = None
            self.last_fields: dict[str, object] | None = None

        async def update_task(self, task_id: str, **fields: object) -> dict[str, object]:
            self.last_task_id = task_id
            self.last_fields = fields
            return {"success": True, "task_id": task_id, "current_task_type": "AUTO"}

    bridge = _BridgeStub()
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: bridge)

    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "task_patch")

    result = await tool.fn(
        task_id="T2",
        transition="set_task_type",
        set={"task_type": "AUTO"},
        ctx=None,
    )

    assert bridge.last_task_id == "T2"
    assert bridge.last_fields == {"task_type": "AUTO"}
    assert result["success"] is True


async def test_task_get_include_logs_returns_empty_list(monkeypatch) -> None:
    class _BridgeStub:
        async def get_task(
            self,
            task_id: str,
            *,
            include_scratchpad: bool | None = None,
            include_logs: bool | None = None,
            include_review: bool | None = None,
            mode: str = "summary",
        ) -> dict[str, object]:
            del include_scratchpad, include_review, mode
            assert include_logs is True
            return {
                "task_id": task_id,
                "title": "Task",
                "status": "in_progress",
                "description": None,
                "acceptance_criteria": [],
                "scratchpad": None,
                "review_feedback": None,
                "logs": [],
                "runtime": None,
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=True)
    tool = _tool(mcp, "task_get")

    result = await tool.fn(task_id="T1", include_logs=True, ctx=None)

    assert (result["task_id"], result["status"], result["logs"]) == ("T1", "in_progress", [])


async def test_task_logs_returns_paginated_page(monkeypatch) -> None:
    class _BridgeStub:
        async def list_task_logs(
            self,
            task_id: str,
            *,
            limit: int = 5,
            offset: int = 0,
            content_char_limit: int | None = None,
            total_char_limit: int | None = None,
        ) -> dict[str, object]:
            del content_char_limit, total_char_limit
            assert task_id == "T1"
            assert limit == 2
            assert offset == 2
            return {
                "task_id": task_id,
                "logs": [
                    {
                        "run": 1,
                        "content": "older run log",
                        "created_at": "2026-02-13T10:00:00Z",
                    }
                ],
                "count": 1,
                "total_runs": 3,
                "returned_runs": 1,
                "offset": 2,
                "limit": 2,
                "has_more": False,
                "next_offset": None,
                "truncated": False,
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=True)
    tool = _tool(mcp, "task_logs")

    result = await tool.fn(task_id="T1", limit=2, offset=2, ctx=None)

    assert result.task_id == "T1"
    assert result.total_runs == 3
    assert result.returned_runs == 1
    assert result.has_more is False
    assert [entry.run for entry in result.logs] == [1]


async def test_task_patch_append_note_uses_default_success_message(monkeypatch) -> None:
    class _BridgeStub:
        async def update_scratchpad(self, task_id: str, content: str) -> dict[str, object]:
            assert task_id == "T1"
            assert content == "notes"
            return {"success": True, "task_id": task_id}

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "task_patch")

    result = await tool.fn(task_id="T1", append_note="notes", ctx=None)

    assert result["success"] is True
    assert result["message"] == "Patch applied"


async def test_task_patch_rejects_non_object_set(monkeypatch) -> None:
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: object())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "task_patch")

    result = await tool.fn(task_id="T1", set="not-a-map", ctx=None)

    assert result["success"] is False
    assert result["code"] == "INVALID_SET"
    assert result["message"] == "set must be an object map"


async def test_task_patch_rejects_unknown_transition(monkeypatch) -> None:
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: object())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "task_patch")

    result = await tool.fn(task_id="T1", transition="not-supported", set={}, ctx=None)

    assert result["success"] is False
    assert result["code"] == "INVALID_TRANSITION"
    assert result["message"] == "Unsupported transition 'not-supported'"


async def test_task_patch_set_status_requires_non_empty_status(monkeypatch) -> None:
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: object())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "task_patch")

    result = await tool.fn(task_id="T1", transition="set_status", set={}, ctx=None)

    assert result["success"] is False
    assert result["code"] == "INVALID_TRANSITION"
    assert result["message"] == "set_status requires set.status"


async def test_task_patch_set_task_type_requires_value(monkeypatch) -> None:
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: object())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "task_patch")

    result = await tool.fn(task_id="T1", transition="set_task_type", set={}, ctx=None)

    assert result["success"] is False
    assert result["code"] == "INVALID_TRANSITION"
    assert result["message"] == "set_task_type requires set.task_type"


async def test_task_patch_denies_status_patch_when_capability_forbidden(monkeypatch) -> None:
    allowed = {("tasks", "update_scratchpad")}
    monkeypatch.setattr(
        "kagan.mcp.server._is_allowed",
        lambda _profile, capability, method: (capability, method) in allowed,
    )
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: object())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "task_patch")

    result = await tool.fn(task_id="T1", set={"status": "IN_PROGRESS"}, ctx=None)

    assert result["success"] is False
    assert result["code"] == "ACTION_NOT_ALLOWED"
    assert result["message"] == "status patch is not allowed for this capability profile."


async def test_task_patch_denies_field_patch_when_capability_forbidden(monkeypatch) -> None:
    allowed = {("tasks", "update_scratchpad")}
    monkeypatch.setattr(
        "kagan.mcp.server._is_allowed",
        lambda _profile, capability, method: (capability, method) in allowed,
    )
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: object())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "task_patch")

    result = await tool.fn(task_id="T1", set={"title": "Updated"}, ctx=None)

    assert result["success"] is False
    assert result["code"] == "ACTION_NOT_ALLOWED"
    assert result["message"] == "field patch is not allowed for this capability profile."


async def test_task_patch_denies_request_review_when_capability_forbidden(monkeypatch) -> None:
    allowed = {("tasks", "update_scratchpad")}
    monkeypatch.setattr(
        "kagan.mcp.server._is_allowed",
        lambda _profile, capability, method: (capability, method) in allowed,
    )
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: object())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "task_patch")

    result = await tool.fn(task_id="T1", transition="request_review", set={}, ctx=None)

    assert result["success"] is False
    assert result["code"] == "ACTION_NOT_ALLOWED"
    assert result["message"] == "request_review is not allowed for this capability profile."


async def test_task_patch_returns_first_failure_from_note_or_move_or_update_path(
    monkeypatch,
) -> None:
    class _BridgeStub:
        def __init__(self) -> None:
            self.move_task = AsyncMock(return_value={"success": True, "task_id": "T1"})
            self.update_task = AsyncMock(return_value={"success": True, "task_id": "T1"})

        async def update_scratchpad(self, task_id: str, content: str) -> dict[str, object]:
            assert task_id == "T1"
            assert content == "note"
            return {
                "success": False,
                "task_id": task_id,
                "code": "NOTE_FAIL",
                "message": "scratchpad update failed",
            }

    bridge = _BridgeStub()
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: bridge)
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "task_patch")

    result = await tool.fn(
        task_id="T1",
        append_note="note",
        set={"status": "IN_PROGRESS", "title": "Updated"},
        ctx=None,
    )

    assert result["success"] is False
    assert result["code"] == "NOTE_FAIL"
    assert result["message"] == "scratchpad update failed"
    bridge.move_task.assert_not_awaited()
    bridge.update_task.assert_not_awaited()


async def test_plan_submit_accepts_task_type_alias() -> None:
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(capability_profile="planner", identity="kagan"),
    )
    tool = _tool(mcp, "plan_submit")

    result = await tool.fn(
        tasks=[
            {
                "title": "Create parser",
                "task_type": "AUTO",
                "description": "Implement parser",
                "acceptance_criteria": ["Parses input"],
                "priority": "medium",
            }
        ],
        ctx=None,
    )

    assert result.success is True
    assert (result.status, result.task_count, result.todo_count, result.todos) == (
        "received",
        1,
        0,
        [],
    )
    task = result.tasks[0]
    assert (task["title"], task["type"], task["description"], task["priority"]) == (
        "Create parser",
        "AUTO",
        "Implement parser",
        "medium",
    )


async def test_job_start_derives_polling_recovery_when_missing(monkeypatch) -> None:
    class _BridgeStub:
        async def submit_job(
            self,
            *,
            task_id: str,
            action: str,
            arguments: dict[str, object] | None = None,
        ) -> dict[str, object]:
            assert task_id == "T1"
            assert arguments is None
            return {
                "success": True,
                "job_id": "J1",
                "task_id": task_id,
                "action": action,
                "status": "queued",
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "job_start")

    result = await tool.fn(task_id="T1", action="start_agent", ctx=None)

    assert result.success is True
    assert result.next_tool == "job_poll"
    assert result.next_arguments == {
        "job_id": "J1",
        "task_id": "T1",
        "wait": True,
        "timeout_seconds": 1.5,
    }


async def test_job_start_returns_actionable_recovery_when_bridge_returns_error_envelope(
    monkeypatch,
) -> None:
    class _BridgeStub:
        async def submit_job(
            self,
            *,
            task_id: str,
            action: str,
            arguments: dict[str, object] | None = None,
        ) -> dict[str, object]:
            assert task_id == "T1"
            assert action == "start_agent"
            assert arguments is None
            return {
                "success": False,
                "job_id": "J-err",
                "task_id": task_id,
                "action": action,
                "status": "failed",
                "code": "TASK_TYPE_MISMATCH",
                "message": "Task must be AUTO to run start_agent",
                "hint": "Switch task_type to AUTO and retry",
                "next_tool": "task_patch",
                "next_arguments": {
                    "task_id": task_id,
                    "transition": "set_task_type",
                    "set": {"task_type": "AUTO"},
                },
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "job_start")

    result = await tool.fn(task_id="T1", action="start_agent", ctx=None)

    assert result.success is False
    assert result.code == "TASK_TYPE_MISMATCH"
    assert result.next_tool == "task_patch"
    assert result.next_arguments == {
        "task_id": "T1",
        "transition": "set_task_type",
        "set": {"task_type": "AUTO"},
    }
    assert "AUTO" in (result.hint or "")


async def test_job_poll_events_maps_payload_and_pagination(monkeypatch) -> None:
    class _BridgeStub:
        async def list_job_events(
            self,
            *,
            job_id: str,
            task_id: str,
            limit: int = 50,
            offset: int = 0,
        ) -> dict[str, object]:
            assert job_id == "J2"
            assert task_id == "T2"
            assert limit == 1
            assert offset == 1
            return {
                "success": True,
                "job_id": job_id,
                "task_id": task_id,
                "events": [
                    {
                        "job_id": job_id,
                        "task_id": task_id,
                        "status": "queued",
                        "timestamp": "2026-02-10T10:00:00Z",
                        "message": "Job queued",
                        "code": "JOB_QUEUED",
                    }
                ],
                "total_events": 3,
                "returned_events": 1,
                "offset": offset,
                "limit": limit,
                "has_more": True,
                "next_offset": 2,
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "job_poll")

    result = await tool.fn(job_id="J2", task_id="T2", events=True, limit=1, offset=1, ctx=None)

    assert result.success is True
    assert (result.total_events, result.returned_events, result.offset, result.limit) == (
        3,
        1,
        1,
        1,
    )
    assert (result.has_more, result.next_offset) == (True, 2)


@pytest.mark.parametrize("status", ["queued", "running"])
async def test_job_poll_derives_wait_followup_for_non_terminal_states(
    monkeypatch,
    status: str,
) -> None:
    class _BridgeStub:
        async def get_job(self, *, job_id: str, task_id: str) -> dict[str, object]:
            assert job_id == "J1"
            assert task_id == "T1"
            return {"success": True, "job_id": job_id, "task_id": task_id, "status": status}

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())

    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "job_poll")
    result = await tool.fn(job_id="J1", task_id="T1", wait=False, ctx=None)

    assert result.success is True
    assert result.next_tool == "job_poll"
    assert result.next_arguments == {
        "job_id": "J1",
        "task_id": "T1",
        "wait": True,
        "timeout_seconds": 1.5,
    }


async def test_job_poll_derives_pending_recovery_when_missing(monkeypatch) -> None:
    class _BridgeStub:
        async def get_job(self, *, job_id: str, task_id: str) -> dict[str, object]:
            return {
                "success": True,
                "job_id": job_id,
                "task_id": task_id,
                "status": "failed",
                "result": {
                    "success": False,
                    "code": "START_PENDING",
                    "message": "pending scheduler admission",
                },
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "job_poll")

    result = await tool.fn(job_id="J1", task_id="T1", wait=False, ctx=None)

    assert result.success is True
    assert result.code == "START_PENDING"
    assert result.next_tool == "job_poll"
    assert result.next_arguments == {
        "job_id": "J1",
        "task_id": "T1",
        "wait": True,
        "timeout_seconds": 1.5,
    }


async def test_job_poll_derives_recovery_fields_when_timeout_payload_partial(monkeypatch) -> None:
    class _BridgeStub:
        async def get_job(self, *, job_id: str, task_id: str) -> dict[str, object]:
            return {
                "success": False,
                "job_id": job_id,
                "task_id": task_id,
                "status": "running",
                "timed_out": True,
                "timeout_requested_seconds": 0.3,
                "timeout_waited_seconds": 0.3,
                "message": "Timed out waiting for job completion",
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "job_poll")

    result = await tool.fn(job_id="J-timeout", task_id="T-timeout", wait=False, ctx=None)

    assert result.success is False
    assert result.timed_out is True
    assert result.timeout_metadata == {
        "timeout_requested_seconds": 0.3,
        "timeout_waited_seconds": 0.3,
    }
    assert result.next_tool == "job_poll"
    assert result.next_arguments == {
        "job_id": "J-timeout",
        "task_id": "T-timeout",
        "wait": True,
        "timeout_seconds": 1.5,
    }


async def test_job_cancel_derives_poll_followup_when_missing(monkeypatch) -> None:
    class _BridgeStub:
        async def cancel_job(self, *, job_id: str, task_id: str) -> dict[str, object]:
            return {
                "success": True,
                "job_id": job_id,
                "task_id": task_id,
                "status": "cancelled",
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "job_cancel")

    result = await tool.fn(job_id="J3", task_id="T3", ctx=None)

    assert result.success is True
    assert result.next_tool == "job_poll"
    assert result.next_arguments == {
        "job_id": "J3",
        "task_id": "T3",
        "wait": True,
        "timeout_seconds": 1.5,
    }


def test_review_apply_schema_exposes_canonical_action_enums() -> None:
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "review_apply")

    assert _enum_values(tool, "action") == {"approve", "reject", "merge", "rebase"}
    assert _enum_values(tool, "rejection_action") == {
        "reopen",
        "return",
        "in_progress",
        "backlog",
    }


async def test_internal_diagnostics_tool_returns_instrumentation_snapshot(monkeypatch) -> None:
    class _BridgeStub:
        async def get_instrumentation_snapshot(self) -> dict[str, object]:
            return {
                "enabled": True,
                "log_events": False,
                "counters": {"core.process.exec.calls": 4},
                "timings": {"core.process.exec.duration_ms": {"count": 4, "avg_ms": 5.2}},
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(
            capability_profile="maintainer",
            identity="kagan_admin",
            enable_internal_instrumentation=True,
        ),
    )
    tool = _tool(mcp, "diagnostics_instrumentation")

    result = await tool.fn(ctx=None)

    assert result.enabled is True
    assert result.log_events is False
    assert result.counters["core.process.exec.calls"] == 4
    assert result.timings["core.process.exec.duration_ms"]["count"] == 4
