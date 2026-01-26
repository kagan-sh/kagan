"""Agent manager for multiple ACP agent processes."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual import log

from kagan.acp.agent import Agent

if TYPE_CHECKING:
    from pathlib import Path

    from textual.message_pump import MessagePump

    from kagan.config import AgentConfig


class AgentManager:
    """Manages multiple ACP agent processes."""

    def __init__(self) -> None:
        self._agents: dict[str, Agent] = {}

    async def spawn(
        self,
        ticket_id: str,
        agent_config: AgentConfig,
        cwd: Path,
        message_target: MessagePump | None = None,
    ) -> Agent:
        """Spawn a new agent for a ticket.

        Args:
            ticket_id: Unique ticket identifier.
            agent_config: Agent configuration with run command.
            cwd: Working directory for the agent.
            message_target: Textual widget to receive agent messages.

        Returns:
            The spawned Agent instance.

        Raises:
            ValueError: If an agent is already running for this ticket.
        """
        log.info(f"[AgentManager.spawn] ticket_id={ticket_id}, cwd={cwd}")
        log.info(f"[AgentManager.spawn] agent_config.name={agent_config.name}")
        log.info(f"[AgentManager.spawn] agent_config.run_command={agent_config.run_command}")

        if ticket_id in self._agents:
            log.warning(f"[AgentManager.spawn] Agent already running for {ticket_id}")
            raise ValueError(f"Agent already running for {ticket_id}")

        log.info("[AgentManager.spawn] Creating Agent instance...")
        agent = Agent(cwd, agent_config)
        log.info("[AgentManager.spawn] Calling agent.start()...")
        agent.start(message_target)
        self._agents[ticket_id] = agent
        log.info(f"[AgentManager.spawn] Agent started for {ticket_id}")
        return agent

    def get(self, ticket_id: str) -> Agent | None:
        """Get agent by ticket_id."""
        return self._agents.get(ticket_id)

    async def terminate(self, ticket_id: str) -> None:
        """Terminate a specific agent."""
        if agent := self._agents.pop(ticket_id, None):
            await agent.stop()

    async def terminate_all(self) -> None:
        """Terminate all agents."""
        for agent in list(self._agents.values()):
            await agent.stop()
        self._agents.clear()

    def list_active(self) -> list[str]:
        """List ticket_ids with active agents."""
        return list(self._agents.keys())

    def is_running(self, ticket_id: str) -> bool:
        """Check if an agent is running for a ticket."""
        return ticket_id in self._agents
