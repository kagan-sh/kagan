"""Buffer management for agent communication."""

from __future__ import annotations

from collections import deque
from typing import TYPE_CHECKING

from kagan.debug_log import log
from kagan.limits import MESSAGE_BUFFER, RESPONSE_BUFFER

if TYPE_CHECKING:
    from textual.message import Message


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
        """Replay all buffered messages to a target.

        The buffer is kept intact so future reconnects can rebuild the full view.
        """
        for msg in self.messages:
            target.post_message(msg)
