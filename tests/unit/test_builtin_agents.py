"""Unit tests for builtin_agents agent availability detection."""

from __future__ import annotations

import pytest

from kagan.data.builtin_agents import (
    AGENT_PRIORITY,
    BUILTIN_AGENTS,
    AgentAvailability,
    _check_command_available,
    any_agent_available,
    check_agent_availability,
    get_all_agent_availability,
    get_first_available_agent,
)

pytestmark = pytest.mark.unit


class TestCheckCommandAvailable:
    """Test _check_command_available helper function."""

    def test_simple_command_available(self, mocker):
        """Simple command returns True when in PATH."""
        mocker.patch("kagan.data.builtin_agents.shutil.which", return_value="/usr/bin/claude")

        assert _check_command_available("claude") is True

    def test_simple_command_not_available(self, mocker):
        """Simple command returns False when not in PATH."""
        mocker.patch("kagan.data.builtin_agents.shutil.which", return_value=None)

        assert _check_command_available("claude") is False

    def test_none_command_returns_false(self, mocker):
        """None command returns False."""
        assert _check_command_available(None) is False

    def test_empty_command_returns_false(self, mocker):
        """Empty string command returns False."""
        mocker.patch("kagan.data.builtin_agents.shutil.which", return_value=None)

        assert _check_command_available("") is False

    def test_npx_command_with_only_npx_available(self, mocker):
        """npx command returns False when only npx is available (binary not installed).

        This is the key test: just having npx is NOT sufficient.
        The package binary must be globally installed for availability check.
        """

        def which_side_effect(cmd):
            if cmd == "npx":
                return "/usr/bin/npx"
            return None  # claude-code-acp not installed

        mocker.patch("kagan.data.builtin_agents.shutil.which", side_effect=which_side_effect)

        # Should be False because only npx exists, not the actual binary
        assert _check_command_available("npx claude-code-acp") is False

    def test_npx_command_with_global_binary(self, mocker):
        """npx command returns True when package binary is globally installed."""

        def which_side_effect(cmd):
            if cmd == "claude-code-acp":
                return "/usr/local/bin/claude-code-acp"
            return None

        mocker.patch("kagan.data.builtin_agents.shutil.which", side_effect=which_side_effect)

        assert _check_command_available("npx claude-code-acp") is True

    def test_npx_command_neither_available(self, mocker):
        """npx command returns False when neither npx nor binary is available."""
        mocker.patch("kagan.data.builtin_agents.shutil.which", return_value=None)

        assert _check_command_available("npx claude-code-acp") is False

    def test_command_with_args(self, mocker):
        """Command with arguments checks first part only."""

        def which_side_effect(cmd):
            if cmd == "opencode":
                return "/usr/bin/opencode"
            return None

        mocker.patch("kagan.data.builtin_agents.shutil.which", side_effect=which_side_effect)

        assert _check_command_available("opencode acp") is True

    def test_malformed_command_string(self, mocker):
        """Malformed command string is handled gracefully."""
        # shlex.split will fail on unbalanced quotes, but function handles it
        mocker.patch("kagan.data.builtin_agents.shutil.which", return_value=None)

        # This should not raise, but return False
        result = _check_command_available("echo 'unclosed")
        assert result is False


class TestCheckAgentAvailability:
    """Test check_agent_availability function."""

    def test_agent_fully_available(self, mocker):
        """Agent with both commands available is fully available.

        Note: For Claude, ACP requires the claude-code-acp binary to be globally
        installed. Just having npx is not sufficient for availability check.
        """

        def which_side_effect(cmd):
            # Both claude CLI and claude-code-acp binary must be available
            if cmd in ("claude", "claude-code-acp"):
                return f"/usr/bin/{cmd}"
            return None

        mocker.patch("kagan.data.builtin_agents.shutil.which", side_effect=which_side_effect)

        agent = BUILTIN_AGENTS["claude"]
        result = check_agent_availability(agent)

        assert result.is_available is True
        assert result.interactive_available is True
        assert result.acp_available is True
        assert result.install_hint == agent.install_command
        assert result.docs_url == agent.docs_url

    def test_agent_only_interactive_available(self, mocker):
        """Agent with only interactive command is still available."""

        def which_side_effect(cmd):
            if cmd == "claude":
                return "/usr/bin/claude"
            return None

        mocker.patch("kagan.data.builtin_agents.shutil.which", side_effect=which_side_effect)

        agent = BUILTIN_AGENTS["claude"]
        result = check_agent_availability(agent)

        assert result.is_available is True
        assert result.interactive_available is True
        assert result.acp_available is False

    def test_agent_not_available(self, mocker):
        """Agent with no commands available is not available."""
        mocker.patch("kagan.data.builtin_agents.shutil.which", return_value=None)

        agent = BUILTIN_AGENTS["claude"]
        result = check_agent_availability(agent)

        assert result.is_available is False
        assert result.interactive_available is False
        assert result.acp_available is False


