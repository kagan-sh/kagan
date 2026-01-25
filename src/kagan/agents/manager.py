"""Agent manager for multiple processes."""

from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.agents.process import AgentProcess, AgentState

if TYPE_CHECKING:
    from pathlib import Path


class AgentManager:
    """Manages multiple agent processes."""

    def __init__(self) -> None:
        self._agents: dict[str, AgentProcess] = {}

    async def spawn(
        self, ticket_id: str, command: str, cwd: Path, env: dict[str, str] | None = None
    ) -> AgentProcess:
        """Spawn a new agent for a ticket."""
        if ticket_id in self._agents:
            raise ValueError(f"Agent already running for {ticket_id}")
        agent = AgentProcess(ticket_id)
        await agent.start(command, cwd, env)
        self._agents[ticket_id] = agent
        return agent

    def get(self, ticket_id: str) -> AgentProcess | None:
        """Get agent by ticket_id."""
        return self._agents.get(ticket_id)

    async def terminate(self, ticket_id: str) -> None:
        """Terminate a specific agent."""
        if agent := self._agents.get(ticket_id):
            await agent.terminate()
            del self._agents[ticket_id]

    async def terminate_all(self) -> None:
        """Terminate all agents."""
        for agent in list(self._agents.values()):
            await agent.terminate()
        self._agents.clear()

    def list_active(self) -> list[str]:
        """List ticket_ids with active agents."""
        return [tid for tid, a in self._agents.items() if a.state == AgentState.RUNNING]
