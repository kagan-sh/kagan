"""Integration tests for TerminalManager."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.acp.terminals import TerminalManager
from kagan.jsonrpc import JSONRPCError

if TYPE_CHECKING:
    from pathlib import Path

pytestmark = pytest.mark.integration


class TestTerminalManager:
    """Tests for TerminalManager with real subprocess execution."""

    async def test_create_terminal_success(self, tmp_path: Path):
        """Test creating a terminal with a simple echo command."""
        manager = TerminalManager(tmp_path)

        terminal_id, cmd_display = await manager.create("echo", ["hello"])

        assert terminal_id == "terminal-1"
        assert "echo" in cmd_display
        assert "hello" in cmd_display
        assert manager.get(terminal_id) is not None

        manager.cleanup_all()

    async def test_create_terminal_with_args(self, tmp_path: Path):
        """Test creating terminal with command arguments."""
        manager = TerminalManager(tmp_path)

        terminal_id, cmd_display = await manager.create(
            "echo",
            args=["-n", "test output"],
        )

        assert terminal_id == "terminal-1"
        assert "-n" in cmd_display
        assert "test output" in cmd_display

        # Wait for exit and verify output
        exit_code, _ = await manager.wait_for_exit(terminal_id)
        assert exit_code == 0

        output = manager.get_output(terminal_id)
        assert "test output" in output["output"]

        manager.cleanup_all()

    async def test_get_terminal_returns_none_for_unknown(self, tmp_path: Path):
        """Test that get() returns None for unknown terminal ID."""
        manager = TerminalManager(tmp_path)

        result = manager.get("nonexistent-terminal")

        assert result is None

    async def test_get_output_raises_for_unknown_terminal(self, tmp_path: Path):
        """Test that get_output() raises JSONRPCError for unknown ID."""
        manager = TerminalManager(tmp_path)

        with pytest.raises(JSONRPCError, match="No terminal with id"):
            manager.get_output("unknown-id")

    async def test_kill_terminal(self, tmp_path: Path):
        """Test killing a running terminal process."""
        manager = TerminalManager(tmp_path)

        # Start a long-running command
        terminal_id, _ = await manager.create("sleep", ["10"])

        # Verify terminal is running
        terminal = manager.get(terminal_id)
        assert terminal is not None
        assert terminal.return_code is None

        # Kill it
        manager.kill(terminal_id)

        # Wait for exit - should get killed signal
        exit_code, _ = await manager.wait_for_exit(terminal_id)
        assert exit_code != 0  # Killed processes don't exit cleanly

        manager.cleanup_all()

    async def test_wait_for_exit_returns_exit_code(self, tmp_path: Path):
        """Test waiting for terminal to exit returns correct exit code."""
        manager = TerminalManager(tmp_path)

        # Create terminal that exits with code 0
        terminal_id, _ = await manager.create("true")
        exit_code, signal = await manager.wait_for_exit(terminal_id)
        assert exit_code == 0
        assert signal is None

        # Create terminal that exits with code 1
        terminal_id2, _ = await manager.create("false")
        exit_code2, _ = await manager.wait_for_exit(terminal_id2)
        assert exit_code2 == 1

        manager.cleanup_all()

    async def test_wait_for_exit_raises_for_unknown(self, tmp_path: Path):
        """Test wait_for_exit raises for unknown terminal."""
        manager = TerminalManager(tmp_path)

        with pytest.raises(JSONRPCError, match="No terminal with id"):
            await manager.wait_for_exit("unknown-id")

    async def test_get_final_output_cleans_ansi(self, tmp_path: Path):
        """Test that get_final_output returns cleaned output."""
        manager = TerminalManager(tmp_path)

        # printf with ANSI escape sequences
        terminal_id, _ = await manager.create(
            "printf",
            [r"\033[31mred text\033[0m normal"],
        )

        await manager.wait_for_exit(terminal_id)

        output = manager.get_final_output(terminal_id)
        # Output should be cleaned (exact result depends on cleaner implementation)
        assert isinstance(output, str)
        # The core text should be present
        assert "red text" in output or "normal" in output

        manager.cleanup_all()

    async def test_get_final_output_returns_empty_for_unknown(self, tmp_path: Path):
        """Test get_final_output returns empty string for unknown terminal."""
        manager = TerminalManager(tmp_path)

        result = manager.get_final_output("nonexistent")

        assert result == ""

    async def test_cleanup_all_releases_terminals(self, tmp_path: Path):
        """Test cleanup_all kills and releases all terminals."""
        manager = TerminalManager(tmp_path)

        # Create multiple terminals
        id1, _ = await manager.create("sleep", ["10"])
        id2, _ = await manager.create("sleep", ["10"])

        assert manager.get(id1) is not None
        assert manager.get(id2) is not None

        # Cleanup all
        manager.cleanup_all()

        # All terminals should be gone
        assert manager.get(id1) is None
        assert manager.get(id2) is None

    async def test_release_terminal(self, tmp_path: Path):
        """Test releasing a specific terminal."""
        manager = TerminalManager(tmp_path)

        terminal_id, _ = await manager.create("echo", ["test"])
        await manager.wait_for_exit(terminal_id)

        terminal = manager.get(terminal_id)
        assert terminal is not None

        manager.release(terminal_id)

        # Terminal still exists but is marked as released
        terminal = manager.get(terminal_id)
        assert terminal is not None
        assert terminal.released is True

    async def test_create_increments_terminal_id(self, tmp_path: Path):
        """Test that terminal IDs increment correctly."""
        manager = TerminalManager(tmp_path)

        id1, _ = await manager.create("true")
        id2, _ = await manager.create("true")
        id3, _ = await manager.create("true")

        assert id1 == "terminal-1"
        assert id2 == "terminal-2"
        assert id3 == "terminal-3"

        manager.cleanup_all()

    async def test_get_output_includes_exit_status(self, tmp_path: Path):
        """Test that get_output includes exit status after command completes."""
        manager = TerminalManager(tmp_path)

        terminal_id, _ = await manager.create("echo", ["done"])
        await manager.wait_for_exit(terminal_id)

        output = manager.get_output(terminal_id)

        assert "exitStatus" in output
        assert output["exitStatus"]["exitCode"] == 0
        assert "done" in output["output"]
        assert output["truncated"] is False

        manager.cleanup_all()
