from __future__ import annotations

from tests.helpers.mock_responses.tasks import (
    make_approve_response,
    make_blocked_response,
    make_clarification_response,
    make_complete_response,
    make_reject_response,
)


def test_make_complete_response_when_files_includes_files_section_and_complete_tag() -> None:
    response = make_complete_response(
        "Completed implementation",
        ["src/auth.py", "tests/test_auth.py"],
    )

    assert "Completed implementation" in response
    assert "## Files Changed" in response
    assert "- `src/auth.py`" in response
    assert "- `tests/test_auth.py`" in response
    assert response.endswith("<complete/>")


def test_make_blocked_response_when_context_given_formats_context_and_blocked_tag() -> None:
    response = make_blocked_response(
        reason="Missing DATABASE_URL",
        context="Cannot connect to the database in this environment.",
    )

    assert response == (
        "Cannot connect to the database in this environment.\n\n"
        '<blocked reason="Missing DATABASE_URL"/>'
    )


def test_make_approve_response_when_metadata_provided_includes_metadata_attributes() -> None:
    response = make_approve_response(
        summary="Looks good",
        approach="JWT + middleware",
        key_files="src/auth/jwt.py, src/auth/middleware.py",
        notes="Reviewed security and tests.",
    )

    assert "Reviewed security and tests." in response
    assert '<approve summary="Looks good"' in response
    assert 'approach="JWT + middleware"' in response
    assert 'key_files="src/auth/jwt.py, src/auth/middleware.py"' in response


def test_reject_and_clarification_responses_when_built_include_sections_and_signal_tags() -> None:
    reject_response = make_reject_response(
        reason="Coverage gap",
        issues=["Missing tests for refresh endpoint", "No DB error handling"],
    )
    clarification_response = make_clarification_response(
        questions=["Should refresh tokens rotate?", "Do we need audit logs?"],
        context="Need requirements clarification before implementing.",
    )

    assert "## Issues Found" in reject_response
    assert "1. Missing tests for refresh endpoint" in reject_response
    assert "2. No DB error handling" in reject_response
    assert reject_response.endswith('<reject reason="Coverage gap"/>')
    assert "Need requirements clarification before implementing." in clarification_response
    assert "1. Should refresh tokens rotate?" in clarification_response
    assert "2. Do we need audit logs?" in clarification_response
