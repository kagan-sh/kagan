"""Tests for review modal stream helper parsing."""

from kagan.ui.modals.review import extract_review_decision


def test_extract_review_decision_from_decision_line() -> None:
    output = """
Reasoning:
- Looked at changes
Decision: Approve
"""
    assert extract_review_decision(output) == "approved"


def test_extract_review_decision_prefers_last_decision() -> None:
    output = """
Decision: Reject
...
Decision: Approve
"""
    assert extract_review_decision(output) == "approved"


def test_extract_review_decision_from_signal_tags() -> None:
    output = "<approve summary='Looks good'/>"
    assert extract_review_decision(output) == "approved"
