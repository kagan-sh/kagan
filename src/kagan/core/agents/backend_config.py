"""Discriminated union types for agent backend configuration."""

from __future__ import annotations

from typing import Annotated, Literal

from pydantic import BaseModel, Field


class ClaudeAgentConfig(BaseModel):
    """Configuration for the Claude Code agent backend."""

    type: Literal["claude"] = "claude"
    model: str = "sonnet"
    allowed_tools: list[str] = Field(default_factory=list)


class OpenCodeAgentConfig(BaseModel):
    """Configuration for the OpenCode agent backend."""

    type: Literal["opencode"] = "opencode"
    model: str = "sonnet"


class CopilotAgentConfig(BaseModel):
    """Configuration for the GitHub Copilot agent backend."""

    type: Literal["copilot"] = "copilot"
    model: str = "claude-sonnet-4"


class GeminiAgentConfig(BaseModel):
    """Configuration for the Gemini CLI agent backend."""

    type: Literal["gemini"] = "gemini"
    model: str = "gemini-2.5-pro"


class KimiAgentConfig(BaseModel):
    """Configuration for the Kimi CLI agent backend."""

    type: Literal["kimi"] = "kimi"
    model: str = "kimi-k2"


class CodexAgentConfig(BaseModel):
    """Configuration for the Codex agent backend."""

    type: Literal["codex"] = "codex"
    model: str = "o3"


AgentBackendConfig = Annotated[
    ClaudeAgentConfig
    | OpenCodeAgentConfig
    | CopilotAgentConfig
    | GeminiAgentConfig
    | KimiAgentConfig
    | CodexAgentConfig,
    Field(discriminator="type"),
]

BACKEND_CONFIG_DEFAULTS: dict[str, type[BaseModel]] = {
    "claude": ClaudeAgentConfig,
    "opencode": OpenCodeAgentConfig,
    "copilot": CopilotAgentConfig,
    "gemini": GeminiAgentConfig,
    "kimi": KimiAgentConfig,
    "codex": CodexAgentConfig,
}

_BACKEND_CONFIG_TYPES = (
    ClaudeAgentConfig,
    OpenCodeAgentConfig,
    CopilotAgentConfig,
    GeminiAgentConfig,
    KimiAgentConfig,
    CodexAgentConfig,
)


def get_backend_config(agent_type: str) -> AgentBackendConfig:
    """Get the default backend config for a given agent type.

    Args:
        agent_type: The agent type identifier (e.g., 'claude', 'opencode').

    Returns:
        The default AgentBackendConfig for the given type.

    Raises:
        ValueError: If the agent type is unknown.
    """
    config_cls = BACKEND_CONFIG_DEFAULTS.get(agent_type)
    if config_cls is None:
        supported = ", ".join(sorted(BACKEND_CONFIG_DEFAULTS))
        raise ValueError(f"Unknown agent type: {agent_type!r}. Supported: {supported}")
    config = config_cls()
    if isinstance(config, _BACKEND_CONFIG_TYPES):
        return config
    msg = f"Unexpected backend config type: {type(config).__name__}"
    raise TypeError(msg)


__all__ = [
    "BACKEND_CONFIG_DEFAULTS",
    "AgentBackendConfig",
    "ClaudeAgentConfig",
    "CodexAgentConfig",
    "CopilotAgentConfig",
    "GeminiAgentConfig",
    "KimiAgentConfig",
    "OpenCodeAgentConfig",
    "get_backend_config",
]
