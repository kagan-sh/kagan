from __future__ import annotations

from typing import Any

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


async def _invoke_job_tool(tool_name: str, tool: Any) -> Any:
    if tool_name == "jobs_get":
        return await tool.fn(job_id="J1", task_id="T1", ctx=None)
    return await tool.fn(job_id="J1", task_id="T1", timeout_seconds=0.25, ctx=None)


class _JobBridgeStub:
    def __init__(self, payload: dict[str, object]) -> None:
        self.payload = payload
        self.wait_timeout_seconds: float | None = None

    async def get_job(self, *, job_id: str, task_id: str) -> dict[str, object]:
        assert job_id == "J1"
        assert task_id == "T1"
        return dict(self.payload)

    async def wait_job(
        self,
        *,
        job_id: str,
        task_id: str,
        timeout_seconds: float,
    ) -> dict[str, object]:
        assert job_id == "J1"
        assert task_id == "T1"
        self.wait_timeout_seconds = timeout_seconds
        return dict(self.payload)


async def test_tasks_move_returns_deterministic_remediation_for_mode_values(monkeypatch) -> None:
    """tasks_move should return next-tool recovery guidance for status=AUTO/PAIR."""
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: object())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "tasks_move")

    result = await tool.fn(task_id="T1", status="AUTO", ctx=None)

    assert result.success is False
    assert result.code == "TASK_TYPE_VALUE_IN_STATUS"
    assert result.next_tool == "tasks_update"
    assert result.next_arguments == {"task_id": "T1", "task_type": "AUTO"}


def test_tasks_move_schema_exposes_only_status_literals() -> None:
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "tasks_move")

    assert _enum_values(tool, "status") == {
        "BACKLOG",
        "IN_PROGRESS",
        "REVIEW",
        "DONE",
        "backlog",
        "in_progress",
        "review",
        "done",
    }


async def test_tasks_update_normalizes_status_mode_to_task_type(monkeypatch) -> None:
    """tasks_update should normalize status=AUTO into task_type=AUTO for recovery."""

    class _BridgeStub:
        def __init__(self) -> None:
            self.last_task_id: str | None = None
            self.last_fields: dict[str, object] | None = None

        async def update_task(self, task_id: str, **fields: object) -> dict[str, object]:
            self.last_task_id = task_id
            self.last_fields = fields
            return {"success": True, "task_id": task_id}

    bridge = _BridgeStub()
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: bridge)

    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "tasks_update")

    result = await tool.fn(task_id="T2", status="AUTO", ctx=None)

    assert bridge.last_task_id == "T2"
    assert bridge.last_fields == {"task_type": "AUTO"}
    assert result.success is True
    assert result.code == "STATUS_WAS_TASK_TYPE"


async def test_get_task_include_logs_returns_empty_list_instead_of_none(monkeypatch) -> None:
    """get_task(include_logs=True) should preserve explicit empty logs payload."""

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
    tool = _tool(mcp, "get_task")

    result = await tool.fn(task_id="T1", include_logs=True, ctx=None)

    assert (result.task_id, result.status, result.logs) == ("T1", "in_progress", [])


async def test_update_scratchpad_uses_success_message_when_core_omits_message(monkeypatch) -> None:
    class _BridgeStub:
        async def update_scratchpad(self, task_id: str, content: str) -> dict[str, object]:
            assert task_id == "T1"
            assert content == "notes"
            return {"success": True, "task_id": task_id}

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "update_scratchpad")

    result = await tool.fn(task_id="T1", content="notes", ctx=None)

    assert result.success is True
    assert result.message == "Scratchpad updated"


async def test_propose_plan_accepts_task_type_alias() -> None:
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(capability_profile="planner", identity="kagan"),
    )
    tool = _tool(mcp, "propose_plan")

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
    summary = (result.status, result.task_count, result.todo_count, result.todos)
    assert summary == ("received", 1, 0, [])
    task = result.tasks[0]
    task_meta = (task["title"], task["type"], task["description"], task["priority"])
    assert task_meta == ("Create parser", "AUTO", "Implement parser", "medium")
    assert task["acceptance_criteria"] == ["Parses input"]


async def test_jobs_submit_derives_polling_recovery_when_missing(monkeypatch) -> None:
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
    tool = _tool(mcp, "jobs_submit")

    result = await tool.fn(task_id="T1", action="start_agent", ctx=None)

    assert result.success is True
    assert result.next_tool == "jobs_wait"
    assert result.next_arguments == {
        "job_id": "J1",
        "task_id": "T1",
        "timeout_seconds": 1.5,
    }


