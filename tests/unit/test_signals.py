"""Tests for signal parsing."""

from __future__ import annotations

import pytest

from kagan.agents.signals import Signal, parse_signal

pytestmark = pytest.mark.unit


class TestSignalParsing:
    """Tests for parse_signal function."""

    @pytest.mark.parametrize(
        ("text", "expected_signal", "expected_reason"),
        [
            # Complete signal variations
            ("Task done! <complete/>", Signal.COMPLETE, ""),
            ("<COMPLETE/>", Signal.COMPLETE, ""),
            ("<complete />", Signal.COMPLETE, ""),
            # Continue signal
            ("Making progress... <continue/>", Signal.CONTINUE, ""),
            # Blocked signal variations
            ('<blocked reason="Need API key"/>', Signal.BLOCKED, "Need API key"),
            (
                '<blocked reason="Cannot proceed: missing dependencies"/>',
                Signal.BLOCKED,
                "Cannot proceed: missing dependencies",
            ),
            # Approve signal variations
            (
                'Good implementation! <approve summary="Added feature X"/>',
                Signal.APPROVE,
                "Added feature X",
            ),
            ('<APPROVE summary="Done"/>', Signal.APPROVE, "Done"),
            # Reject signal variations
            (
                'Missing tests. <reject reason="No unit tests added"/>',
                Signal.REJECT,
                "No unit tests added",
            ),
            (
                '<reject reason="Code quality issues: missing error handling"/>',
                Signal.REJECT,
                "Code quality issues: missing error handling",
            ),
            # No signal defaults to CONTINUE
            ("Just some agent output without a signal", Signal.CONTINUE, ""),
        ],
    )
    def test_parse_signal(self, text: str, expected_signal: Signal, expected_reason: str):
        """Test parsing various signal formats."""
        result = parse_signal(text)
        assert result.signal == expected_signal
        assert result.reason == expected_reason

    def test_parse_signal_in_longer_text(self):
        """Test parsing signal embedded in longer text."""
        text = """
        I've completed the implementation:
        - Added the new feature
        - Updated tests
        - All tests pass

        <complete/>

        Let me know if you need anything else.
        """
        result = parse_signal(text)
        assert result.signal == Signal.COMPLETE

    def test_parse_multiple_signals_first_wins(self):
        """Test that first matching signal is returned."""
        result = parse_signal("<complete/> <continue/> <blocked reason='test'/>")
        assert result.signal == Signal.COMPLETE
