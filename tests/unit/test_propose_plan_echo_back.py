from __future__ import annotations

import json

from kagan.agents.planner import PlanProposal, parse_proposed_plan
from kagan.mcp.models import PlanProposalResponse

# --- MCP server echo-back response tests ---


def test_propose_plan_mcp_returns_full_echo_on_valid_input() -> None:
    """MCP tool returns normalized tasks/todos in result, not just counts."""
    proposal = PlanProposal.model_validate(
        {
            "tasks": [{"title": "Build API", "type": "AUTO", "priority": "high"}],
            "todos": [{"content": "Analyze requirements", "status": "completed"}],
        }
    )
    response = PlanProposalResponse(
        status="received",
        task_count=len(proposal.tasks),
        todo_count=len(proposal.todos),
        tasks=[t.model_dump(mode="json") for t in proposal.tasks],
        todos=[t.model_dump(mode="json") for t in proposal.todos],
    )

    assert response.status == "received"
    assert response.tasks is not None
    assert len(response.tasks) == 1
    assert response.tasks[0]["title"] == "Build API"
    assert response.todos is not None
    assert len(response.todos) == 1


# --- Planner parser echo-back preference tests ---


def _echo_back_content(payload: dict) -> list[dict]:
    """Build ACP content list from an echo-back payload dict."""
    return [
        {
            "type": "content",
            "content": {"type": "text", "text": json.dumps(payload)},
        }
    ]


def test_planner_parses_plan_from_echo_back_content() -> None:
    """Parser extracts plan from tool call's content field (the MCP result echo)."""
    echo = {
        "status": "received",
        "task_count": 2,
        "todo_count": 0,
        "tasks": [
            {"title": "Task A", "type": "AUTO"},
            {"title": "Task B", "type": "PAIR"},
        ],
        "todos": [],
    }
    tool_calls = {
        "tc-1": {
            "name": "propose_plan",
            "status": "completed",
            "title": 'propose_plan: {"tasks":[{"title":"Task..."}]}',
            "content": _echo_back_content(echo),
        }
    }

    tasks, _todos, error = parse_proposed_plan(tool_calls)

    assert error is None
    assert len(tasks) == 2
    assert tasks[0].title == "Task A"
    assert tasks[1].title == "Task B"


def test_planner_prefers_echo_back_over_raw_input() -> None:
    """When both rawInput and content exist, parser uses content (echo-back)."""
    raw_input_payload = {
        "tasks": [{"title": "From rawInput", "type": "AUTO"}],
    }
    echo_payload = {
        "status": "received",
        "task_count": 1,
        "todo_count": 0,
        "tasks": [{"title": "From echo-back", "type": "AUTO"}],
        "todos": [],
    }
    tool_calls = {
        "tc-1": {
            "name": "propose_plan",
            "status": "completed",
            "rawInput": json.dumps(raw_input_payload),
            "content": _echo_back_content(echo_payload),
        }
    }

    tasks, _todos, error = parse_proposed_plan(tool_calls)

    assert error is None
    assert len(tasks) == 1
    assert tasks[0].title == "From echo-back"


def test_planner_skips_summary_only_content() -> None:
    """Content with only task_count/status (no tasks) falls back to rawInput."""
    summary_only = {"status": "received", "task_count": 1, "todo_count": 0}
    raw_input_payload = {
        "tasks": [{"title": "From rawInput fallback", "type": "AUTO"}],
    }
    tool_calls = {
        "tc-1": {
            "name": "propose_plan",
            "status": "completed",
            "rawInput": json.dumps(raw_input_payload),
            "content": _echo_back_content(summary_only),
        }
    }

    tasks, _todos, error = parse_proposed_plan(tool_calls)

    assert error is None
    assert len(tasks) == 1
    assert tasks[0].title == "From rawInput fallback"
