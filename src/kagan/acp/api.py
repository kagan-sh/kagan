# mypy: disable-error-code="empty-body"
"""ACP remote API - outgoing client-to-agent RPC methods."""

from __future__ import annotations

from kagan import jsonrpc
from kagan.acp import protocol  # noqa: TC001 - needed at runtime for get_type_hints

API = jsonrpc.API()


@API.method()
def initialize(
    protocolVersion: int,
    clientCapabilities: protocol.ClientCapabilities,
    clientInfo: protocol.Implementation,
) -> protocol.InitializeResponse:
    """Initialize ACP connection."""
    ...


@API.method(name="session/new")
def session_new(
    cwd: str,
    mcpServers: list[protocol.McpServer] | None = None,
) -> protocol.NewSessionResponse:
    """Create a new session."""
    ...


@API.notification(name="session/cancel")
def session_cancel(sessionId: str, _meta: dict | None = None) -> None:
    """Cancel current operation."""
    ...


@API.method(name="session/prompt")
def session_prompt(
    prompt: list[protocol.ContentBlock],
    sessionId: str,
) -> protocol.SessionPromptResponse:
    """Send a prompt to the agent."""
    ...


@API.method(name="session/set_mode")
def session_set_mode(
    sessionId: str,
    modeId: str,
) -> protocol.SetSessionModeResponse:
    """Set the session mode."""
    ...