class TestGetAllAgentAvailability:
    """Test get_all_agent_availability function."""

    def test_returns_agents_in_priority_order(self, mocker):
        """Returns agents in AGENT_PRIORITY order."""
        mocker.patch("kagan.data.builtin_agents.shutil.which", return_value=None)

        result = get_all_agent_availability()

        assert len(result) == len(AGENT_PRIORITY)
        for i, key in enumerate(AGENT_PRIORITY):
            assert result[i].agent.config.short_name == key

    def test_returns_availability_for_all_agents(self, mocker):
        """Returns AgentAvailability for all builtin agents."""
        mocker.patch("kagan.data.builtin_agents.shutil.which", return_value=None)

        result = get_all_agent_availability()

        assert all(isinstance(a, AgentAvailability) for a in result)


class TestGetFirstAvailableAgent:
    """Test get_first_available_agent function."""

    def test_returns_claude_when_available(self, mocker):
        """Returns claude when it's available (priority first)."""

        def which_side_effect(cmd):
            if cmd in ("claude", "opencode"):
                return f"/usr/bin/{cmd}"
            return None

        mocker.patch("kagan.data.builtin_agents.shutil.which", side_effect=which_side_effect)

        result = get_first_available_agent()

        assert result is not None
        assert result.config.short_name == "claude"

    def test_returns_opencode_when_claude_not_available(self, mocker):
        """Returns opencode when claude is not available."""

        def which_side_effect(cmd):
            if cmd == "opencode":
                return "/usr/bin/opencode"
            return None

        mocker.patch("kagan.data.builtin_agents.shutil.which", side_effect=which_side_effect)

        result = get_first_available_agent()

        assert result is not None
        assert result.config.short_name == "opencode"

    def test_returns_none_when_no_agents_available(self, mocker):
        """Returns None when no agents are available."""
        mocker.patch("kagan.data.builtin_agents.shutil.which", return_value=None)

        result = get_first_available_agent()

        assert result is None


class TestAnyAgentAvailable:
    """Test any_agent_available function."""

    def test_returns_true_when_any_available(self, mocker):
        """Returns True when at least one agent is available."""

        def which_side_effect(cmd):
            if cmd == "opencode":
                return "/usr/bin/opencode"
            return None

        mocker.patch("kagan.data.builtin_agents.shutil.which", side_effect=which_side_effect)

        assert any_agent_available() is True

    def test_returns_false_when_none_available(self, mocker):
        """Returns False when no agents are available."""
        mocker.patch("kagan.data.builtin_agents.shutil.which", return_value=None)

        assert any_agent_available() is False


class TestAgentAvailabilityDataclass:
    """Test AgentAvailability dataclass properties."""

    def test_is_available_both_true(self):
        """is_available returns True when both modes available."""
        agent = BUILTIN_AGENTS["claude"]
        avail = AgentAvailability(
            agent=agent,
            interactive_available=True,
            acp_available=True,
        )
        assert avail.is_available is True

    def test_is_available_only_interactive(self):
        """is_available returns True when only interactive available."""
        agent = BUILTIN_AGENTS["claude"]
        avail = AgentAvailability(
            agent=agent,
            interactive_available=True,
            acp_available=False,
        )
        assert avail.is_available is True

    def test_is_available_only_acp(self):
        """is_available returns True when only ACP available."""
        agent = BUILTIN_AGENTS["claude"]
        avail = AgentAvailability(
            agent=agent,
            interactive_available=False,
            acp_available=True,
        )
        assert avail.is_available is True

    def test_is_available_neither(self):
        """is_available returns False when neither available."""
        agent = BUILTIN_AGENTS["claude"]
        avail = AgentAvailability(
            agent=agent,
            interactive_available=False,
            acp_available=False,
        )
        assert avail.is_available is False

    def test_install_hint_property(self):
        """install_hint returns agent's install command."""
        agent = BUILTIN_AGENTS["claude"]
        avail = AgentAvailability(agent=agent)
        assert avail.install_hint == agent.install_command

    def test_docs_url_property(self):
        """docs_url returns agent's docs URL."""
        agent = BUILTIN_AGENTS["claude"]
        avail = AgentAvailability(agent=agent)
        assert avail.docs_url == agent.docs_url
