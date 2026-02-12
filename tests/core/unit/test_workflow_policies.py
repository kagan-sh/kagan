from __future__ import annotations

import pytest

from kagan.core.models.enums import (
    TaskStatus,
    transition_status_from_agent_complete,
    transition_status_from_review_pass,
    transition_status_from_review_reject,
)


@pytest.mark.parametrize("current_status", list(TaskStatus))
def test_transition_status_from_agent_complete_success_only_advances_in_progress(
    current_status: TaskStatus,
) -> None:
    updated = transition_status_from_agent_complete(current_status, success=True)

    if current_status is TaskStatus.IN_PROGRESS:
        assert updated is TaskStatus.REVIEW
        return

    assert updated is current_status


@pytest.mark.parametrize("current_status", list(TaskStatus))
def test_transition_status_from_agent_complete_failure_is_noop(current_status: TaskStatus) -> None:
    assert transition_status_from_agent_complete(current_status, success=False) is current_status


@pytest.mark.parametrize("current_status", list(TaskStatus))
def test_transition_status_from_review_pass_only_advances_review(
    current_status: TaskStatus,
) -> None:
    updated = transition_status_from_review_pass(current_status)

    if current_status is TaskStatus.REVIEW:
        assert updated is TaskStatus.DONE
        return

    assert updated is current_status


@pytest.mark.parametrize("current_status", list(TaskStatus))
def test_transition_status_from_review_reject_only_moves_review_to_in_progress(
    current_status: TaskStatus,
) -> None:
    updated = transition_status_from_review_reject(current_status)

    if current_status is TaskStatus.REVIEW:
        assert updated is TaskStatus.IN_PROGRESS
        return

    assert updated is current_status
