"""Tests for CLI tools module (kagan tools enhance)."""

from __future__ import annotations

from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock

import pytest
from click.testing import CliRunner

from kagan.cli.tools import TOOL_CHOICES, _get_default_tool, enhance, tools

if TYPE_CHECKING:
    from pytest_mock import MockerFixture

pytestmark = pytest.mark.integration


def _setup_mock_refiner(mocker: MockerFixture, return_value: str = "Enhanced") -> MagicMock:
    """Setup mock refiner with common configuration."""
    mock_refiner = MagicMock()
    mock_refiner.refine = AsyncMock(return_value=return_value)
    mock_refiner.stop = AsyncMock()
    mocker.patch("kagan.agents.refiner.PromptRefiner", return_value=mock_refiner)

    mock_agent = MagicMock()
    mock_agent.config = MagicMock()
    mocker.patch("kagan.data.builtin_agents.get_builtin_agent", return_value=mock_agent)

    return mock_refiner


def _setup_mock_availability(mocker: MockerFixture, availability: list[tuple[str, bool]]) -> None:
    """Setup mock agent availability."""
    mocks = []
    for name, available in availability:
        m = MagicMock()
        m.is_available = available
        m.agent.config.short_name = name
        mocks.append(m)
    mocker.patch("kagan.data.builtin_agents.get_all_agent_availability", return_value=mocks)


class TestToolsGroup:
    """Tests for the tools CLI group."""

    def test_tools_group_help_and_subcommands(self) -> None:
        """Tools group shows help with enhance subcommand."""
        runner = CliRunner()
        result = runner.invoke(tools, ["--help"])

        assert result.exit_code == 0
        assert "Stateless developer utilities" in result.output
        assert "enhance" in result.output


