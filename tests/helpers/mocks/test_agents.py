from __future__ import annotations

from pathlib import Path

import pytest

from tests.helpers.mocks.agents import (
    MockAgentFactory,
    SmartMockAgent,
    build_repo_routed_smart_agent_factory,
    build_smart_agent_factory,
)
from tests.helpers.mocks.core import create_test_agent_config


def test_mock_agent_factory_when_defaults_set_applies_defaults_to_created_agent() -> None:
    factory = MockAgentFactory()
    factory.set_default_response("Configured response <complete/>")
    factory.set_default_thinking("Thinking...")

    created_agent = factory(Path("."), create_test_agent_config())

    assert created_agent.get_response_text() == "Configured response <complete/>"
    assert created_agent.get_thinking_text() == "Thinking..."
    assert factory.get_last_agent() is created_agent


@pytest.mark.asyncio
async def test_smart_agent_factory_when_prompt_matches_route_returns_routed_response() -> None:
    route_response = "Route response <complete/>"
    fallback_response = "Fallback response <complete/>"
    factory = build_smart_agent_factory(
        routes={"route-keyword": (route_response, {})},
        default=(fallback_response, {}),
    )

    routed_agent = factory(Path("."), create_test_agent_config())
    assert isinstance(routed_agent, SmartMockAgent)

    await routed_agent.send_prompt("trigger route-keyword")
    assert routed_agent.get_response_text() == route_response


@pytest.mark.asyncio
async def test_repo_routed_factory_when_repos_differ_returns_repo_specific_and_default(
    tmp_path: Path,
) -> None:
    repo_a = tmp_path / "repo-a"
    repo_b = tmp_path / "repo-b"
    repo_a.mkdir()
    repo_b.mkdir()

    factory = build_repo_routed_smart_agent_factory(
        routes_by_repo={str(repo_a): {"route-keyword": ("Repo A response <complete/>", {})}},
        default=("Fallback response <complete/>", {}),
    )

    repo_a_agent = factory(repo_a, create_test_agent_config())
    repo_b_agent = factory(repo_b, create_test_agent_config())

    await repo_a_agent.send_prompt("route-keyword")
    await repo_b_agent.send_prompt("route-keyword")

    assert repo_a_agent.get_response_text() == "Repo A response <complete/>"
    assert repo_b_agent.get_response_text() == "Fallback response <complete/>"
