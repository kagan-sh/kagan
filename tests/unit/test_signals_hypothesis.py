"""Property-based tests for signal parsing using Hypothesis."""

from __future__ import annotations

import pytest
from hypothesis import given
from hypothesis import strategies as st

from kagan.agents.signals import Signal, SignalResult, parse_signal
from tests.strategies import (
    approve_signals,
    blocked_signals,
    plain_text,
    reject_signals,
    text_with_signal,
)

pytestmark = pytest.mark.unit


class TestParseSignalProperties:
    """Property-based tests for parse_signal function."""

    @given(plain_text)
    def test_parse_always_returns_signal_result(self, text: str) -> None:
        """parse_signal always returns a valid SignalResult."""
        result = parse_signal(text)
        assert isinstance(result, SignalResult)
        assert isinstance(result.signal, Signal)
        assert isinstance(result.reason, str)

    @given(plain_text.filter(lambda x: "<" not in x and ">" not in x))
    def test_plain_text_returns_continue(self, text: str) -> None:
        """Text without any tags returns CONTINUE signal."""
        result = parse_signal(text)
        assert result.signal == Signal.CONTINUE
        assert result.reason == ""

    @given(text_with_signal())
    def test_signal_detected_when_present(self, data: tuple[str, str]) -> None:
        """When a signal tag is present, it's detected."""
        full_text, signal_tag = data
        result = parse_signal(full_text)

        # Determine expected signal from tag
        tag_lower = signal_tag.lower()
        if "complete" in tag_lower:
            assert result.signal == Signal.COMPLETE
        elif "continue" in tag_lower:
            assert result.signal == Signal.CONTINUE
        elif "blocked" in tag_lower:
            assert result.signal == Signal.BLOCKED
        elif "approve" in tag_lower:
            assert result.signal == Signal.APPROVE
        elif "reject" in tag_lower:
            assert result.signal == Signal.REJECT

    @given(blocked_signals())
    def test_blocked_extracts_reason(self, signal: str) -> None:
        """Blocked signal extracts the reason attribute."""
        result = parse_signal(signal)
        assert result.signal == Signal.BLOCKED
        assert result.reason != ""

    @given(approve_signals())
    def test_approve_extracts_summary(self, signal: str) -> None:
        """Approve signal extracts the summary attribute."""
        result = parse_signal(signal)
        assert result.signal == Signal.APPROVE
        assert result.reason != ""

    @given(reject_signals())
    def test_reject_extracts_reason(self, signal: str) -> None:
        """Reject signal extracts the reason attribute."""
        result = parse_signal(signal)
        assert result.signal == Signal.REJECT
        assert result.reason != ""

    def test_approve_extracts_all_attributes(self) -> None:
        """Approve signal extracts summary, approach, and key_files attributes."""
        signal = '<approve summary="Added validation" approach="Pydantic" key_files="src/v.py"/>'
        result = parse_signal(signal)
        assert result.signal == Signal.APPROVE
        assert result.reason == "Added validation"
        assert result.approach == "Pydantic"
        assert result.key_files == "src/v.py"

    def test_approve_with_only_summary(self) -> None:
        """Approve signal works with only summary (backwards compatible)."""
        signal = '<approve summary="Done"/>'
        result = parse_signal(signal)
        assert result.signal == Signal.APPROVE
        assert result.reason == "Done"
        assert result.approach == ""
        assert result.key_files == ""

    def test_approve_with_partial_attributes(self) -> None:
        """Approve signal works with partial attributes."""
        signal = '<approve summary="Done" key_files="main.py"/>'
        result = parse_signal(signal)
        assert result.signal == Signal.APPROVE
        assert result.reason == "Done"
        assert result.approach == ""
        assert result.key_files == "main.py"


class TestPatternPriority:
    """Tests for pattern priority behavior.

    The parse_signal function checks patterns in a fixed order:
    COMPLETE > BLOCKED > CONTINUE > APPROVE > REJECT

    When multiple signals are present, the highest-priority pattern wins,
    regardless of position in the text.
    """

    @given(st.permutations(["<complete/>", "<continue/>", '<blocked reason="x"/>']))
    def test_complete_wins_over_others(self, signals: list[str]) -> None:
        """COMPLETE signal takes priority over CONTINUE and BLOCKED."""
        text = " ".join(signals)
        result = parse_signal(text)
        # Complete is checked first in pattern list, so it always wins
        assert result.signal == Signal.COMPLETE

    @given(st.permutations(["<continue/>", '<blocked reason="test"/>']))
    def test_blocked_wins_over_continue(self, signals: list[str]) -> None:
        """BLOCKED signal takes priority over CONTINUE."""
        text = " ".join(signals)
        result = parse_signal(text)
        # Blocked is checked before Continue in pattern list
        assert result.signal == Signal.BLOCKED

    def test_pattern_priority_order(self) -> None:
        """Verify the documented pattern priority order."""
        # Complete wins over everything
        assert parse_signal("<continue/><complete/>").signal == Signal.COMPLETE
        assert parse_signal('<blocked reason="x"/><complete/>').signal == Signal.COMPLETE

        # Blocked wins over Continue
        assert parse_signal('<continue/><blocked reason="x"/>').signal == Signal.BLOCKED


class TestCaseInsensitivity:
    """Tests for case-insensitive signal parsing."""

    @given(st.sampled_from(["<COMPLETE/>", "<Complete/>", "<complete/>", "<CoMpLeTe/>"]))
    def test_complete_case_insensitive(self, tag: str) -> None:
        """Complete signal is case-insensitive."""
        result = parse_signal(tag)
        assert result.signal == Signal.COMPLETE

    @given(st.sampled_from(["<CONTINUE/>", "<Continue/>", "<continue/>", "<CoNtInUe/>"]))
    def test_continue_case_insensitive(self, tag: str) -> None:
        """Continue signal is case-insensitive."""
        result = parse_signal(tag)
        assert result.signal == Signal.CONTINUE
