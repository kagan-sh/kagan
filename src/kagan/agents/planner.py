"""Planner agent support for task generation from natural language."""

from __future__ import annotations

import json
import re
from typing import TYPE_CHECKING, Any, Literal

from acp.schema import PlanEntry, ToolCall
from pydantic import BaseModel, ConfigDict, Field, ValidationError, field_validator

from kagan.core.models.entities import Task
from kagan.core.models.enums import ChatRole, TaskPriority, TaskStatus, TaskType
from kagan.debug_log import log as debug_log
from kagan.mcp_naming import get_mcp_server_name

if TYPE_CHECKING:
    from collections.abc import Mapping


PLANNER_PROMPT = """\
You are a Planning Specialist that designs well-scoped units of work as development tasks.

## Core Principles

- Iterative refinement: draft, check, refine.
- Clarity & specificity: concise, unambiguous, structured output.
- Learning by example: follow the example patterns below.
- Structured reasoning: think step by step internally, then summarize as concise todos.

## Safety & Secrets

Never access or request secrets/credentials/keys (e.g., `.env`, `.env.*`, `id_rsa`,
`*.pem`, `*.key`, `credentials.json`). If the request depends on secrets, ask for
redacted values or suggest safe mock inputs.

## Your Role

You analyze requests and propose tasks for workers to execute later.

Your outputs are limited to:
- Clarifying questions (when requests are ambiguous)
- A single tool call to `propose_plan` with the tasks and todos
- A short confirmation sentence after the tool call

When a user requests "create a script" or "write code", design a task
describing what a worker should build.

## Output Contract (Tool Call)

Always call the MCP tool `propose_plan` exactly once with structured arguments.
After the tool call, reply with one short confirmation sentence.

Tool arguments:
- tasks: list of task objects
  - title: string (verb + clear objective)
  - type: "AUTO" or "PAIR"
  - description: string (what to build and why)
  - acceptance_criteria: list of strings (2-5 testable conditions)
  - priority: "low" | "medium" | "high"
- todos: list of todo objects (3-6 items)
  - content: short summary of the planning steps
  - status: "pending" | "in_progress" | "completed" | "failed"

## Task Design Guidelines

1. **Title**: Start with a verb (Create, Implement, Fix, Add, Update, Refactor)
2. **Description**: Provide enough context for a developer to understand the task
3. **Acceptance Criteria**: Include 2-5 testable conditions that define completion
4. **Scope**: Each task represents one focused unit of work

## Task Types

**AUTO** - Worker agent completes autonomously:
- Bug fixes with clear reproduction steps
- Adding logging, metrics, or validation
- Writing tests for existing code
- Code refactoring with defined scope
- Dependency updates

**PAIR** - Requires human collaboration:
- New feature design decisions
- UX/UI choices
- API contract design
- Architecture decisions
- Security-sensitive changes

## Priority Levels

- **high**: Blocking issues, security vulnerabilities
- **medium**: Standard feature work, improvements
- **low**: Nice-to-have, cleanup tasks

## Workflow

Let's think step by step for complex requests.

1. Analyze the request for clarity
2. Ask 1-2 clarifying questions if the scope is ambiguous
3. Break complex requests into 2-5 focused tasks
4. Provide concise todos for the planning steps
5. Call `propose_plan` with the tasks and todos

## Examples

### Example 1: Bug Fix

User: "Login button doesn't work on mobile"

Tool call arguments:
{
  "tasks": [
    {
      "title": "Fix mobile login button tap handling",
      "type": "AUTO",
      "description": "Investigate touch/click handlers and CSS layering to ensure taps trigger "
        "login on mobile.",
      "acceptance_criteria": [
        "Login button responds to tap on iOS Safari and Android Chrome",
        "Login action triggers network request when button is tapped",
        "No visual overlap blocks the button on mobile breakpoints"
      ],
      "priority": "high"
    }
  ],
  "todos": [
    {"content": "Analyze the bug report scope", "status": "completed"},
    {"content": "Identify likely mobile interaction issues", "status": "completed"},
    {"content": "Define testable acceptance criteria", "status": "completed"}
  ]
}

### Example 2: Feature Work

User: "Add a dark mode toggle"

Tool call arguments:
{
  "tasks": [
    {
      "title": "Add dark mode theme variables",
      "type": "AUTO",
      "description": "Create CSS variables for dark mode and apply to core UI components.",
      "acceptance_criteria": [
        "Dark mode variables exist for background, text, and accents",
        "Core UI components use the new variables",
        "Theme can be toggled via a single class or attribute"
      ],
      "priority": "medium"
    },
    {
      "title": "Add UI toggle for dark mode",
      "type": "PAIR",
      "description": "Add a toggle in settings or header; decide placement and UX with a human "
        "partner.",
      "acceptance_criteria": [
        "Toggle is visible and accessible in the UI",
        "Toggle switches between light and dark themes",
        "Theme preference persists across reloads"
      ],
      "priority": "medium"
    }
  ],
  "todos": [
    {"content": "Determine scope and key UI surfaces", "status": "completed"},
    {"content": "Split into styling and UX tasks", "status": "completed"},
    {"content": "Define acceptance criteria", "status": "completed"}
  ]
}

### Example 3: Refactor

User: "Refactor the API client to reduce duplication"

Tool call arguments:
{
  "tasks": [
    {
      "title": "Refactor API client request helpers",
      "type": "AUTO",
      "description": "Consolidate duplicate request logic into shared helpers with clear "
        "interfaces.",
      "acceptance_criteria": [
        "Shared helper handles auth headers and transient failure handling",
        "Existing callers migrated to the helper",
        "No behavior regressions in existing API calls"
      ],
      "priority": "low"
    }
  ],
  "todos": [
    {"content": "Identify repeated API client patterns", "status": "completed"},
    {"content": "Design a shared helper interface", "status": "completed"},
    {"content": "Define regression-safe acceptance criteria", "status": "completed"}
  ]
}

{conversation_context}## User Request

<input>
{user_request}
</input>

Call `propose_plan` with the tasks and todos now.
"""

