"""Tests for ANSI terminal output cleaner."""

from __future__ import annotations

import pytest

from kagan.ansi import clean_terminal_output
from kagan.ansi.cleaner import Line, TerminalBuffer

pytestmark = pytest.mark.unit


class TestCleanTerminalOutput:
    """Tests for clean_terminal_output function."""

    @pytest.mark.parametrize(
        ("input_text", "expected"),
        [
            # Basic text handling
            ("", ""),
            ("hello world", "hello world"),
            ("line1\nline2\nline3", "line1\nline2\nline3"),
            # Carriage return handling
            ("hello\rworld", "world"),
            ("hello world\rhi", "hillo world"),
            ("line1\r\nline2", "line1\nline2"),
            ("progress: 0%\rprogress: 50%\rprogress: 100%", "progress: 100%"),
            # Backspace handling
            ("hello\x08X", "hellX"),
            ("hello\x08\x08\x08XYZ", "heXYZ"),
            # Trailing whitespace
            ("hello   \nworld   ", "hello\nworld"),
            # Cursor horizontal absolute
            ("hello world\x1b[1GXYZ", "XYZlo world"),
        ],
    )
    def test_text_manipulation(self, input_text: str, expected: str):
        """Test text manipulation (CR, backspace, cursor movement)."""
        assert clean_terminal_output(input_text) == expected

    @pytest.mark.parametrize(
        ("input_text", "expected"),
        [
            # ANSI color codes
            ("\x1b[31mhello\x1b[0m", "hello"),
            ("\x1b[1mbold\x1b[0m", "bold"),
            ("\x1b[31;1;4mstyle\x1b[0m", "style"),
        ],
    )
    def test_ansi_stripping(self, input_text: str, expected: str):
        """Test ANSI escape sequence stripping."""
        assert clean_terminal_output(input_text) == expected

    @pytest.mark.parametrize(
        ("input_text", "expected"),
        [
            # Clear line from cursor
            ("hello world\x1b[5G\x1b[K", "hell"),
            # Clear entire line
            ("hello\x1b[2K", ""),
            # OSC sequences
            ("\x1b]0;Window Title\x07hello", "hello"),
        ],
    )
    def test_clear_sequences(self, input_text: str, expected: str):
        """Test line clearing sequences."""
        assert clean_terminal_output(input_text) == expected

    def test_cursor_up_movement(self):
        """Cursor up movement preserves column position."""
        output = "line1\nline2\x1b[AX"
        assert clean_terminal_output(output) == "line1X\nline2"

    def test_precommit_style_progress(self):
        """Pre-commit style progress output with dots and overwrites."""
        output = "checking...\rSkipped"
        assert clean_terminal_output(output) == "Skippedg..."

    def test_real_precommit_output(self):
        """Simulate real pre-commit hook output pattern."""
        output = (
            "uv-lock...................\r"
            "uv-lock...................(no files to check)Skipped\n"
            "lint......................\r"
            "lint......................(no files to check)Skipped\n"
        )
        result = clean_terminal_output(output)
        lines = result.strip().split("\n")
        assert len(lines) == 2
        assert all("Skipped" in line for line in lines)

    def test_mixed_content(self):
        """Mixed regular text, colors, and control characters."""
        output = "\x1b[32m✓\x1b[0m Test passed\nLine 2"
        result = clean_terminal_output(output)
        assert "✓" in result and "Test passed" in result and "Line 2" in result


class TestLine:
    """Tests for the Line helper class."""

    @pytest.mark.parametrize(
        ("initial", "cursor", "write_text", "expected_content", "expected_cursor"),
        [
            ("", 0, "hello", "hello", 5),
            ("hello", 0, "XY", "XYllo", 2),
        ],
    )
    def test_write(self, initial, cursor, write_text, expected_content, expected_cursor):
        """Test Line write operations."""
        line = Line(content=initial, cursor=cursor)
        line.write(write_text)
        assert line.content == expected_content
        assert line.cursor == expected_cursor

    def test_carriage_return(self):
        """Carriage return moves cursor to 0."""
        line = Line(content="hello", cursor=5)
        line.carriage_return()
        assert line.cursor == 0
        assert line.content == "hello"


class TestTerminalBuffer:
    """Tests for the TerminalBuffer helper class."""

    def test_newline_creates_new_line(self):
        """Newline creates a new line."""
        buf = TerminalBuffer()
        buf.write("line1")
        buf.newline()
        buf.write("line2")
        assert buf.get_output() == "line1\nline2"

    def test_cursor_up(self):
        """Cursor up moves to previous line."""
        buf = TerminalBuffer()
        buf.write("line1")
        buf.newline()
        buf.write("line2")
        buf.cursor_up()
        buf.carriage_return()
        buf.write("X")
        assert buf.get_output() == "Xine1\nline2"
