"""Tests for signal parsing."""

from __future__ import annotations

from kagan.agents.signals import Signal, SignalResult, parse_signal


class TestSignalParsing:
    """Tests for parse_signal function."""

    def test_parse_complete_signal(self):
        """Test parsing <complete/> signal."""
        result = parse_signal("Task done! <complete/>")
        assert result.signal == Signal.COMPLETE
        assert result.reason == ""

    def test_parse_complete_signal_case_insensitive(self):
        """Test parsing <COMPLETE/> with different case."""
        result = parse_signal("<COMPLETE/>")
        assert result.signal == Signal.COMPLETE

    def test_parse_complete_signal_with_space(self):
        """Test parsing <complete /> with space."""
        result = parse_signal("<complete />")
        assert result.signal == Signal.COMPLETE

    def test_parse_continue_signal(self):
        """Test parsing <continue/> signal."""
        result = parse_signal("Making progress... <continue/>")
        assert result.signal == Signal.CONTINUE
        assert result.reason == ""

    def test_parse_blocked_signal(self):
        """Test parsing <blocked reason="..."/> signal."""
        result = parse_signal('<blocked reason="Need API key"/>')
        assert result.signal == Signal.BLOCKED
        assert result.reason == "Need API key"

    def test_parse_blocked_signal_complex_reason(self):
        """Test parsing blocked signal with complex reason."""
        result = parse_signal('<blocked reason="Cannot proceed: missing dependencies"/>')
        assert result.signal == Signal.BLOCKED
        assert result.reason == "Cannot proceed: missing dependencies"

    def test_parse_no_signal_defaults_to_continue(self):
        """Test that missing signal defaults to CONTINUE."""
        result = parse_signal("Just some agent output without a signal")
        assert result.signal == Signal.CONTINUE
        assert result.reason == ""

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
        # This tests the parsing order - COMPLETE is checked first
        result = parse_signal("<complete/> <continue/> <blocked reason='test'/>")
        assert result.signal == Signal.COMPLETE


class TestSignalResult:
    """Tests for SignalResult dataclass."""

    def test_signal_result_equality(self):
        """Test SignalResult equality."""
        r1 = SignalResult(Signal.COMPLETE, "")
        r2 = SignalResult(Signal.COMPLETE, "")
        assert r1 == r2

    def test_signal_result_inequality_signal(self):
        """Test SignalResult inequality with different signals."""
        r1 = SignalResult(Signal.COMPLETE, "")
        r2 = SignalResult(Signal.CONTINUE, "")
        assert r1 != r2

    def test_signal_result_inequality_reason(self):
        """Test SignalResult inequality with different reasons."""
        r1 = SignalResult(Signal.BLOCKED, "reason 1")
        r2 = SignalResult(Signal.BLOCKED, "reason 2")
        assert r1 != r2

    def test_signal_result_not_equal_to_other_types(self):
        """Test SignalResult not equal to non-SignalResult."""
        r = SignalResult(Signal.COMPLETE, "")
        assert r != "complete"
        assert r != Signal.COMPLETE
