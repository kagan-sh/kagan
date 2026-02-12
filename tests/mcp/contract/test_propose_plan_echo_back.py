from __future__ import annotations

import json

import pytest

from kagan.core.agents.planner import parse_proposed_plan

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


@pytest.mark.parametrize(
    ("fallback_field", "fallback_payload", "expected_title"),
    [
        (
            "rawInput",
            {"tasks": [{"title": "From rawInput fallback", "type": "AUTO"}]},
            "From rawInput fallback",
        ),
        (
            "title",
            {"tasks": [{"title": "From title fallback", "type": "AUTO"}]},
            "From title fallback",
        ),
    ],
)
def test_planner_skips_summary_only_content(
    fallback_field: str,
    fallback_payload: dict[str, list[dict[str, str]]],
    expected_title: str,
) -> None:
    """Summary-only content falls back to the best non-echo payload source."""
    summary_only = {"status": "received", "task_count": 1, "todo_count": 0}
    tool_call: dict[str, object] = {
        "name": "propose_plan",
        "status": "completed",
        "content": _echo_back_content(summary_only),
    }
    if fallback_field == "rawInput":
        tool_call["rawInput"] = json.dumps(fallback_payload)
    else:
        tool_call["title"] = f"propose_plan: {json.dumps(fallback_payload)}"

    tool_calls = {"tc-1": tool_call}

    tasks, _todos, error = parse_proposed_plan(tool_calls)

    assert error is None
    assert len(tasks) == 1
    assert tasks[0].title == expected_title
