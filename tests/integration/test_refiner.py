"""Integration tests for PromptRefiner service."""

from __future__ import annotations

from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from tests.helpers.mocks import create_mock_agent

pytestmark = pytest.mark.integration


class TestPromptRefiner:
    """Test PromptRefiner with mocked agent subprocess."""

    @pytest.fixture
    def refiner_agent(self) -> MagicMock:
        """Create mock agent that returns refined text."""
        return create_mock_agent(
            response="Analyze the authentication bug in the login module, "
            "identify the root cause, and implement a fix with appropriate tests"
        )

    async def test_refine_returns_enhanced_prompt(self, refiner_agent: MagicMock, config) -> None:
        """Verify refiner returns agent response as refined prompt."""
        from kagan.agents.refiner import PromptRefiner

        with patch("kagan.agents.refiner.Agent", return_value=refiner_agent):
            agent_config = config.get_worker_agent()
            assert agent_config is not None
            refiner = PromptRefiner(Path.cwd(), agent_config)

            result = await refiner.refine("fix login bug")

            assert "authentication" in result.lower()
            refiner_agent.send_prompt.assert_called_once()
            await refiner.stop()

    async def test_refine_returns_original_on_empty_response(self, config) -> None:
        """Verify original input returned if agent response is empty."""
        from kagan.agents.refiner import PromptRefiner

        empty_agent = create_mock_agent(response="")

        with patch("kagan.agents.refiner.Agent", return_value=empty_agent):
            agent_config = config.get_worker_agent()
            assert agent_config is not None
            refiner = PromptRefiner(Path.cwd(), agent_config)

            result = await refiner.refine("fix bug")

            assert result == "fix bug"
            await refiner.stop()

    async def test_refine_returns_original_on_too_short_response(self, config) -> None:
        """Verify original input returned if agent response is suspiciously short."""
        from kagan.agents.refiner import PromptRefiner

        # Response shorter than half the input length
        short_agent = create_mock_agent(response="ok")

        with patch("kagan.agents.refiner.Agent", return_value=short_agent):
            agent_config = config.get_worker_agent()
            assert agent_config is not None
            refiner = PromptRefiner(Path.cwd(), agent_config)

            result = await refiner.refine("fix the authentication bug in login")

            # Should return original since "ok" is too short
            assert result == "fix the authentication bug in login"
            await refiner.stop()

    async def test_refiner_reuses_agent_across_calls(
        self, refiner_agent: MagicMock, config
    ) -> None:
        """Verify agent is initialized once and reused."""
        from kagan.agents.refiner import PromptRefiner

        with patch("kagan.agents.refiner.Agent", return_value=refiner_agent) as agent_cls:
            agent_config = config.get_worker_agent()
            assert agent_config is not None
            refiner = PromptRefiner(Path.cwd(), agent_config)

            await refiner.refine("first prompt")
            await refiner.refine("second prompt")

            # Agent constructor called only once
            agent_cls.assert_called_once()
            # But send_prompt called twice
            assert refiner_agent.send_prompt.call_count == 2
            await refiner.stop()

    async def test_stop_cleans_up_agent(self, refiner_agent: MagicMock, config) -> None:
        """Verify stop properly terminates agent."""
        from kagan.agents.refiner import PromptRefiner

        with patch("kagan.agents.refiner.Agent", return_value=refiner_agent):
            agent_config = config.get_worker_agent()
            assert agent_config is not None
            refiner = PromptRefiner(Path.cwd(), agent_config)

            await refiner.refine("test")
            await refiner.stop()

            refiner_agent.stop.assert_called_once()

    async def test_stop_without_refine_is_safe(self, config) -> None:
        """Verify stop is safe to call even if refine was never called."""
        from kagan.agents.refiner import PromptRefiner

        agent_config = config.get_worker_agent()
        assert agent_config is not None
        refiner = PromptRefiner(Path.cwd(), agent_config)

        # Should not raise
        await refiner.stop()

    async def test_agent_auto_approve_is_enabled(self, refiner_agent: MagicMock, config) -> None:
        """Verify refiner agent has auto_approve enabled."""
        from kagan.agents.refiner import PromptRefiner

        with patch("kagan.agents.refiner.Agent", return_value=refiner_agent):
            agent_config = config.get_worker_agent()
            assert agent_config is not None
            refiner = PromptRefiner(Path.cwd(), agent_config)

            await refiner.refine("test prompt")

            refiner_agent.set_auto_approve.assert_called_once_with(True)
            await refiner.stop()


class TestRefinementRules:
    """Test refinement prompt construction."""

    def test_build_refinement_prompt_includes_user_input(self) -> None:
        """Verify user input is embedded in the prompt."""
        from kagan.agents.refinement_rules import build_refinement_prompt

        result = build_refinement_prompt("fix login bug")
        assert "fix login bug" in result

    def test_build_refinement_prompt_has_context_section(self) -> None:
        """Verify prompt includes Kagan-specific context."""
        from kagan.agents.refinement_rules import build_refinement_prompt

        result = build_refinement_prompt("add feature")
        # Should mention ticket creation context
        assert "ticket" in result.lower()
        assert "planning" in result.lower()

    def test_build_refinement_prompt_empty_input(self) -> None:
        """Verify empty input doesn't break prompt construction."""
        from kagan.agents.refinement_rules import build_refinement_prompt

        result = build_refinement_prompt("")
        assert isinstance(result, str)
        assert len(result) > 0