PROPOSE_PLAN_TOOL_NAME = "propose_plan"
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


class ProposedTodo(BaseModel):
    """Planner todo entry for plan display."""

    model_config = ConfigDict(extra="ignore")

    content: str = Field(..., min_length=1, max_length=200)
    status: Literal["pending", "in_progress", "completed", "failed"] = "completed"

    @field_validator("content", mode="before")
    @classmethod
    def _clean_content(cls, v: Any) -> str:
        if v is None:
            return ""
        return str(v).strip()

    @field_validator("status", mode="before")
    @classmethod
    def _normalize_status(cls, v: Any) -> str:
        if v is None:
            return "pending"
        value = str(v).lower()
        if value in ("pending", "in_progress", "completed", "failed"):
            return value
        return "pending"


class ProposedTask(BaseModel):
    """Planner task proposal parsed from tool call arguments."""

    model_config = ConfigDict(extra="ignore")

    title: str = Field(..., min_length=1, max_length=200)
    type: Literal["AUTO", "PAIR"] = "PAIR"
    description: str = Field("", max_length=10000)
    acceptance_criteria: list[str] = Field(default_factory=list)
    priority: Literal["low", "medium", "high"] = "medium"

    @field_validator("title", mode="before")
    @classmethod
    def _clean_title(cls, v: Any) -> str:
        if v is None:
            return ""
        return str(v).strip()

    @field_validator("description", mode="before")
    @classmethod
    def _clean_description(cls, v: Any) -> str:
        if v is None:
            return ""
        return str(v).strip()

    @field_validator("type", mode="before")
    @classmethod
    def _normalize_type(cls, v: Any) -> str:
        if v is None:
            return "PAIR"
        value = str(v).upper()
        return "AUTO" if value == "AUTO" else "PAIR"

    @field_validator("priority", mode="before")
    @classmethod
    def _normalize_priority(cls, v: Any) -> str:
        if v is None:
            return "medium"
        value = str(v).lower()
        if value in ("low", "medium", "high"):
            return value
        return "medium"

    @field_validator("acceptance_criteria", mode="before")
    @classmethod
    def _coerce_criteria(cls, v: Any) -> list[str]:
        if v is None:
            return []
        if isinstance(v, str):
            return [v]
        if isinstance(v, list):
            return v
        return [str(v)]

    @field_validator("acceptance_criteria")
    @classmethod
    def _clean_criteria(cls, v: list[str]) -> list[str]:
        cleaned: list[str] = []
        for item in v:
            text = str(item).strip()
            if text:
                cleaned.append(text)
        return cleaned


