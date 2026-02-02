"""Property-based tests for ANSI terminal output cleaner using Hypothesis."""

from __future__ import annotations

import pytest
from hypothesis import given

from kagan.ansi import clean_terminal_output
from kagan.ansi.cleaner import strip_ansi
from tests.strategies import plain_text, text_with_ansi

pytestmark = pytest.mark.unit


class TestStripAnsiProperties:
    """Property-based tests for strip_ansi function."""

    @given(plain_text)
    def test_plain_text_unchanged(self, text: str) -> None:
        """Plain text without ANSI codes passes through unchanged."""
        result = strip_ansi(text)
        assert result == text

    @given(text_with_ansi())
    def test_no_escape_in_output(self, text: str) -> None:
        """Output never contains escape character after stripping."""
        result = strip_ansi(text)
        assert "\x1b" not in result

    @given(text_with_ansi())
    def test_idempotence(self, text: str) -> None:
        """Stripping twice equals stripping once (idempotent)."""
        once = strip_ansi(text)
        twice = strip_ansi(once)
        assert once == twice

    @given(plain_text, plain_text)
    def test_concatenation_preserved(self, a: str, b: str) -> None:
        """Stripping preserves concatenation of plain text parts."""
        # If we have two plain texts, strip(a + b) should preserve both
        combined = a + b
        result = strip_ansi(combined)
        assert result == combined

    @given(text_with_ansi())
    def test_length_never_increases(self, text: str) -> None:
        """Output length is always <= input length."""
        result = strip_ansi(text)
        assert len(result) <= len(text)


class TestCleanTerminalOutputProperties:
    """Property-based tests for clean_terminal_output function."""

    @given(text_with_ansi())
    def test_alias_equivalence(self, text: str) -> None:
        """clean_terminal_output is equivalent to strip_ansi."""
        assert clean_terminal_output(text) == strip_ansi(text)

    @given(plain_text)
    def test_unicode_preserved(self, text: str) -> None:
        """Unicode characters are preserved in output."""
        result = clean_terminal_output(text)
        # For plain text, result should equal input
        assert result == text

    @given(text_with_ansi())
    def test_result_is_string(self, text: str) -> None:
        """Result is always a string."""
        result = clean_terminal_output(text)
        assert isinstance(result, str)


class TestEdgeCases:
    """Tests for edge cases and boundary conditions."""

    def test_empty_string(self) -> None:
        """Empty string returns empty string."""
        assert strip_ansi("") == ""
        assert clean_terminal_output("") == ""

    def test_only_escape_codes(self) -> None:
        """String with only escape codes returns empty string."""
        result = strip_ansi("\x1b[31m\x1b[0m\x1b[1m")
        assert result == ""

    @given(plain_text.filter(lambda x: "\n" not in x))
    def test_newlines_preserved(self, text: str) -> None:
        """Newlines in plain text are preserved."""
        with_newlines = f"line1\n{text}\nline3"
        result = strip_ansi(with_newlines)
        assert result == with_newlines
        assert result.count("\n") == 2

    def test_malformed_escapes(self) -> None:
        """Malformed escape sequences are handled gracefully.

        This is an edge case where the regex may consume part of following text.
        We just verify it doesn't crash and preserves some meaningful content.
        (Migrated from test_ansi.py)
        """
        result = clean_terminal_output("hello\x1b[world")
        assert "hello" in result  # At least hello is preserved
