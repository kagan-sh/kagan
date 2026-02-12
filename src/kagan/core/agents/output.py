"""Output serialization utilities for agent data."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Sequence

    from kagan.core.acp import Agent


def _json_safe(value: Any) -> Any:
    """Convert ACP/Pydantic payloads into JSON-serializable primitives."""
    if value is None:
        return None
    if hasattr(value, "model_dump"):
        return _json_safe(value.model_dump(by_alias=True, exclude_none=True))
    if isinstance(value, dict):
        return {k: _json_safe(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_json_safe(v) for v in value]
    return value


def _serialize_messages(
    buffered_messages: Sequence[object],
    *,
    include_thinking: bool = False,
    compact_streamed_text: bool = True,
) -> list[dict[str, Any]]:
    """Normalize agent message objects into persisted message dictionaries."""
    from kagan.core.acp import messages as msg_types

    serialized_messages: list[dict[str, Any]] = []
    for message in buffered_messages:
        if isinstance(message, msg_types.AgentUpdate):
            if not message.text:
                continue
            # Compact mode stores streamed text via final response only.
            if compact_streamed_text and message.content_type == "text":
                continue
            serialized_messages.append({"type": "response", "content": message.text})
        elif isinstance(message, msg_types.Thinking):
            if include_thinking and message.text:
                serialized_messages.append({"type": "thinking", "content": message.text})
        elif isinstance(message, msg_types.ToolCall):
            serialized_messages.append(
                {
                    "type": "tool_call",
                    "id": message.tool_call.tool_call_id,
                    "title": message.tool_call.title,
                    "kind": message.tool_call.kind or "",
                    "status": message.tool_call.status or "",
                    "content": _json_safe(message.tool_call.content) or [],
                    "raw_input": _json_safe(message.tool_call.raw_input),
                    "raw_output": _json_safe(message.tool_call.raw_output),
                }
            )
        elif isinstance(message, msg_types.ToolCallUpdate):
            serialized_messages.append(
                {
                    "type": "tool_call_update",
                    "id": message.update.tool_call_id,
                    "status": message.update.status or "",
                    "title": message.tool_call.title,
                    "kind": message.tool_call.kind or "",
                    "content": _json_safe(message.tool_call.content) or [],
                    "raw_input": _json_safe(message.tool_call.raw_input),
                    "raw_output": _json_safe(message.tool_call.raw_output),
                }
            )
        elif isinstance(message, msg_types.Plan):
            serialized_messages.append(
                {
                    "type": "plan",
                    "entries": [e.model_dump() for e in message.entries] if message.entries else [],
                }
            )
        elif isinstance(message, msg_types.AgentReady):
            serialized_messages.append({"type": "agent_ready"})
        elif isinstance(message, msg_types.AgentFail):
            serialized_messages.append(
                {
                    "type": "agent_fail",
                    "message": message.message,
                    "details": message.details,
                }
            )
    return serialized_messages


def serialize_agent_output(agent: Agent, *, include_thinking: bool = False) -> str:
    """Serialize agent output into a compact JSON payload.

    Compact mode keeps high-signal events (tool calls, plans, failures) and stores
    the final response once, instead of persisting every streamed response chunk.
    """
    serialized_messages = _serialize_messages(
        agent.get_messages(),
        include_thinking=include_thinking,
        compact_streamed_text=True,
    )

    response_text = agent.get_response_text()
    if response_text:
        serialized_messages.append({"type": "response", "content": response_text})

    return json.dumps({"messages": serialized_messages})


def serialize_agent_messages(
    buffered_messages: Sequence[object],
    *,
    include_thinking: bool = False,
) -> str | None:
    """Serialize a message slice for incremental persistence during active runs."""
    serialized_messages = _serialize_messages(
        buffered_messages,
        include_thinking=include_thinking,
        compact_streamed_text=False,
    )
    if not serialized_messages:
        return None
    return json.dumps({"messages": serialized_messages})


def build_merge_conflict_note(
    original_error: str,
    rebase_success: bool,
    rebase_msg: str,
    conflict_files: list[str],
    files_on_base: list[str],
    base_branch: str,
) -> str:
    """Build a detailed scratchpad note about merge conflict for agent context."""
    lines = [
        "\n\n--- MERGE CONFLICT - AUTO MERGE ---",
        f"Original merge error: {original_error}",
        "",
    ]

    if rebase_success:
        lines.append(f"✓ Successfully rebased onto origin/{base_branch}")
        lines.append("The branch is now up to date. Please verify changes and signal COMPLETE.")
    else:
        lines.append(f"⚠ Rebase onto origin/{base_branch} had conflicts: {rebase_msg}")
        lines.append("")
        lines.append("ACTION REQUIRED: You need to manually resolve the conflicts.")
        lines.append("")
        lines.append("Steps to resolve:")
        lines.append(f"1. Run: git fetch origin {base_branch}")
        lines.append(f"2. Run: git rebase origin/{base_branch}")
        lines.append("3. For each conflict, edit the file to resolve, then: git add <file>")
        lines.append("4. Run: git rebase --continue")
        lines.append("5. Once resolved, signal COMPLETE to re-attempt the merge")

    if conflict_files:
        lines.append("")
        lines.append("Files with conflicts:")
        for f in conflict_files[:10]:
            lines.append(f"  - {f}")
        if len(conflict_files) > 10:
            lines.append(f"  ... and {len(conflict_files) - 10} more")

    if files_on_base:
        lines.append("")
        lines.append(f"Files recently changed on {base_branch} (potential conflict sources):")
        for f in files_on_base[:10]:
            lines.append(f"  - {f}")
        if len(files_on_base) > 10:
            lines.append(f"  ... and {len(files_on_base) - 10} more")

    lines.append("")
    lines.append("---")
    lines.append("")

    return "\n".join(lines)