async def test_jobs_submit_derives_action_discovery_recovery_when_unsupported(monkeypatch) -> None:
    class _BridgeStub:
        async def submit_job(
            self,
            *,
            task_id: str,
            action: str,
            arguments: dict[str, object] | None = None,
        ) -> dict[str, object]:
            del arguments
            assert task_id == "T1"
            assert action == "start_agent"
            return {
                "success": False,
                "task_id": task_id,
                "code": "UNSUPPORTED_ACTION",
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "jobs_submit")

    result = await tool.fn(task_id="T1", action="start_agent", ctx=None)

    assert result.success is False
    assert result.code == "UNSUPPORTED_ACTION"
    assert result.next_tool == "jobs_list_actions"
    assert result.next_arguments == {}


async def test_jobs_list_actions_returns_supported_action_names() -> None:
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "jobs_list_actions")

    result = await tool.fn(ctx=None)

    assert result.actions == ["start_agent", "stop_agent"]


async def test_jobs_list_actions_matches_submit_action_schema() -> None:
    mcp = _create_mcp_server(readonly=False)
    submit_tool = _tool(mcp, "jobs_submit")
    list_actions_tool = _tool(mcp, "jobs_list_actions")

    submit_actions = _enum_values(submit_tool, "action")
    result = await list_actions_tool.fn(ctx=None)

    assert submit_actions == {"start_agent", "stop_agent"}
    assert set(result.actions) == submit_actions


def test_review_schema_exposes_canonical_action_enums() -> None:
    mcp = _create_mcp_server(readonly=False)
    review_tool = _tool(mcp, "review")

    assert _enum_values(review_tool, "action") == {"approve", "reject", "merge", "rebase"}
    assert _enum_values(review_tool, "rejection_action") == {
        "reopen",
        "return",
        "in_progress",
        "backlog",
    }


@pytest.mark.parametrize("tool_name", ["jobs_get", "jobs_wait"])
@pytest.mark.parametrize("status", ["queued", "running"])
async def test_job_tools_derive_wait_followup_for_non_terminal_states(
    monkeypatch,
    tool_name: str,
    status: str,
) -> None:
    bridge = _JobBridgeStub(
        {
            "success": True,
            "job_id": "J1",
            "task_id": "T1",
            "status": status,
        }
    )
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: bridge)

    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, tool_name)
    result = await _invoke_job_tool(tool_name, tool)

    assert result.success is True
    assert result.next_tool == "jobs_wait"
    assert result.next_arguments == {
        "job_id": "J1",
        "task_id": "T1",
        "timeout_seconds": 1.5,
    }


@pytest.mark.parametrize("tool_name", ["jobs_get", "jobs_wait"])
async def test_job_tools_preserve_timeout_metadata(monkeypatch, tool_name: str) -> None:
    timeout = {"requested_seconds": 0.25, "waited_seconds": 0.25}
    bridge = _JobBridgeStub(
        {
            "success": True,
            "job_id": "J1",
            "task_id": "T1",
            "status": "running",
            "code": "JOB_TIMEOUT",
            "timed_out": True,
            "timeout": timeout,
        }
    )
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: bridge)

    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, tool_name)
    result = await _invoke_job_tool(tool_name, tool)

    assert result.timed_out is True
    assert result.timeout_metadata == timeout
    assert result.next_tool == "jobs_wait"
    assert result.next_arguments == {
        "job_id": "J1",
        "task_id": "T1",
        "timeout_seconds": 1.5,
    }


async def test_jobs_wait_timeout_metadata_falls_back_to_result_payload(monkeypatch) -> None:
    class _BridgeStub:
        async def wait_job(
            self,
            *,
            job_id: str,
            task_id: str,
            timeout_seconds: float,
        ) -> dict[str, object]:
            assert job_id == "J2"
            assert task_id == "T2"
            assert timeout_seconds == 0.5
            return {
                "success": False,
                "job_id": job_id,
                "task_id": task_id,
                "status": "running",
                "result": {
                    "success": False,
                    "code": "JOB_TIMEOUT",
                    "timed_out": True,
                    "timeout": {"requested_seconds": timeout_seconds},
                },
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "jobs_wait")

    result = await tool.fn(job_id="J2", task_id="T2", timeout_seconds=0.5, ctx=None)

    assert result.code == "JOB_TIMEOUT"
    assert result.timed_out is True
    assert result.timeout_metadata == {"requested_seconds": 0.5}
    assert result.next_tool == "jobs_wait"


async def test_jobs_events_maps_canonical_payload_and_pagination(monkeypatch) -> None:
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
    tool = _tool(mcp, "jobs_events")

    result = await tool.fn(job_id="J2", task_id="T2", limit=1, offset=1, ctx=None)

    assert result.success is True
    r = result
    assert (r.total_events, r.returned_events, r.offset, r.limit) == (3, 1, 1, 1)
    assert (r.has_more, r.next_offset) == (True, 2)
    event = result.events[0]
    expected = ("J2", "T2", "queued", "2026-02-10T10:00:00Z", "JOB_QUEUED")
    assert (event.job_id, event.task_id, event.status, event.timestamp, event.code) == expected


