"""Integration tests: end-to-end MCP bridge flows for current v2 semantics."""

from __future__ import annotations

from typing import Any
from unittest.mock import AsyncMock

from kagan.core.ipc.contracts import CoreResponse
from kagan.mcp.tools import CoreClientBridge


def _bridge_for_routes(
    routes: dict[tuple[str, str], dict[str, Any]],
    *,
    captured_calls: list[dict[str, Any]] | None = None,
) -> CoreClientBridge:
    client = AsyncMock()
    client.is_connected = True

    async def _request(
        *,
        session_id: str,
        session_profile: str | None = None,
        session_origin: str | None = None,
        capability: str,
        method: str,
        params: dict[str, Any] | None = None,
        idempotency_key: str | None = None,
    ) -> CoreResponse:
        del session_id, session_profile, session_origin, idempotency_key
        if captured_calls is not None:
            captured_calls.append(
                {
                    "capability": capability,
                    "method": method,
                    "params": params or {},
                }
            )
        payload = routes[(capability, method)]
        return CoreResponse.success("req-1", result=payload)

    client.request = _request
    return CoreClientBridge(client, session_id="test-session")


async def test_end_to_end_job_flow_uses_submit_wait_events_contract() -> None:
    calls: list[dict[str, Any]] = []
    bridge = _bridge_for_routes(
        {
            (
                "jobs",
                "submit",
            ): {
                "success": True,
                "job_id": "J-101",
                "task_id": "T-77",
                "action": "agent_start",
                "status": "queued",
            },
            ("jobs", "get"): {
                "success": True,
                "job_id": "J-101",
                "task_id": "T-77",
                "status": "running",
            },
            ("jobs", "wait"): {
                "success": True,
                "job_id": "J-101",
                "task_id": "T-77",
                "status": "succeeded",
                "timed_out": False,
            },
            ("jobs", "events"): {
                "success": True,
                "job_id": "J-101",
                "task_id": "T-77",
                "events": [
                    {
                        "job_id": "J-101",
                        "task_id": "T-77",
                        "status": "queued",
                        "timestamp": "2026-02-10T10:00:00+00:00",
                        "message": "Job queued",
                        "code": "JOB_QUEUED",
                    },
                    {
                        "job_id": "J-101",
                        "task_id": "T-77",
                        "status": "succeeded",
                        "timestamp": "2026-02-10T10:00:03+00:00",
                        "message": "Completed",
                        "code": "STARTED",
                    },
                ],
                "total_events": 5,
                "returned_events": 2,
                "offset": 2,
                "limit": 2,
                "has_more": True,
                "next_offset": 4,
            },
            ("jobs", "cancel"): {
                "success": True,
                "job_id": "J-101",
                "task_id": "T-77",
                "status": "cancelled",
            },
        },
        captured_calls=calls,
    )

    submitted = await bridge.submit_job(task_id="T-77", action="agent_start")
    current = await bridge.get_job(job_id="J-101", task_id="T-77")
    waited = await bridge.wait_job(job_id="J-101", task_id="T-77", timeout_seconds=1.5)
    events = await bridge.list_job_events(job_id="J-101", task_id="T-77", limit=2, offset=2)
    cancelled = await bridge.cancel_job(job_id="J-101", task_id="T-77")

    assert submitted["status"] == "queued"
    assert current["status"] == "running"
    assert waited["status"] == "succeeded"
    assert events["returned_events"] == 2
    assert events["next_offset"] == 4
    assert cancelled["status"] == "cancelled"
    assert calls == [
        {
            "capability": "jobs",
            "method": "submit",
            "params": {"task_id": "T-77", "action": "agent_start"},
        },
        {
            "capability": "jobs",
            "method": "get",
            "params": {"job_id": "J-101", "task_id": "T-77"},
        },
        {
            "capability": "jobs",
            "method": "wait",
            "params": {"job_id": "J-101", "task_id": "T-77", "timeout_seconds": 1.5},
        },
        {
            "capability": "jobs",
            "method": "events",
            "params": {"job_id": "J-101", "task_id": "T-77", "limit": 2, "offset": 2},
        },
        {
            "capability": "jobs",
            "method": "cancel",
            "params": {"job_id": "J-101", "task_id": "T-77"},
        },
    ]


