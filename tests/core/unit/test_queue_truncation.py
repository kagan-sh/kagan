from __future__ import annotations

from kagan.core.utils import truncate_queue_payload


def test_truncate_queue_payload_returns_original_when_within_limit() -> None:
    content = "hello"
    assert truncate_queue_payload(content, max_chars=10) == content


def test_truncate_queue_payload_adds_prefix_and_tail_when_truncated() -> None:
    content = "abcdefghijklmnopqrstuvwxyz"
    result = truncate_queue_payload(content, prefix="[cut]\n", max_chars=12)
    assert result == "[cut]\nuvwxyz"


def test_truncate_queue_payload_handles_tiny_or_zero_limits() -> None:
    content = "abcdef"
    assert truncate_queue_payload(content, prefix="[cut]\n", max_chars=5) == "bcdef"
    assert truncate_queue_payload(content, max_chars=0) == ""
