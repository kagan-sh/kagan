"""Factory Protocol for creating Agent instances."""

from __future__ import annotations

from typing import TYPE_CHECKING, Protocol

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.core.acp import Agent
    from kagan.core.config import AgentConfig


class AgentFactory(Protocol):
    """Protocol for creating Agent instances."""

    def __call__(
        self,
        project_root: Path,
        agent_config: AgentConfig,
        *,
        read_only: bool = False,
    ) -> Agent:
        """Create an Agent instance."""
        ...


def create_agent(
    project_root: Path,
    agent_config: AgentConfig,
    *,
    read_only: bool = False,
) -> Agent:
    """Create a production Agent instance."""
    from kagan.core.acp import Agent

    return Agent(project_root, agent_config, read_only=read_only)
