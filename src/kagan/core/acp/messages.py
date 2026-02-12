"""ACP message types and buffer management for agent communication."""

from __future__ import annotations

import asyncio  # noqa: TC003 (used in type hint for dataclass field)
from collections import deque
from dataclasses import dataclass
from typing import TYPE_CHECKING, NamedTuple

from textual.message import Message

from kagan.core.debug_log import log
from kagan.core.limits import MESSAGE_BUFFER, RESPONSE_BUFFER

if TYPE_CHECKING:
    from acp.schema import (
        AvailableCommand,
        PermissionOption,
        PlanEntry,
    )
    from acp.schema import (
        ToolCall as AcpToolCall,
    )
    from acp.schema import (
        ToolCallUpdate as AcpToolCallUpdate,
    )


class Mode(NamedTuple):
    """An agent mode."""

    id: str
    name: str
    description: str | None


class Model(NamedTuple):
    """An available LLM model."""

    id: str
    name: str
    description: str | None


class Answer(NamedTuple):
    """Permission dialog answer."""

    id: str


class AgentMessage(Message):
    """Base class for all agent-related messages."""

    pass


@dataclass(slots=True)
class AgentReady(AgentMessage):
    """Agent is initialized and ready for prompts."""

    pass


@dataclass(slots=True)
class AgentFail(AgentMessage):
    """Agent failed to start or encountered an error."""

    message: str
    details: str = ""


@dataclass(slots=True)
class AgentComplete(AgentMessage):
    """Agent completed its response."""

    pass


@dataclass(slots=True)
class AgentUpdate(AgentMessage):
    """Agent sent text content."""

    content_type: str
    text: str


@dataclass(slots=True)
class Thinking(AgentMessage):
    """Agent thinking/reasoning content."""

    content_type: str
    text: str


@dataclass(slots=True)
class ToolCall(AgentMessage):
    """Agent is making a tool call."""

    tool_call: AcpToolCall


@dataclass(slots=True)
class ToolCallUpdate(AgentMessage):
    """Tool call status update."""

    tool_call: AcpToolCall
    update: AcpToolCallUpdate


@dataclass(slots=True)
class Plan(AgentMessage):
    """Agent's plan entries."""

    entries: list[PlanEntry]


@dataclass(slots=True)
class RequestPermission(AgentMessage):
    """Agent needs permission for an operation."""

    options: list[PermissionOption]
    tool_call: AcpToolCall | AcpToolCallUpdate
    result_future: asyncio.Future[Answer]


@dataclass(slots=True)
class SetModes(AgentMessage):
    """Agent reported available modes."""

    current_mode: str
    modes: dict[str, Mode]


@dataclass(slots=True)
class ModeUpdate(AgentMessage):
    """Agent informed us about a mode change."""

    current_mode: str


@dataclass(slots=True)
class SetModels(AgentMessage):
    """Agent reported available models."""

    current_model: str
    models: dict[str, Model]


@dataclass(slots=True)
class ModelUpdate(AgentMessage):
    """Agent informed us about a model change."""

    current_model: str


@dataclass(slots=True)
class AvailableCommandsUpdate(AgentMessage):
    """Agent is reporting its slash commands."""

    commands: list[AvailableCommand]


# ---------------------------------------------------------------------------
# Agent buffers
# ---------------------------------------------------------------------------


class AgentBuffers:
    """Manages response and message buffers for an agent."""

    def __init__(self) -> None:
        self.response: deque[str] = deque(maxlen=RESPONSE_BUFFER)
        self.messages: deque[Message] = deque(maxlen=MESSAGE_BUFFER)

    def append_response(self, text: str) -> None:
        log.debug(f"[AGENT] {text}")
        self.response.append(text)

    def buffer_message(self, message: Message) -> None:
        self.messages.append(message)

    def get_response_text(self) -> str:
        return "".join(self.response)

    def clear_response(self) -> None:
        self.response.clear()

    def clear_messages(self) -> None:
        self.messages.clear()

    def clear_all(self) -> None:
        self.response.clear()
        self.messages.clear()

    def replay_messages_to(self, target) -> None:
        """Replay all buffered messages to a target."""
        for msg in self.messages:
            target.post_message(msg)
