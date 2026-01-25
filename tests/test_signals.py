"""Tests for signal parsing."""

from kagan.agents.signals import Signal, SignalResult, parse_signal


class TestParseSignal:
    def test_parse_complete(self):
        assert parse_signal("Done!\n<complete/>") == SignalResult(Signal.COMPLETE)

    def test_parse_complete_no_slash(self):
        assert parse_signal("<complete>").signal == Signal.COMPLETE

    def test_parse_continue(self):
        assert parse_signal("Progress...\n<continue/>") == SignalResult(Signal.CONTINUE)

    def test_parse_blocked_with_reason(self):
        result = parse_signal('<blocked reason="need API key"/>')
        assert result.signal == Signal.BLOCKED
        assert result.reason == "need API key"

    def test_default_to_continue(self):
        result = parse_signal("Some output without signal")
        assert result.signal == Signal.CONTINUE

    def test_case_insensitive(self):
        assert parse_signal("<COMPLETE/>").signal == Signal.COMPLETE
        assert parse_signal("<Continue/>").signal == Signal.CONTINUE

    def test_signal_in_middle_of_output(self):
        output = "Did some work\n<complete/>\nMore text"
        assert parse_signal(output).signal == Signal.COMPLETE
