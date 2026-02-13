"""Tests for MCP tool annotations."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.mcp.server import _create_mcp_server

if TYPE_CHECKING:
    from mcp.types import ToolAnnotations


def _tool_annotations(mcp: object) -> dict[str, ToolAnnotations | None]:
    tool_manager = mcp._tool_manager  # type: ignore[attr-defined]  # quality-allow-private
    return {name: tool.annotations for name, tool in tool_manager._tools.items()}


@pytest.fixture()
def full_mcp():
    return _create_mcp_server(readonly=False)


_ANNOTATION_MATRIX: dict[str, tuple[bool, bool, bool]] = {
    "get_task": (True, False, True),
    "tasks_list": (True, False, True),
    "tasks_wait": (True, False, True),
    "sessions_exists": (True, False, True),
    "projects_list": (True, False, True),
    "repos_list": (True, False, True),
    "audit_tail": (True, False, True),
    "settings_get": (True, False, True),
    "get_context": (True, False, True),
    "propose_plan": (False, False, False),
    "update_scratchpad": (False, False, False),
    "request_review": (False, False, False),
    "tasks_create": (False, False, False),
    "tasks_update": (False, False, False),
    "tasks_move": (False, False, False),
    "jobs_submit": (False, False, False),
    "jobs_get": (True, False, True),
    "jobs_wait": (True, False, True),
    "jobs_events": (True, False, True),
    "jobs_list_actions": (True, False, True),
    "jobs_cancel": (False, False, False),
    "sessions_create": (False, False, False),
    "sessions_kill": (False, False, False),
    "settings_update": (False, False, False),
    "projects_create": (False, False, False),
    "projects_open": (False, False, False),
    "tasks_delete": (False, True, False),
    "review": (False, True, False),
}

_FULL_MODE_ANNOTATION_MATRIX: dict[str, tuple[bool, bool, bool]] = {
    name: annotation for name, annotation in _ANNOTATION_MATRIX.items() if name != "propose_plan"
}

_READONLY_TOOLS = {
    "propose_plan",
    "get_task",
    "tasks_list",
    "tasks_wait",
    "projects_list",
    "repos_list",
    "audit_tail",
}


@pytest.mark.parametrize(
    ("tool_name", "expected"),
    sorted(_FULL_MODE_ANNOTATION_MATRIX.items()),
)
def test_full_mode_tool_annotations(
    full_mcp,
    tool_name: str,
    expected: tuple[bool, bool, bool],
) -> None:
    annotations = _tool_annotations(full_mcp)
    annotation = annotations.get(tool_name)

    assert annotation is not None, f"Tool {tool_name} is missing annotations"
    expected_read_only, expected_destructive, expected_idempotent = expected
    assert annotation.readOnlyHint is expected_read_only
    assert annotation.destructiveHint is expected_destructive
    assert annotation.idempotentHint is expected_idempotent


def test_readonly_mode_annotations_match_expected_subset() -> None:
    annotations = _tool_annotations(_create_mcp_server(readonly=True))
    assert set(annotations) == _READONLY_TOOLS
    for tool_name in _READONLY_TOOLS:
        annotation = annotations[tool_name]
        assert annotation is not None
        expected_read_only, expected_destructive, expected_idempotent = _ANNOTATION_MATRIX[
            tool_name
        ]
        assert annotation.readOnlyHint is expected_read_only
        assert annotation.destructiveHint is expected_destructive
        assert annotation.idempotentHint is expected_idempotent
