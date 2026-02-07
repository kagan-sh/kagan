"""Output serialization utilities for agent data."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from kagan.acp.agent import Agent


def serialize_agent_output(agent: Agent) -> str:
    """Serialize agent output including tool calls, thinking, and response to JSON."""
    from kagan.acp import messages as msg_types

    serialized_messages: list[dict[str, Any]] = []
    for message in agent._buffers.messages:
        if isinstance(message, msg_types.AgentUpdate):
            serialized_messages.append({"type": "response", "content": message.text})
        elif isinstance(message, msg_types.Thinking):
            serialized_messages.append({"type": "thinking", "content": message.text})
        elif isinstance(message, msg_types.ToolCall):
            serialized_messages.append(
                {
                    "type": "tool_call",
                    "id": str(message.tool_call.get("id", "")),
                    "title": str(message.tool_call.get("title", "")),
                    "kind": str(message.tool_call.get("kind", "")),
                }
            )
        elif isinstance(message, msg_types.ToolCallUpdate):
            serialized_messages.append(
                {
                    "type": "tool_call_update",
                    "id": str(message.update.get("id", "")),
                    "status": str(message.update.get("status", "")),
                }
            )
        elif isinstance(message, msg_types.Plan):
            serialized_messages.append(
                {
                    "type": "plan",
                    "entries": [dict(e) for e in message.entries] if message.entries else [],
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

    return json.dumps(
        {
            "messages": serialized_messages,
            "response_text": agent.get_response_text(),
        }
    )


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
        "\n\n--- MERGE CONFLICT - AUTO RETRY ---",
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
        lines.append("5. Once resolved, signal COMPLETE to retry the merge")

    if conflict_files:
        lines.append("")
        lines.append("Files with conflicts:")
        for f in conflict_files[:10]:  # Limit to first 10
            lines.append(f"  - {f}")
        if len(conflict_files) > 10:
            lines.append(f"  ... and {len(conflict_files) - 10} more")

    if files_on_base:
        lines.append("")
        lines.append(f"Files recently changed on {base_branch} (potential conflict sources):")
        for f in files_on_base[:10]:  # Limit to first 10
            lines.append(f"  - {f}")
        if len(files_on_base) > 10:
            lines.append(f"  ... and {len(files_on_base) - 10} more")

    lines.append("")
    lines.append("---")
    lines.append("")

    return "\n".join(lines)
