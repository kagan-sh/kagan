"""Tests for extended agent installer functionality."""

from __future__ import annotations

from unittest.mock import patch

import pytest

from kagan.agents.installer import (
    AgentType,
    _check_prerequisites,
    _get_path_hint,
    check_agent_installed,
)
from kagan.builtin_agents import get_agent_status, list_available_agents
from kagan.command_utils import clear_which_cache


@pytest.fixture(autouse=True)
def _clear_cache() -> None:
    """Ensure cached_which cache is empty before every test."""
    clear_which_cache()


class TestCheckAgentInstalled:
    """Tests for check_agent_installed function."""

    @pytest.mark.parametrize("agent", list(AgentType))
    def test_returns_true_when_installed(self, agent: AgentType) -> None:
        """Should return True when agent CLI is in PATH."""
        with patch("shutil.which", return_value=f"/usr/local/bin/{agent}"):
            assert check_agent_installed(agent) is True

    @pytest.mark.parametrize("agent", list(AgentType))
    def test_returns_false_when_not_installed(self, agent: AgentType) -> None:
        """Should return False when agent CLI is not in PATH."""
        with patch("shutil.which", return_value=None):
            assert check_agent_installed(agent) is False

    def test_raises_for_invalid_agent(self) -> None:
        """Should raise ValueError for unsupported agent."""
        with pytest.raises(ValueError, match="Unsupported agent"):
            check_agent_installed("invalid_agent")


class TestCheckPrerequisites:
    """Tests for _check_prerequisites function."""

    @pytest.mark.parametrize(
        "agent",
        [AgentType.OPENCODE, AgentType.CODEX, AgentType.GEMINI, AgentType.COPILOT],
    )
    def test_npm_agents_require_npm(self, agent: AgentType) -> None:
        """NPM-based agents should require npm."""
        with patch("shutil.which", return_value=None):
            error = _check_prerequisites(agent)
            assert error is not None
            assert "npm" in error.lower()
            assert "nodejs" in error.lower() or "node.js" in error.lower()

    @pytest.mark.parametrize(
        "agent",
        [AgentType.OPENCODE, AgentType.CODEX, AgentType.GEMINI, AgentType.COPILOT],
    )
    def test_npm_agents_pass_when_npm_available(self, agent: AgentType) -> None:
        """NPM-based agents should pass when npm is available."""
        with patch("shutil.which", return_value="/usr/local/bin/npm"):
            assert _check_prerequisites(agent) is None

    def test_kimi_requires_uv(self) -> None:
        """Kimi should require uv."""
        with patch("shutil.which", return_value=None):
            error = _check_prerequisites(AgentType.KIMI)
            assert error is not None
            assert "uv" in error.lower()

    def test_kimi_passes_when_uv_available(self) -> None:
        """Kimi should pass when uv is available."""
        with patch("shutil.which", return_value="/usr/local/bin/uv"):
            assert _check_prerequisites(AgentType.KIMI) is None

    def test_claude_has_no_prerequisites(self) -> None:
        """Claude should have no prerequisites (uses curl)."""
        with patch("shutil.which", return_value=None):
            assert _check_prerequisites(AgentType.CLAUDE) is None


class TestGetPathHint:
    """Tests for _get_path_hint function."""

    @pytest.mark.parametrize(
        "agent",
        [AgentType.OPENCODE, AgentType.CODEX, AgentType.GEMINI, AgentType.COPILOT],
    )
    def test_npm_agents_mention_npm_path(self, agent: AgentType) -> None:
        """NPM agents should mention npm global bin directory."""
        hint = _get_path_hint(agent)
        assert "npm" in hint.lower()
        assert "path" in hint.lower()

    def test_kimi_mentions_local_bin(self) -> None:
        """Kimi should mention ~/.local/bin."""
        hint = _get_path_hint(AgentType.KIMI)
        assert ".local/bin" in hint

    def test_claude_mentions_shell_restart(self) -> None:
        """Claude should mention shell restart or PATH."""
        hint = _get_path_hint(AgentType.CLAUDE)
        assert "shell" in hint.lower() or "path" in hint.lower()


class TestGetAgentStatus:
    """Tests for get_agent_status function."""

    def test_returns_dict_with_all_six_agents(self) -> None:
        """Should return status for all 6 agents."""
        with patch(
            "kagan.builtin_agents._check_command_available",
            return_value=False,
        ):
            status = get_agent_status()
            expected_agents = {a.value for a in AgentType}
            assert set(status.keys()) == expected_agents

    def test_returns_correct_availability(self) -> None:
        """Should return correct True/False based on installation."""

        def mock_check(cmd: str | None) -> bool:
            if cmd is None:
                return False
            return "claude" in cmd or "opencode" in cmd

        with patch(
            "kagan.builtin_agents._check_command_available",
            side_effect=mock_check,
        ):
            status = get_agent_status()
            assert status["claude"] is True
            assert status["opencode"] is True
            assert status["codex"] is False
            assert status["gemini"] is False
            assert status["kimi"] is False
            assert status["copilot"] is False


class TestListAvailableAgents:
    """Tests for list_available_agents function."""

    def test_returns_empty_when_none_installed(self) -> None:
        """Should return empty list when no agents are installed."""
        with patch(
            "kagan.builtin_agents._check_command_available",
            return_value=False,
        ):
            available = list_available_agents()
            assert available == []

    def test_returns_installed_agents(self) -> None:
        """Should return only installed agents."""

        def mock_check(cmd: str | None) -> bool:
            if cmd is None:
                return False
            return "claude" in cmd

        with patch(
            "kagan.builtin_agents._check_command_available",
            side_effect=mock_check,
        ):
            available = list_available_agents()
            assert len(available) == 1
            assert available[0].config.short_name == "claude"

    def test_returns_multiple_installed_agents(self) -> None:
        """Should return all installed agents."""

        def mock_check(cmd: str | None) -> bool:
            if cmd is None:
                return False
            return "claude" in cmd or "opencode" in cmd or "codex" in cmd

        with patch(
            "kagan.builtin_agents._check_command_available",
            side_effect=mock_check,
        ):
            available = list_available_agents()
            short_names = {a.config.short_name for a in available}
            assert "claude" in short_names
            assert "opencode" in short_names
            assert "codex" in short_names
