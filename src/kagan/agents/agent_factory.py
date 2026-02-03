"""Factory Protocol for creating Agent instances.

Enables dependency injection for testing without modifying production behavior.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Protocol

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.acp.agent import Agent
    from kagan.config import AgentConfig


class AgentFactory(Protocol):
    """Protocol for creating Agent instances.

    Production code uses `create_agent` (default implementation).
    Tests inject custom factories to return mocks.
    """

    def __call__(
        self,
        project_root: Path,
        agent_config: AgentConfig,
        *,
        read_only: bool = False,
    ) -> Agent:
        """Create an Agent instance.

        Args:
            project_root: Project root directory for agent execution
            agent_config: Configuration for the agent
            read_only: If True, agent cannot modify files (review/planner mode)

        Returns:
            Agent instance ready for start()
        """
        ...


def create_agent(
    project_root: Path,
    agent_config: AgentConfig,
    *,
    read_only: bool = False,
) -> Agent:
    """Default agent factory - returns real Agent instance.

    This is the production implementation used by all components.
    Tests can replace this with a mock factory via dependency injection.

    Args:
        project_root: Project root directory for agent execution
        agent_config: Configuration for the agent
        read_only: If True, agent cannot modify files

    Returns:
        Real Agent instance
    """
    from kagan.acp.agent import Agent

    return Agent(project_root, agent_config, read_only=read_only)
