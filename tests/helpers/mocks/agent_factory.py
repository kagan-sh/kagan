"""Factory for creating mock agents with configurable defaults."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from tests.helpers.mocks.agent_base import MockAgent

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.core.config import AgentConfig


class MockAgentFactory:
    """Factory for creating MockAgent instances with controllable behavior.

    Works as a generic factory -- pass ``agent_cls`` to use SmartMockAgent
    or any other MockAgent subclass.
    """

    def __init__(
        self,
        *,
        agent_cls: type[MockAgent] | None = None,
        agent_kwargs: dict[str, Any] | None = None,
    ) -> None:
        self._agent_cls = agent_cls or MockAgent
        self._agent_kwargs = agent_kwargs or {}
        self._default_response = "Done. <complete/>"
        self._default_tool_calls: dict[str, Any] = {}
        self._default_thinking = ""
        self._agents: list[MockAgent] = []

    def set_default_response(self, text: str) -> None:
        self._default_response = text

    def set_default_tool_calls(self, tool_calls: dict[str, Any]) -> None:
        self._default_tool_calls = tool_calls

    def set_default_thinking(self, text: str) -> None:
        self._default_thinking = text

    def get_last_agent(self) -> MockAgent | None:
        return self._agents[-1] if self._agents else None

    def get_all_agents(self) -> list[MockAgent]:
        return list(self._agents)

    def __call__(
        self,
        project_root: Path,
        agent_config: AgentConfig,
        *,
        read_only: bool = False,
    ) -> Any:
        agent = self._agent_cls(
            project_root,
            agent_config,
            read_only=read_only,
            **self._agent_kwargs,
        )
        agent.set_response(self._default_response)
        agent.set_tool_calls(dict(self._default_tool_calls))
        agent.set_thinking_text(self._default_thinking)
        self._agents.append(agent)
        return agent
