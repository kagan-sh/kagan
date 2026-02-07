from __future__ import annotations

import pytest

from kagan.mcp.tools import _format_review_feedback


@pytest.mark.parametrize(
    ("review_result", "expected"),
    [
        ({"status": "approved", "summary": "Looks good"}, "approved: Looks good"),
        ({"approved": True, "summary": "Ship it"}, "approved: Ship it"),
        ({"approved": False, "summary": ""}, "Review rejected."),
        ({"summary": "Only text"}, "Only text"),
        ({}, None),
        ("nope", None),
    ],
)
def test_format_review_feedback(review_result: object, expected: str | None) -> None:
    assert _format_review_feedback(review_result) == expected