class PlanProposal(BaseModel):
    """Validated plan proposal from the planner tool call."""

    model_config = ConfigDict(extra="ignore")

    tasks: list[ProposedTask] = Field(..., min_length=1)
    todos: list[ProposedTodo] = Field(default_factory=list)

    @field_validator("todos", mode="before")
    @classmethod
    def _coerce_todos(cls, v: Any) -> list[Any]:
        """Coerce invalid todos input to empty list (LLM might send wrong type)."""
        if v is None:
            return []
        if isinstance(v, list):
            return v

        return []

    def to_tasks(self) -> list[Task]:
        """Convert proposed tasks into Task models."""
        from datetime import datetime
        from uuid import uuid4

        tasks: list[Task] = []
        for item in self.tasks:
            task_type = TaskType.AUTO if item.type == "AUTO" else TaskType.PAIR
            priority_map = {
                "low": TaskPriority.LOW,
                "medium": TaskPriority.MEDIUM,
                "high": TaskPriority.HIGH,
            }
            now = datetime.now()
            tasks.append(
                Task(
                    id=uuid4().hex[:8],
                    project_id="plan",
                    title=item.title[:200],
                    description=item.description,
                    status=TaskStatus.BACKLOG,
                    priority=priority_map.get(item.priority, TaskPriority.MEDIUM),
                    task_type=task_type,
                    assigned_hat=None,
                    agent_backend=None,
                    parent_id=None,
                    acceptance_criteria=item.acceptance_criteria,
                    created_at=now,
                    updated_at=now,
                )
            )
        return tasks

    def to_plan_entries(self) -> list[PlanEntry]:
        """Convert todos to plan display entries."""
        entries: list[PlanEntry] = []
        for todo in self.todos:
            status = "completed" if todo.status == "failed" else todo.status
            entries.append(PlanEntry(content=todo.content, status=status, priority="medium"))
        return entries


