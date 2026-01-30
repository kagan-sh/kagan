"""ACP message types for UI dispatch."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, NamedTuple

from textual.message import Message

if TYPE_CHECKING:
    import asyncio

    from kagan.acp import protocol


class Mode(NamedTuple):
    """An agent mode."""

    id: str
    name: str
    description: str | None


class Answer(NamedTuple):
    """Permission dialog answer."""

    id: str


class AgentMessage(Message):
    """Base class for all agent-related messages."""

    pass


@dataclass
class AgentReady(AgentMessage):
    """Agent is initialized and ready for prompts."""

    pass


@dataclass
class AgentFail(AgentMessage):
    """Agent failed to start or encountered an error."""

    message: str
    details: str = ""


@dataclass
class AgentUpdate(AgentMessage):
    """Agent sent text content."""

    type: str
    text: str


@dataclass
class Thinking(AgentMessage):
    """Agent thinking/reasoning content."""

    type: str
    text: str


@dataclass
class ToolCall(AgentMessage):
    """Agent is making a tool call."""

    tool_call: protocol.ToolCall


@dataclass
class ToolCallUpdate(AgentMessage):
    """Tool call status update."""

    tool_call: protocol.ToolCall
    update: protocol.ToolCallUpdate


@dataclass
class Plan(AgentMessage):
    """Agent's plan entries."""

    entries: list[protocol.PlanEntry]


@dataclass
class RequestPermission(AgentMessage):
    """Agent needs permission for an operation."""

    options: list[protocol.PermissionOption]
    tool_call: protocol.ToolCall | protocol.ToolCallUpdatePermissionRequest
    result_future: asyncio.Future[Answer]


@dataclass
class SetModes(AgentMessage):
    """Agent reported available modes."""

    current_mode: str
    modes: dict[str, Mode]


@dataclass
class ModeUpdate(AgentMessage):
    """Agent informed us about a mode change."""

    current_mode: str


@dataclass
class AvailableCommandsUpdate(AgentMessage):
    """Agent is reporting its slash commands."""

    commands: list[protocol.AvailableCommand]
