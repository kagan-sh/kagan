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
    "task_get": (True, False, True),
    "task_logs": (True, False, True),
    "task_list": (True, False, True),
    "task_wait": (True, False, True),
    "project_list": (True, False, True),
    "repo_list": (True, False, True),
    "audit_list": (True, False, True),
    "settings_get": (True, False, True),
    "plan_submit": (False, False, False),
    "task_create": (False, False, False),
    "task_patch": (False, False, False),
    "task_delete": (False, True, False),
    "job_start": (False, False, False),
    "job_poll": (True, False, True),
    "job_cancel": (False, False, False),
    "session_manage": (False, False, False),
    "project_open": (False, False, False),
    "settings_set": (False, False, False),
    "review_apply": (False, True, False),
    # GitHub plugin admin tools (V1 contract)
    "kagan_github_contract_probe": (True, False, True),
    "kagan_github_connect_repo": (False, False, False),
    "kagan_github_sync_issues": (False, False, False),
}

_FULL_MODE_ANNOTATION_MATRIX: dict[str, tuple[bool, bool, bool]] = {
    name: annotation for name, annotation in _ANNOTATION_MATRIX.items() if name != "plan_submit"
}

_READONLY_TOOLS = {
    "plan_submit",
    "task_get",
    "task_logs",
    "task_list",
    "task_wait",
    "project_list",
    "repo_list",
    "audit_list",
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