class TestEnhanceCommand:
    """Tests for the enhance command."""

    def test_enhance_help(self) -> None:
        """Test enhance command shows help."""
        runner = CliRunner()
        result = runner.invoke(enhance, ["--help"])

        assert result.exit_code == 0
        assert "Enhance a prompt" in result.output
        assert "-t" in result.output or "--tool" in result.output

    def test_enhance_requires_prompt(self) -> None:
        """Test enhance command requires a prompt argument."""
        runner = CliRunner()
        result = runner.invoke(enhance, [])

        assert result.exit_code != 0
        assert "Missing argument" in result.output or "Error" in result.output

    def test_enhance_valid_tool_choices(self) -> None:
        """Test that valid tool choices are documented."""
        assert "claude" in TOOL_CHOICES
        assert "opencode" in TOOL_CHOICES

    def test_enhance_rejects_invalid_tool(self) -> None:
        """Test enhance command rejects invalid tool choice."""
        runner = CliRunner()
        result = runner.invoke(enhance, ["test prompt", "-t", "invalid_tool"])

        assert result.exit_code != 0
        assert "Invalid value" in result.output or "invalid_tool" in result.output

    def test_enhance_with_explicit_tool(self, mocker: MockerFixture) -> None:
        """Test enhance command with explicit tool selection."""
        mock_refiner = _setup_mock_refiner(mocker, "Enhanced: test prompt")

        runner = CliRunner(mix_stderr=False)
        result = runner.invoke(enhance, ["test prompt", "-t", "claude"])

        assert result.exit_code == 0
        assert "Enhanced: test prompt" in result.output
        mock_refiner.refine.assert_called_once_with("test prompt")
        mock_refiner.stop.assert_called_once()

    def test_enhance_auto_detects_tool(self, mocker: MockerFixture) -> None:
        """Test enhance command auto-detects available tool."""
        _setup_mock_refiner(mocker, "Auto-enhanced prompt")
        _setup_mock_availability(mocker, [("claude", True)])

        runner = CliRunner(mix_stderr=False)
        result = runner.invoke(enhance, ["my prompt"])

        assert result.exit_code == 0
        assert "Auto-enhanced prompt" in result.output

    def test_enhance_unknown_tool_error(self, mocker: MockerFixture) -> None:
        """Test enhance command shows error for unknown tool."""
        mocker.patch("kagan.data.builtin_agents.get_builtin_agent", return_value=None)

        runner = CliRunner(mix_stderr=False)
        result = runner.invoke(enhance, ["test prompt", "-t", "claude"])

        assert result.exit_code != 0
        assert (
            "Unknown tool" in result.output
            or "Unknown tool" in result.stderr
            or "Error" in result.output
        )

    def test_enhance_refiner_exception_returns_original(self, mocker: MockerFixture) -> None:
        """Test enhance returns original prompt when refiner fails."""
        mock_refiner = MagicMock()
        mock_refiner.refine = AsyncMock(side_effect=RuntimeError("Agent timeout"))
        mock_refiner.stop = AsyncMock()
        mocker.patch("kagan.agents.refiner.PromptRefiner", return_value=mock_refiner)

        mock_agent = MagicMock()
        mock_agent.config = MagicMock()
        mocker.patch("kagan.data.builtin_agents.get_builtin_agent", return_value=mock_agent)

        runner = CliRunner(mix_stderr=False)
        result = runner.invoke(enhance, ["original prompt", "-t", "claude"])

        assert result.exit_code == 0
        assert "original prompt" in result.output
        assert "failed" in result.stderr.lower()

    @pytest.mark.parametrize(
        ("prompt", "expected_call"),
        [
            ("prompt with émojis 🚀", "prompt with émojis 🚀"),
            ("line1\nline2\nline3", "line1\nline2\nline3"),
            ('prompt with "quotes"', 'prompt with "quotes"'),
            ("a" * 10000, "a" * 10000),
            ("", ""),
        ],
        ids=["unicode", "multiline", "quotes", "very_long", "empty"],
    )
    def test_enhance_with_various_prompts(
        self, mocker: MockerFixture, prompt: str, expected_call: str
    ) -> None:
        """Enhance handles various prompt formats."""
        mock_refiner = _setup_mock_refiner(mocker, "Enhanced")

        runner = CliRunner(mix_stderr=False)
        result = runner.invoke(enhance, [prompt, "-t", "claude"])

        assert result.exit_code == 0
        mock_refiner.refine.assert_called_once_with(expected_call)

    @pytest.mark.parametrize(
        ("tool", "expected_output"),
        [
            ("claude", "Enhanced"),
            ("CLAUDE", "Enhanced"),
            ("opencode", "OpenCode enhanced"),
        ],
        ids=["claude_lower", "claude_upper", "opencode"],
    )
    def test_enhance_with_tool_variations(
        self, mocker: MockerFixture, tool: str, expected_output: str
    ) -> None:
        """Enhance works with different tool selections."""
        _setup_mock_refiner(mocker, expected_output)

        runner = CliRunner(mix_stderr=False)
        result = runner.invoke(enhance, ["test", "-t", tool])

        assert result.exit_code == 0
        if expected_output != "Enhanced":
            assert expected_output in result.output


class TestGetDefaultTool:
    """Tests for _get_default_tool helper function."""

    @pytest.mark.parametrize(
        ("availability", "expected"),
        [
            ([("opencode", True)], "opencode"),
            ([("claude", False), ("opencode", True)], "opencode"),
            ([], "claude"),
        ],
        ids=["first_available", "skips_unavailable", "defaults_to_claude"],
    )
    def test_get_default_tool_scenarios(
        self, mocker: MockerFixture, availability: list[tuple[str, bool]], expected: str
    ) -> None:
        """_get_default_tool returns first available or defaults to claude."""
        _setup_mock_availability(mocker, availability)
        assert _get_default_tool() == expected


class TestMainEntryIntegration:
    """Tests for integration with main CLI entry point."""

    def test_tools_accessible_via_main_cli(self) -> None:
        """Tools and enhance are accessible via main CLI."""
        from kagan.__main__ import cli

        runner = CliRunner()

        # Tools group accessible
        result = runner.invoke(cli, ["tools", "--help"])
        assert result.exit_code == 0
        assert "enhance" in result.output

        # Enhance accessible
        result = runner.invoke(cli, ["tools", "enhance", "--help"])
        assert result.exit_code == 0
        assert "Enhance a prompt" in result.output