def build_planner_prompt(
    user_input: str,
    conversation_history: list[tuple[str, str]] | None = None,
) -> str:
    """Build the prompt for the planner agent.

    Args:
        user_input: The user's natural language request.
        conversation_history: Optional list of (role, content) tuples for context.

    Returns:
        Formatted prompt for the planner.
    """

    context_section = ""
    if conversation_history:
        context_parts = []
        for role, content in conversation_history:
            if role == ChatRole.USER:
                context_parts.append(f"User: {content}")
            else:
                truncated = content[:2000] + "..." if len(content) > 2000 else content
                context_parts.append(f"Assistant: {truncated}")

        context_section = f"""
## Previous Conversation

{chr(10).join(context_parts)}

---

"""

    return PLANNER_PROMPT.replace("{conversation_context}", context_section).replace(
        "{user_request}", user_input
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
    selected = _select_propose_plan_call(list(tool_calls.values()))
    if selected is None:
        debug_log.debug("[PlannerParse] No propose_plan call selected")
        return [], None, None

    payload_info = _extract_plan_payload_with_source(selected)
    if payload_info is None:
        debug_log.warning(
            "[PlannerParse] propose_plan selected but no readable payload",
            selected=_summarize_tool_call(selected),
        )
        return [], None, "propose_plan was called without readable arguments."
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


def _select_propose_plan_call(
    calls: list[ToolCall | dict[str, Any]],
) -> ToolCall | dict[str, Any] | None:
    # Match tool names ending with "propose_plan" to handle MCP prefixes
    # (e.g., "kagan_propose_plan", "mcp__kagan__propose_plan", "propose_plan").
    # Prefer higher-confidence payload sources and richer task lists over title previews.
    ranked_matches: list[tuple[int, int, int, int, ToolCall | dict[str, Any]]] = []
    for index, call in enumerate(calls):
        if not _is_kagan_propose_plan_call(call):
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


def _is_kagan_propose_plan_call(tool_call: ToolCall | dict[str, Any]) -> bool:
    for raw_name in _candidate_tool_names(tool_call):
        raw_lower = raw_name.strip().lower()
        if raw_lower.startswith(f"mcp__{_KAGAN_MCP_SERVER_NAME}__{PROPOSE_PLAN_TOOL_NAME}"):
            return True
        if raw_lower.startswith(f"{_KAGAN_MCP_SERVER_NAME}_{PROPOSE_PLAN_TOOL_NAME}"):
            return True
        if raw_lower.startswith(PROPOSE_PLAN_TOOL_NAME):
            return True
        if (
            f"tool={PROPOSE_PLAN_TOOL_NAME}" in raw_lower
            or f"name={PROPOSE_PLAN_TOOL_NAME}" in raw_lower
            or f"toolname={PROPOSE_PLAN_TOOL_NAME}" in raw_lower
        ):
            return True

        normalized = _normalize_tool_name(raw_name)
        if normalized == PROPOSE_PLAN_TOOL_NAME:
            return True
        if normalized in {
            f"{_KAGAN_MCP_SERVER_NAME}_{PROPOSE_PLAN_TOOL_NAME}",
            f"mcp__{_KAGAN_MCP_SERVER_NAME}__{PROPOSE_PLAN_TOOL_NAME}",
        }:
            return True
    return False


def _candidate_tool_names(tool_call: ToolCall | dict[str, Any]) -> list[str]:
    values: list[str] = []

    def _append_name(value: Any) -> None:
        text = str(value).strip()
        if text:
            values.append(text)

    if isinstance(tool_call, ToolCall):
        if tool_call.title is not None:
            _append_name(tool_call.title)
        raw_input = tool_call.raw_input
        if isinstance(raw_input, dict):
            for key in ("name", "toolName", "title"):
                if key in raw_input and raw_input[key] is not None:
                    _append_name(raw_input[key])
        return values

    for key in ("name", "toolName", "title"):
        if key in tool_call and tool_call[key] is not None:
            _append_name(tool_call[key])

    meta = tool_call.get("_meta")
    if isinstance(meta, dict):
        claude_code = meta.get("claudeCode")
        if isinstance(claude_code, dict):
            tool_name = claude_code.get("toolName")
            if tool_name is not None:
                _append_name(tool_name)

    tool_info = tool_call.get("tool")
    if isinstance(tool_info, dict):
        for key in ("name", "toolName", "title"):
            if key in tool_info and tool_info[key] is not None:
                _append_name(tool_info[key])

    return values


def _tool_call_name(tool_call: ToolCall | dict[str, Any]) -> str:
    """Extract tool name from tool call, handling MCP prefixes."""
    if isinstance(tool_call, ToolCall):
        name = _normalize_tool_name(tool_call.title)
        if name:
            return name
        raw_input = tool_call.raw_input
        if isinstance(raw_input, dict):
            for key in ("name", "toolName", "title"):
                if key in raw_input and raw_input[key] is not None:
                    return _normalize_tool_name(raw_input[key])
        return ""

    for key in ("name", "toolName", "title"):
        if key in tool_call and tool_call[key] is not None:
            return _normalize_tool_name(tool_call[key])

    meta = tool_call.get("_meta")
    if isinstance(meta, dict):
        claude_code = meta.get("claudeCode")
        if isinstance(claude_code, dict):
            tool_name = claude_code.get("toolName")
            if tool_name is not None:
                return _normalize_tool_name(tool_name)

    tool_info = tool_call.get("tool")
    if isinstance(tool_info, dict):
        for key in ("name", "toolName", "title"):
            if key in tool_info and tool_info[key] is not None:
                return _normalize_tool_name(tool_info[key])
    return ""


def _extract_plan_payload(tool_call: ToolCall | dict[str, Any]) -> dict[str, Any] | None:
    payload_info = _extract_plan_payload_with_source(tool_call)
    return payload_info[0] if payload_info is not None else None


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
    candidates: list[tuple[dict[str, Any], str]] = []

    def _record_candidate(
        payload: dict[str, Any], source: str
    ) -> tuple[dict[str, Any], str] | None:
        candidates.append((payload, source))
        if _payload_has_tasks_key(payload):
            return payload, source
        return None

    # --- Phase 0: Check for echo-back in content/rawOutput (Kagan MCP result) ---
    # Echo-back payloads have "status" + "tasks" keys — produced by Kagan's MCP server.
    # These are the single source of truth; prefer them over agent-formatted input.
    echo = _extract_echo_back_payload(tool_call)
    if echo is not None:
        debug_log.debug("[PlannerParse] Using echo-back content as source of truth")
        return echo, "echo_back"

    if isinstance(tool_call, ToolCall):
        source_values = (
            ("raw_input", tool_call.raw_input),
            ("raw_output", tool_call.raw_output),
        )
        for source, value in source_values:
            payload = _parse_payload(value)
            if payload is not None:
                matched = _record_candidate(payload, source)
                if matched is not None:
                    return matched
        content_list = tool_call.content or []
        for item in content_list:
            if getattr(item, "type", None) != "content":
                continue
            sub_content = item.content
            if getattr(sub_content, "type", None) != "text":
                continue
            payload = _parse_payload(sub_content.text)
            if payload is not None:
                matched = _record_candidate(payload, "content")
                if matched is not None:
                    return matched
        payload = _parse_payload(tool_call.title)
        if payload is not None:
            matched = _record_candidate(payload, "title")
            if matched is not None:
                return matched
        return None

    for key in ("rawInput", "arguments", "input", "params", "args"):
        payload = _parse_payload(tool_call.get(key))
        if payload is not None:
            matched = _record_candidate(payload, key)
            if matched is not None:
                return matched

    tool_info = tool_call.get("tool")
    if isinstance(tool_info, dict):
        for key in ("rawInput", "arguments", "input", "params", "args"):
            payload = _parse_payload(tool_info.get(key))
            if payload is not None:
                matched = _record_candidate(payload, f"tool.{key}")
                if matched is not None:
                    return matched

    content_list = tool_call.get("content")
    if isinstance(content_list, list):
        for item in content_list:
            if not isinstance(item, dict):
                continue
            if item.get("type") != "content":
                continue
            content = item.get("content")
            if not isinstance(content, dict):
                continue
            if content.get("type") != "text":
                continue
            payload = _parse_payload(content.get("text"))
            if payload is not None:
                matched = _record_candidate(payload, "content")
                if matched is not None:
                    return matched

    for key in ("title",):
        payload = _parse_payload(tool_call.get(key))
        if payload is not None:
            matched = _record_candidate(payload, key)
            if matched is not None:
                return matched

    if isinstance(tool_info, dict):
        for key in ("title",):
            payload = _parse_payload(tool_info.get(key))
            if payload is not None:
                matched = _record_candidate(payload, f"tool.{key}")
                if matched is not None:
                    return matched

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
    if isinstance(tool_call, ToolCall):
        # Check rawOutput first (direct MCP result)
        payload = _parse_payload(tool_call.raw_output)
        if payload is not None and _is_echo_back_payload(payload):
            return payload
        # Check content list
        for item in tool_call.content or []:
            if getattr(item, "type", None) != "content":
                continue
            sub_content = item.content
            if getattr(sub_content, "type", None) != "text":
                continue
            payload = _parse_payload(sub_content.text)
            if payload is not None and _is_echo_back_payload(payload):
                return payload
        return None

    # Dict-based tool call
    payload = _parse_payload(tool_call.get("rawOutput"))
    if payload is not None and _is_echo_back_payload(payload):
        return payload

    content_list = tool_call.get("content")
    if isinstance(content_list, list):
        for item in content_list:
            if not isinstance(item, dict):
                continue
            if item.get("type") != "content":
                continue
            content = item.get("content")
            if not isinstance(content, dict):
                continue
            if content.get("type") != "text":
                continue
            payload = _parse_payload(content.get("text"))
            if payload is not None and _is_echo_back_payload(payload):
                return payload

    return None


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
