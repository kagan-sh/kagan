"""Planner tool-call parsing helpers."""

from __future__ import annotations

import json
import re
from typing import TYPE_CHECKING, Any

from acp.schema import PlanEntry, ToolCall
from pydantic import ValidationError

from kagan.core.debug_log import log as debug_log
from kagan.core.mcp_naming import get_mcp_server_name

from .planner_models import PlanProposal

if TYPE_CHECKING:
    from collections.abc import Mapping

    from kagan.core.adapters.db.schema import Task


PLAN_SUBMIT_TOOL_NAME = "plan_submit"
_KAGAN_MCP_SERVER_NAME = get_mcp_server_name().strip().lower()
_PLAN_PAYLOAD_WRAPPER_KEYS = (
    "arguments",
    "args",
    "params",
    "input",
    "data",
    "payload",
    "tool_input",
    "rawInput",
    "raw_input",
    "tool",
    "call",
)


def parse_proposed_plan(
    tool_calls: Mapping[str, ToolCall | dict[str, Any]],
) -> tuple[list[Task], list[PlanEntry] | None, str | None]:
    """Parse proposed tasks from tool calls.

    Returns (tasks, todos, error). If no proposal is found, returns empty tasks and None.
    """
    if not tool_calls:
        return [], None, None

    debug_log.debug(
        "[PlannerParse] Received tool calls",
        count=len(tool_calls),
        ids=list(tool_calls.keys())[:6],
    )
    selected = _select_plan_submit_call(list(tool_calls.values()))
    if selected is None:
        debug_log.debug("[PlannerParse] No plan_submit call selected")
        return [], None, None

    payload_info = _extract_plan_payload_with_source(selected)
    if payload_info is None:
        debug_log.warning(
            "[PlannerParse] plan_submit selected but no readable payload",
            selected=_summarize_tool_call(selected),
        )
        return [], None, "plan_submit was called without readable arguments."
    payload, source = payload_info
    debug_log.debug(
        "[PlannerParse] Parsed payload candidate",
        source=source,
        selected=_summarize_tool_call(selected),
        payload=_preview_value(payload),
    )

    try:
        proposal = PlanProposal.model_validate(payload)
    except ValidationError as exc:
        debug_log.warning(
            "[PlannerParse] Plan validation failed",
            error=_format_plan_error(exc),
            source=source,
            payload=_preview_value(payload),
        )
        return [], None, _format_plan_error(exc)

    tasks = proposal.to_tasks()
    todos = proposal.to_plan_entries()
    debug_log.info(
        "[PlannerParse] Plan parsed successfully",
        source=source,
        task_count=len(tasks),
        todo_count=len(todos),
    )
    return tasks, todos or None, None


def _select_plan_submit_call(
    calls: list[ToolCall | dict[str, Any]],
) -> ToolCall | dict[str, Any] | None:
    # Match tool names ending with "plan_submit" to handle MCP prefixes
    # (e.g., "kagan_plan_submit", "mcp__kagan__plan_submit", "plan_submit").
    # Prefer higher-confidence payload sources and richer task lists over title previews.
    ranked_matches: list[tuple[int, int, int, int, ToolCall | dict[str, Any]]] = []
    for index, call in enumerate(calls):
        if not _is_kagan_plan_submit_call(call):
            continue
        payload_info = _extract_plan_payload_with_source(call)
        if payload_info is None:
            continue
        payload, source = payload_info
        tasks_value = payload.get("tasks")
        task_count = len(tasks_value) if isinstance(tasks_value, list) else 0
        source_rank = _payload_source_rank(source)
        status_rank = _tool_call_status_rank(_tool_call_status(call))
        ranked_matches.append((source_rank, task_count, status_rank, index, call))

    if ranked_matches:
        return max(ranked_matches)[-1]
    return None


