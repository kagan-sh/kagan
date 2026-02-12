"""Route-aware mock agents and factory builders."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, Any

from tests.helpers.mocks.agent_base import MockAgent

if TYPE_CHECKING:
    from collections.abc import Callable

    from kagan.core.config import AgentConfig


class SmartMockAgent(MockAgent):
    """Route-based mock agent that adapts responses based on prompt keywords.

    Replaces the ad-hoc PairFlowAgent, FullFlowAgent, JourneyAgent classes.

    Usage::

        routes = {
            "propose_plan": (PLAN_RESPONSE, plan_tool_calls),
            "Code Review Specialist": (REVIEW_RESPONSE, {}),
        }
        agent = SmartMockAgent(root, config, routes=routes)

    If no route matches, the *default* response/tool_calls pair is used.
    An optional *on_default* async callback is called when the default
    route fires (e.g. to commit files in the worktree).
    """

    def __init__(
        self,
        project_root: Path,
        agent_config: AgentConfig,
        *,
        routes: dict[str, tuple[str, dict[str, Any]]] | None = None,
        default: tuple[str, dict[str, Any]] | None = None,
        on_default: Callable[..., Any] | None = None,
        read_only: bool = False,
    ) -> None:
        super().__init__(project_root, agent_config, read_only=read_only)
        self._routes = routes or {}
        self._default = default or ("Done. <complete/>", {})
        self._on_default = on_default

    async def send_prompt(self, prompt: str) -> str | None:
        for keyword, (response, tool_calls) in self._routes.items():
            if keyword in prompt:
                self.set_response(response)
                self.set_tool_calls(tool_calls)
                return await super().send_prompt(prompt)

        response, tool_calls = self._default
        if self._on_default is not None:
            import inspect

            result = self._on_default(self)
            if inspect.isawaitable(result):
                await result
        self.set_response(response)
        self.set_tool_calls(tool_calls)
        return await super().send_prompt(prompt)


class NoopMessageAgent:
    """Minimal stream target-compatible agent used by UI tests."""

    def set_message_target(self, _target: Any) -> None:
        return None


def build_smart_agent_factory(
    *,
    routes: dict[str, tuple[str, dict[str, Any]]] | None = None,
    default: tuple[str, dict[str, Any]] | None = None,
    on_default: Callable[..., Any] | None = None,
) -> Callable[..., Any]:
    """Create a reusable SmartMockAgent factory for app/test fixtures."""

    def _factory(
        project_root: Path,
        agent_config: AgentConfig,
        *,
        read_only: bool = False,
    ) -> Any:
        return SmartMockAgent(
            project_root,
            agent_config,
            read_only=read_only,
            routes=routes,
            default=default,
            on_default=on_default,
        )

    return _factory


def build_repo_routed_smart_agent_factory(
    routes_by_repo: dict[str, dict[str, tuple[str, dict[str, Any]]]],
    *,
    default: tuple[str, dict[str, Any]] | None = None,
    on_default: Callable[..., Any] | None = None,
) -> Callable[..., Any]:
    """Create SmartMockAgent factory with route maps selected by project root."""
    normalized_routes = {
        str(Path(repo_root)): routes for repo_root, routes in routes_by_repo.items()
    }

    def _factory(
        project_root: Path,
        agent_config: AgentConfig,
        *,
        read_only: bool = False,
    ) -> Any:
        return SmartMockAgent(
            project_root,
            agent_config,
            read_only=read_only,
            routes=normalized_routes.get(str(project_root), {}),
            default=default,
            on_default=on_default,
        )

    return _factory
