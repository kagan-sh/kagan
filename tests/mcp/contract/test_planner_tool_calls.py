from __future__ import annotations

import json
from typing import Any, cast

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from kagan.core.agents.planner import parse_proposed_plan


def _payload_strategy() -> st.SearchStrategy[dict[str, Any]]:
    text = st.text(
        alphabet=st.characters(whitelist_categories=("Ll", "Lu", "Nd", "Po")),
        min_size=1,
        max_size=60,
    )
    priorities = st.sampled_from(["low", "medium", "high"])
    task_types = st.sampled_from(["AUTO", "PAIR"])
    task = st.fixed_dictionaries(
        cast(
            "Any",
            {
                "title": text,
                "type": task_types,
                "description": text,
                "acceptance_criteria": st.lists(text, min_size=1, max_size=3),
                "priority": priorities,
            },
        )
    )
    todo = st.fixed_dictionaries(
        cast(
            "Any",
            {
                "content": text,
                "status": st.sampled_from(["pending", "in_progress", "completed", "failed"]),
            },
        )
    )
    return st.fixed_dictionaries(
        cast(
            "Any",
            {
                "tasks": st.lists(task, min_size=1, max_size=3),
                "todos": st.lists(todo, min_size=0, max_size=3),
            },
        )
    )


@settings(max_examples=40)
@given(
    payload=_payload_strategy(),
    prefix=st.sampled_from(
        [
            "propose_plan: ",
            "tool=propose_plan: ",
            "  propose_plan: ",
            "PROPOSE_PLAN: ",
        ]
    ),
    suffix=st.sampled_from(["", " ", "\n", " (approved)"]),
)
def test_parse_proposed_plan_from_title_payload(
    payload: dict[str, Any], prefix: str, suffix: str
) -> None:
    tool_calls = {
        "tc-plan-title": {
            "title": f"{prefix}{json.dumps(payload)}{suffix}",
            "status": "completed",
        }
    }

    tasks, todos, error = parse_proposed_plan(tool_calls)

    assert error is None
    assert len(tasks) == len(payload["tasks"])
    expected_todos = payload["todos"]
    if expected_todos:
        assert todos is not None
        assert len(todos) == len(expected_todos)
    else:
        assert todos is None


@settings(max_examples=25)
@given(
    prefix=st.sampled_from(["propose_plan: ", "tool=propose_plan: ", "PROPOSE_PLAN: "]),
    suffix=st.sampled_from(["", " ", " (bad)"]),
    bad_tasks=st.one_of(
        st.just([]),
        st.lists(st.fixed_dictionaries({}), min_size=1, max_size=2),
    ),
)
def test_parse_proposed_plan_invalid_payload_errors(
    prefix: str, suffix: str, bad_tasks: list[object]
) -> None:
    payload = {"tasks": bad_tasks, "todos": []}
    tool_calls = {
        "tc-plan-title": {
            "title": f"{prefix}{json.dumps(payload)}{suffix}",
            "status": "completed",
        }
    }

    tasks, todos, error = parse_proposed_plan(tool_calls)

    assert tasks == []
    assert todos is None
    assert error is not None


@pytest.mark.parametrize(
    "wrapped",
    [
        {"arguments": {"tasks": [{"title": "T", "type": "AUTO"}], "todos": []}},
        {"input": {"tasks": [{"title": "T", "type": "PAIR"}]}},
        {"params": {"tasks": [{"title": "T"}], "todos": []}},
        {"data": {"tasks": [{"title": "T"}]}},
    ],
)
def test_parse_proposed_plan_unwraps_nested_payloads(wrapped: dict[str, object]) -> None:
    tool_calls = {
        "tc-plan-title": {
            "title": f"propose_plan: {json.dumps(wrapped)}",
            "status": "completed",
        }
    }

    tasks, _todos, error = parse_proposed_plan(tool_calls)

    assert error is None
    assert len(tasks) == 1
    assert tasks[0].title == "T"


def test_parse_proposed_plan_prefers_richer_non_title_payload() -> None:
    preview_payload = {"tasks": [{"title": "Preview only"}]}
    full_payload = {
        "tasks": [
            {"title": "Initialize project", "type": "AUTO"},
            {"title": "Build API client", "type": "AUTO"},
            {"title": "Implement UI", "type": "PAIR"},
            {"title": "Wire configuration", "type": "AUTO"},
        ],
        "todos": [],
    }
    tool_calls = {
        "tc-preview": {
            "name": "propose_plan",
            "title": f"propose_plan: {json.dumps(preview_payload)}",
            "status": "completed",
        },
        "tc-full": {
            "name": "propose_plan",
            "status": "in_progress",
            "rawInput": json.dumps({"arguments": full_payload}),
        },
    }

    tasks, todos, error = parse_proposed_plan(tool_calls)

    assert error is None
    assert len(tasks) == 4
    assert tasks[0].title == "Initialize project"
    assert todos is None


def test_parse_proposed_plan_ignores_non_plan_tool_with_tasks_payload() -> None:
    tool_calls = {
        "tc-shell": {
            "name": "shell",
            "status": "completed",
            "title": 'shell: {"tasks":[{"title":"Should not parse"}]}',
            "content": [
                {
                    "type": "content",
                    "content": {
                        "type": "text",
                        "text": '{"tasks":[{"title":"Should not parse"}]}',
                    },
                }
            ],
        }
    }

    tasks, todos, error = parse_proposed_plan(tool_calls)

    assert tasks == []
    assert todos is None
    assert error is None