def _is_kagan_plan_submit_call(tool_call: ToolCall | dict[str, Any]) -> bool:
    for raw_name in _candidate_tool_names(tool_call):
        raw_lower = raw_name.strip().lower()
        if raw_lower.startswith(f"mcp__{_KAGAN_MCP_SERVER_NAME}__{PLAN_SUBMIT_TOOL_NAME}"):
            return True
        if raw_lower.startswith(f"{_KAGAN_MCP_SERVER_NAME}_{PLAN_SUBMIT_TOOL_NAME}"):
            return True
        if raw_lower.startswith(PLAN_SUBMIT_TOOL_NAME):
            return True
        if (
            f"tool={PLAN_SUBMIT_TOOL_NAME}" in raw_lower
            or f"name={PLAN_SUBMIT_TOOL_NAME}" in raw_lower
            or f"toolname={PLAN_SUBMIT_TOOL_NAME}" in raw_lower
        ):
            return True

        normalized = _normalize_tool_name(raw_name)
        if normalized == PLAN_SUBMIT_TOOL_NAME:
            return True
        if normalized in {
            f"{_KAGAN_MCP_SERVER_NAME}_{PLAN_SUBMIT_TOOL_NAME}",
            f"mcp__{_KAGAN_MCP_SERVER_NAME}__{PLAN_SUBMIT_TOOL_NAME}",
        }:
            return True
    return False


def _candidate_tool_names(tool_call: ToolCall | dict[str, Any]) -> list[str]:
    values: list[str] = []
    for value in _iter_tool_name_values(tool_call):
        text = str(value).strip()
        if text:
            values.append(text)
    return values


def _tool_call_name(tool_call: ToolCall | dict[str, Any]) -> str:
    """Extract tool name from tool call, handling MCP prefixes."""
    for value in _iter_tool_name_values(tool_call):
        normalized = _normalize_tool_name(value)
        if normalized:
            return normalized
    return ""


def _iter_tool_name_values(tool_call: ToolCall | dict[str, Any]) -> list[object]:
    values: list[object] = []
    if isinstance(tool_call, ToolCall):
        if tool_call.title is not None:
            values.append(tool_call.title)
        raw_input = tool_call.raw_input
        if isinstance(raw_input, dict):
            values.extend(_name_field_values(raw_input))
        return values

    values.extend(_name_field_values(tool_call))

    meta = tool_call.get("_meta")
    if isinstance(meta, dict):
        claude_code = meta.get("claudeCode")
        if isinstance(claude_code, dict):
            tool_name = claude_code.get("toolName")
            if tool_name is not None:
                values.append(tool_name)

    tool_info = tool_call.get("tool")
    if isinstance(tool_info, dict):
        values.extend(_name_field_values(tool_info))

    return values


def _name_field_values(mapping: Mapping[str, Any]) -> list[object]:
    return [
        value for key in ("name", "toolName", "title") if (value := mapping.get(key)) is not None
    ]


def _extract_plan_payload_with_source(
    tool_call: ToolCall | dict[str, Any],
) -> tuple[dict[str, Any], str] | None:
    """Extract the plan payload from a tool call, supporting multiple protocols.

    Preference order:
    1. Echo-back content (Kagan MCP result with status + tasks) — most reliable
    2. rawInput / arguments — agent-formatted input
    3. Other content — non-echo-back text payloads
    4. title — last resort (often truncated)
    """
    # --- Phase 0: Check for echo-back in content/rawOutput (Kagan MCP result) ---
    # Echo-back payloads have "status" + "tasks" keys — produced by Kagan's MCP server.
    # These are the single source of truth; prefer them over agent-formatted input.
    echo = _extract_echo_back_payload(tool_call)
    if echo is not None:
        debug_log.debug("[PlannerParse] Using echo-back content as source of truth")
        return echo, "echo_back"

    for source, value in _iter_plan_payload_values(tool_call):
        payload = _parse_payload(value)
        if payload is not None and _payload_has_tasks_key(payload):
            return payload, source
    return None


def _is_echo_back_payload(payload: dict[str, Any]) -> bool:
    """Check if a payload is a Kagan MCP echo-back response.

    Echo-backs have "status" (e.g. "received") AND "tasks" list — produced by
    Kagan's MCP server, not by the agent.
    """
    return (
        "status" in payload and isinstance(payload.get("tasks"), list) and len(payload["tasks"]) > 0
    )