@pytest.mark.parametrize("tool_name", ["jobs_get", "jobs_wait"])
@pytest.mark.parametrize("status", ["succeeded", "failed", "cancelled"])
async def test_job_tools_do_not_derive_followup_for_terminal_states_without_hints(
    monkeypatch,
    tool_name: str,
    status: str,
) -> None:
    bridge = _JobBridgeStub(
        {
            "success": True,
            "job_id": "J1",
            "task_id": "T1",
            "status": status,
            "code": "STARTED" if status == "succeeded" else None,
        }
    )
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: bridge)

    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, tool_name)
    result = await _invoke_job_tool(tool_name, tool)

    assert (
        result.job_id,
        result.task_id,
        result.status,
        result.code,
        result.next_tool,
        result.next_arguments,
    ) == ("J1", "T1", status, "STARTED" if status == "succeeded" else None, None, None)


async def test_jobs_get_derives_blocked_recovery_when_missing(monkeypatch) -> None:
    class _BridgeStub:
        async def get_job(self, *, job_id: str, task_id: str) -> dict[str, object]:
            assert job_id == "J1"
            assert task_id == "T1"
            return {
                "success": True,
                "job_id": job_id,
                "task_id": task_id,
                "status": "failed",
                "result": {
                    "success": False,
                    "code": "START_BLOCKED",
                    "message": "blocked",
                    "runtime": {
                        "is_blocked": True,
                        "blocked_by_task_ids": ["B1"],
                    },
                },
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "jobs_get")

    result = await tool.fn(job_id="J1", task_id="T1", ctx=None)

    assert result.success is True
    assert result.code == "START_BLOCKED"
    assert result.next_tool == "get_task"
    assert result.next_arguments == {"task_id": "B1", "mode": "summary"}


async def test_jobs_get_keeps_explicit_recovery_from_core(monkeypatch) -> None:
    class _BridgeStub:
        async def get_job(self, *, job_id: str, task_id: str) -> dict[str, object]:
            assert job_id == "J1"
            assert task_id == "T1"
            return {
                "success": False,
                "job_id": job_id,
                "task_id": task_id,
                "status": "failed",
                "code": "START_BLOCKED",
                "message": "blocked",
                "hint": "Use tasks_update before retrying.",
                "next_tool": "tasks_update",
                "next_arguments": {"task_id": task_id, "task_type": "AUTO"},
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "jobs_get")

    result = await tool.fn(job_id="J1", task_id="T1", ctx=None)

    assert result.success is False
    assert result.code == "START_BLOCKED"
    assert result.hint == "Use tasks_update before retrying."
    assert result.next_tool == "tasks_update"
    assert result.next_arguments == {"task_id": "T1", "task_type": "AUTO"}


async def test_jobs_get_derives_not_running_recovery_when_missing(monkeypatch) -> None:
    class _BridgeStub:
        async def get_job(self, *, job_id: str, task_id: str) -> dict[str, object]:
            assert job_id == "J2"
            assert task_id == "T2"
            return {
                "success": True,
                "job_id": job_id,
                "task_id": task_id,
                "status": "failed",
                "result": {
                    "success": False,
                    "code": "NOT_RUNNING",
                    "message": "No running agent",
                },
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "jobs_get")

    result = await tool.fn(job_id="J2", task_id="T2", ctx=None)

    assert result.success is True
    assert result.code == "NOT_RUNNING"
    assert result.next_tool == "get_task"
    assert result.next_arguments == {
        "task_id": "T2",
        "include_logs": True,
        "mode": "summary",
    }


async def test_jobs_cancel_derives_wait_followup_when_missing(monkeypatch) -> None:
    class _BridgeStub:
        async def cancel_job(self, *, job_id: str, task_id: str) -> dict[str, object]:
            assert job_id == "J3"
            assert task_id == "T3"
            return {
                "success": True,
                "job_id": job_id,
                "task_id": task_id,
                "status": "cancelled",
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "jobs_cancel")

    result = await tool.fn(job_id="J3", task_id="T3", ctx=None)

    assert result.success is True
    assert result.next_tool == "jobs_wait"
    assert result.next_arguments == {
        "job_id": "J3",
        "task_id": "T3",
        "timeout_seconds": 1.5,
    }


async def test_request_review_derives_recovery_when_failed_without_next_tool(monkeypatch) -> None:
    class _BridgeStub:
        async def request_review(self, task_id: str, summary: str) -> dict[str, object]:
            assert task_id == "T3"
            assert summary == "done"
            return {"success": False, "status": "error", "message": "not ready"}

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(readonly=False)
    tool = _tool(mcp, "request_review")

    result = await tool.fn(task_id="T3", summary="done", ctx=None)

    assert result.success is False
    assert result.status == "error"
    assert result.next_tool == "get_task"
    assert result.next_arguments == {
        "task_id": "T3",
        "include_logs": True,
        "mode": "summary",
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