async def test_end_to_end_task_lifecycle_uses_create_move_update_review_contract() -> None:
    calls: list[dict[str, Any]] = []
    bridge = _bridge_for_routes(
        {
            ("tasks", "create"): {
                "success": True,
                "task": {"id": "T-501", "title": "Lifecycle", "status": "BACKLOG"},
            },
            ("tasks", "move"): {
                "success": True,
                "task_id": "T-501",
                "status": "IN_PROGRESS",
            },
            ("tasks", "update"): {
                "success": True,
                "task": {"id": "T-501", "status": "IN_PROGRESS"},
            },
            ("review", "request"): {
                "success": True,
                "task_id": "T-501",
                "status": "review",
                "message": "Ready for merge",
            },
        },
        captured_calls=calls,
    )

    created = await bridge.create_task(
        title="Lifecycle",
        description="phase-4 lifecycle coverage",
        status="BACKLOG",
        priority="MEDIUM",
        task_type="AUTO",
    )
    moved = await bridge.move_task(task_id="T-501", status="IN_PROGRESS")
    updated = await bridge.update_task(
        "T-501",
        acceptance_criteria=["Command wiring remains stable"],
    )
    reviewed = await bridge.request_review("T-501", "Lifecycle complete")

    assert created["task"]["id"] == "T-501"
    assert moved["status"] == "IN_PROGRESS"
    assert updated["success"] is True
    assert reviewed["status"] == "review"
    assert calls == [
        {
            "capability": "tasks",
            "method": "create",
            "params": {
                "title": "Lifecycle",
                "description": "phase-4 lifecycle coverage",
                "status": "BACKLOG",
                "priority": "MEDIUM",
                "task_type": "AUTO",
            },
        },
        {
            "capability": "tasks",
            "method": "move",
            "params": {"task_id": "T-501", "status": "IN_PROGRESS"},
        },
        {
            "capability": "tasks",
            "method": "update",
            "params": {
                "task_id": "T-501",
                "acceptance_criteria": ["Command wiring remains stable"],
            },
        },
        {
            "capability": "review",
            "method": "request",
            "params": {"task_id": "T-501", "summary": "Lifecycle complete"},
        },
    ]


async def test_end_to_end_merge_review_flow_uses_request_approve_merge_contract() -> None:
    calls: list[dict[str, Any]] = []
    bridge = _bridge_for_routes(
        {
            ("review", "request"): {
                "success": True,
                "task_id": "T-601",
                "status": "review",
                "message": "Ready for merge",
            },
            ("review", "approve"): {
                "success": True,
                "task_id": "T-601",
                "status": "approved",
            },
            ("review", "merge"): {
                "success": True,
                "task_id": "T-601",
                "status": "done",
            },
        },
        captured_calls=calls,
    )

    requested = await bridge.request_review("T-601", "Ready for maintainer review")
    approved = await bridge.review_action("T-601", action="approve")
    merged = await bridge.review_action("T-601", action="merge")

    assert requested["status"] == "review"
    assert approved["status"] == "approved"
    assert merged["status"] == "done"
    assert calls == [
        {
            "capability": "review",
            "method": "request",
            "params": {"task_id": "T-601", "summary": "Ready for maintainer review"},
        },
        {
            "capability": "review",
            "method": "approve",
            "params": {"task_id": "T-601"},
        },
        {
            "capability": "review",
            "method": "merge",
            "params": {"task_id": "T-601"},
        },
    ]


async def test_end_to_end_session_flow_uses_create_exists_kill_contract() -> None:
    calls: list[dict[str, Any]] = []
    bridge = _bridge_for_routes(
        {
            ("sessions", "create"): {
                "success": True,
                "task_id": "T-701",
                "session_name": "kagan-T-701",
                "backend": "tmux",
            },
            ("sessions", "exists"): {
                "success": True,
                "task_id": "T-701",
                "exists": True,
                "session_name": "kagan-T-701",
            },
            ("sessions", "kill"): {
                "success": True,
                "task_id": "T-701",
            },
        },
        captured_calls=calls,
    )

    created = await bridge.create_session(
        "T-701",
        reuse_if_exists=False,
        worktree_path="/tmp/kagan-phase4-wt",
    )
    exists = await bridge.session_exists("T-701")
    killed = await bridge.kill_session("T-701")

    assert created["session_name"] == "kagan-T-701"
    assert exists["exists"] is True
    assert killed["success"] is True
    assert calls == [
        {
            "capability": "sessions",
            "method": "create",
            "params": {
                "task_id": "T-701",
                "reuse_if_exists": False,
                "worktree_path": "/tmp/kagan-phase4-wt",
            },
        },
        {
            "capability": "sessions",
            "method": "exists",
            "params": {"task_id": "T-701"},
        },
        {
            "capability": "sessions",
            "method": "kill",
            "params": {"task_id": "T-701"},
        },
    ]


async def test_review_reject_sends_feedback_and_rejection_action() -> None:
    calls: list[dict[str, Any]] = []
    bridge = _bridge_for_routes(
        {
            ("review", "reject"): {
                "success": True,
                "task_id": "t1",
            }
        },
        captured_calls=calls,
    )

    result = await bridge.review_action(
        "t1",
        action="reject",
        feedback="needs tests",
        rejection_action="reopen",
    )

    assert result["success"] is True
    assert calls == [
        {
            "capability": "review",
            "method": "reject",
            "params": {
                "task_id": "t1",
                "feedback": "needs tests",
                "action": "reopen",
            },
        }
    ]