def _extract_echo_back_payload(
    tool_call: ToolCall | dict[str, Any],
) -> dict[str, Any] | None:
    """Extract echo-back payload from content/rawOutput if present.

    Returns the payload only if it's a genuine echo-back (has status + tasks).
    Summary-only responses (status + task_count but no tasks) are skipped.
    """
    for value in _iter_echo_back_payload_values(tool_call):
        payload = _parse_payload(value)
        if payload is not None and _is_echo_back_payload(payload):
            return payload
    return None


def _iter_plan_payload_values(
    tool_call: ToolCall | dict[str, Any],
) -> list[tuple[str, object]]:
    if isinstance(tool_call, ToolCall):
        values: list[tuple[str, object]] = [
            ("raw_input", tool_call.raw_input),
            ("raw_output", tool_call.raw_output),
        ]
        values.extend(("content", text) for text in _iter_content_text_values(tool_call))
        values.append(("title", tool_call.title))
        return values

    values = [
        (key, tool_call.get(key)) for key in ("rawInput", "arguments", "input", "params", "args")
    ]
    tool_info = tool_call.get("tool")
    if isinstance(tool_info, dict):
        values.extend(
            (f"tool.{key}", tool_info.get(key))
            for key in ("rawInput", "arguments", "input", "params", "args")
        )
    values.extend(("content", text) for text in _iter_content_text_values(tool_call))
    values.append(("title", tool_call.get("title")))
    if isinstance(tool_info, dict):
        values.append(("tool.title", tool_info.get("title")))
    return values


def _iter_echo_back_payload_values(tool_call: ToolCall | dict[str, Any]) -> list[object]:
    if isinstance(tool_call, ToolCall):
        return [tool_call.raw_output, *_iter_content_text_values(tool_call)]
    return [tool_call.get("rawOutput"), *_iter_content_text_values(tool_call)]


def _iter_content_text_values(tool_call: ToolCall | dict[str, Any]) -> list[object]:
    values: list[object] = []
    if isinstance(tool_call, ToolCall):
        for item in tool_call.content or []:
            if getattr(item, "type", None) != "content":
                continue
            sub_content = item.content
            if getattr(sub_content, "type", None) != "text":
                continue
            values.append(sub_content.text)
        return values

    content_list = tool_call.get("content")
    if not isinstance(content_list, list):
        return values
    for item in content_list:
        if not isinstance(item, dict) or item.get("type") != "content":
            continue
        content = item.get("content")
        if not isinstance(content, dict) or content.get("type") != "text":
            continue
        values.append(content.get("text"))
    return values


def _normalize_tool_name(value: Any) -> str:
    name = str(value).strip().lower()
    if "__" in name:
        name = name.split("__")[-1]
    match = re.match(r"[a-z0-9_./-]+", name)
    return match.group(0) if match else name


def _tool_call_status(tool_call: ToolCall | dict[str, Any]) -> str:
    if isinstance(tool_call, ToolCall):
        return str(tool_call.status or "").lower()
    return str(tool_call.get("status", "")).lower()


def _tool_call_status_rank(status: str) -> int:
    if status == "completed":
        return 2
    if status == "in_progress":
        return 1
    return 0


def _payload_source_rank(source: str) -> int:
    if source == "echo_back":
        return 6
    if source.startswith("raw_input") or source.startswith("rawInput"):
        return 5
    if source in {"arguments", "input", "params", "args"}:
        return 4
    if source.startswith("tool.") and source.split(".", 1)[1] in {
        "rawInput",
        "arguments",
        "input",
        "params",
        "args",
    }:
        return 4
    if source == "raw_output":
        return 3
    if source == "content":
        return 2
    if source.endswith("title"):
        return 1
    return 0


def _parse_payload(value: Any) -> dict[str, Any] | None:
    parsed = _parse_payload_candidate(value)
    if parsed is None:
        return None
    return _normalize_plan_payload(parsed)


