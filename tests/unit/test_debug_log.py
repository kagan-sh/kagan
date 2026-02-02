"""Unit tests for debug logging."""

from __future__ import annotations

import pytest

from kagan.debug_log import KaganLogger, clear_log_buffer, log_buffer
from kagan.limits import MAX_LOG_MESSAGE_LENGTH

pytestmark = pytest.mark.unit


class TestLogTruncation:
    """Tests for log message truncation."""

    def test_log_truncates_oversized_messages(self):
        """Very large log messages should be truncated to prevent memory bloat."""
        clear_log_buffer()
        logger = KaganLogger()
        large_message = "x" * 10000  # Much larger than MAX_LOG_MESSAGE_LENGTH

        logger.info(large_message)

        assert len(log_buffer) == 1
        logged_message = log_buffer[0].message
        assert len(logged_message) <= MAX_LOG_MESSAGE_LENGTH + 20  # Allow for truncation suffix
        assert "... [truncated]" in logged_message

    def test_log_preserves_small_messages(self):
        """Small messages should be preserved exactly."""
        clear_log_buffer()
        logger = KaganLogger()
        small_message = "This is a normal log message"

        logger.info(small_message)

        assert len(log_buffer) == 1
        assert log_buffer[0].message == small_message
        assert "truncated" not in log_buffer[0].message

    def test_log_truncates_at_exact_boundary(self):
        """Messages exactly at the limit should not be truncated."""
        clear_log_buffer()
        logger = KaganLogger()
        exact_message = "y" * MAX_LOG_MESSAGE_LENGTH

        logger.info(exact_message)

        assert len(log_buffer) == 1
        assert log_buffer[0].message == exact_message
        assert "truncated" not in log_buffer[0].message

    def test_log_truncates_one_over_boundary(self):
        """Messages one character over the limit should be truncated."""
        clear_log_buffer()
        logger = KaganLogger()
        over_message = "z" * (MAX_LOG_MESSAGE_LENGTH + 1)

        logger.info(over_message)

        assert len(log_buffer) == 1
        logged_message = log_buffer[0].message
        assert "... [truncated]" in logged_message
        # The message should start with the truncated content
        assert logged_message.startswith("z" * 100)
