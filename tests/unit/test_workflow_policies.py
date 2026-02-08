from __future__ import annotations

from kagan.core.models.enums import TaskStatus
from kagan.core.models.policies import (
    transition_status_from_agent_complete,
    transition_status_from_review_pass,
    transition_status_from_review_reject,
)


def test_transition_status_from_agent_complete_success_advances_in_progress() -> None:
    assert transition_status_from_agent_complete(TaskStatus.IN_PROGRESS, True) == TaskStatus.REVIEW


def test_transition_status_from_agent_complete_failure_is_noop() -> None:
    assert (
        transition_status_from_agent_complete(TaskStatus.IN_PROGRESS, False)
        == TaskStatus.IN_PROGRESS
    )


def test_transition_status_from_review_pass_advances_review() -> None:
    assert transition_status_from_review_pass(TaskStatus.REVIEW) == TaskStatus.DONE


def test_transition_status_from_review_reject_returns_to_in_progress() -> None:
    assert transition_status_from_review_reject(TaskStatus.REVIEW) == TaskStatus.IN_PROGRESS


def test_transition_policy_is_noop_for_unrelated_states() -> None:
    assert transition_status_from_review_pass(TaskStatus.BACKLOG) == TaskStatus.BACKLOG
    assert transition_status_from_review_reject(TaskStatus.DONE) == TaskStatus.DONE