def _parse_payload_candidate(value: Any) -> dict[str, Any] | None:
    if isinstance(value, dict):
        return value
    if isinstance(value, str):
        try:
            parsed = json.loads(value)
        except json.JSONDecodeError:
            return _extract_json_object(value)
        if isinstance(parsed, dict):
            return parsed
    return None


def _normalize_plan_payload(payload: dict[str, Any]) -> dict[str, Any]:
    if "tasks" in payload:
        return payload

    visited: set[int] = set()
    stack: list[dict[str, Any]] = [payload]

    while stack:
        current = stack.pop()
        current_id = id(current)
        if current_id in visited:
            continue
        visited.add(current_id)

        if "tasks" in current:
            return current

        for key in _PLAN_PAYLOAD_WRAPPER_KEYS:
            nested = _parse_payload_candidate(current.get(key))
            if nested is not None:
                stack.append(nested)

    return payload


def _payload_has_tasks_key(payload: dict[str, Any]) -> bool:
    return "tasks" in payload and isinstance(payload["tasks"], list)


def _extract_json_object(text: str) -> dict[str, Any] | None:
    """Extract the first JSON object from a string."""
    in_string = False
    escape = False
    depth = 0
    start: int | None = None

    for idx, ch in enumerate(text):
        if in_string:
            if escape:
                escape = False
            elif ch == "\\":
                escape = True
            elif ch == '"':
                in_string = False
            continue

        if ch == '"':
            in_string = True
            continue
        if ch == "{":
            if depth == 0:
                start = idx
            depth += 1
            continue
        if ch == "}" and depth > 0:
            depth -= 1
            if depth == 0 and start is not None:
                candidate = text[start : idx + 1]
                try:
                    parsed = json.loads(candidate)
                except json.JSONDecodeError:
                    start = None
                    continue
                if isinstance(parsed, dict):
                    return parsed
                start = None
    return None


def _format_plan_error(error: ValidationError) -> str:
    issues = error.errors()
    snippets: list[str] = []
    for issue in issues[:3]:
        loc = ".".join(str(part) for part in issue.get("loc", []))
        msg = issue.get("msg", "Invalid value")
        snippets.append(f"{loc}: {msg}".strip(": "))
    suffix = "..." if len(issues) > 3 else ""
    details = "; ".join(snippets) if snippets else "Invalid plan proposal."
    issue_count = len(issues)
    return (
        f"Invalid plan proposal ({issue_count} issue{'s' if issue_count != 1 else ''}): "
        f"{details}{suffix}"
    )


def _preview_value(value: Any, max_chars: int = 600) -> str:
    if isinstance(value, str):
        text = value
    else:
        try:
            text = json.dumps(value, ensure_ascii=True, default=str)
        except (TypeError, ValueError):
            text = repr(value)
    compact = " ".join(text.split())
    if len(compact) <= max_chars:
        return compact
    return compact[: max_chars - 3] + "..."


def _summarize_tool_call(tool_call: ToolCall | dict[str, Any]) -> dict[str, Any]:
    if isinstance(tool_call, ToolCall):
        return {
            "name": _tool_call_name(tool_call),
            "status": _tool_call_status(tool_call),
            "title": _preview_value(tool_call.title or "", max_chars=120),
            "raw_input": _preview_value(tool_call.raw_input, max_chars=220),
            "raw_output": _preview_value(tool_call.raw_output, max_chars=220),
        }

    return {
        "name": _tool_call_name(tool_call),
        "status": _tool_call_status(tool_call),
        "title": _preview_value(tool_call.get("title"), max_chars=120),
        "rawInput": _preview_value(tool_call.get("rawInput"), max_chars=220),
        "arguments": _preview_value(tool_call.get("arguments"), max_chars=220),
        "input": _preview_value(tool_call.get("input"), max_chars=220),
        "params": _preview_value(tool_call.get("params"), max_chars=220),
    }


__all__ = [
    "parse_proposed_plan",
]
