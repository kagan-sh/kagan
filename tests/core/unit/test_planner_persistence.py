"""Unit tests for PlannerRepository."""

from __future__ import annotations

import tempfile
from pathlib import Path

import pytest

from kagan.core.adapters.db.repositories import PlannerRepository, TaskRepository
from kagan.core.models.enums import ProposalStatus


@pytest.fixture
async def planner_repo():
    """Create a test PlannerRepository backed by a temporary database."""
    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = Path(tmpdir) / "test_planner.db"
        task_repo = TaskRepository(db_path)
        await task_repo.initialize()

        project_id = await task_repo.ensure_test_project("Test Project")
        repo = PlannerRepository(task_repo.session_factory)
        yield repo, project_id
        await task_repo.close()


async def test_save_proposal_creates_draft(planner_repo) -> None:
    """save_proposal() creates a draft proposal with all fields."""
    repo, project_id = planner_repo
    tasks_json = [{"title": "Add login page", "task_type": "PAIR", "priority": "MED"}]
    todos_json = [{"content": "Analyze scope", "status": "completed"}]

    proposal = await repo.save_proposal(
        project_id=project_id,
        repo_id="repo-abc",
        tasks_json=tasks_json,
        todos_json=todos_json,
    )

    assert proposal.id is not None
    assert len(proposal.id) == 8
    assert proposal.project_id == project_id
    assert proposal.repo_id == "repo-abc"
    assert proposal.status == ProposalStatus.DRAFT
    assert proposal.tasks_json == tasks_json
    assert proposal.todos_json == todos_json
    assert proposal.created_at is not None


async def test_get_proposal(planner_repo) -> None:
    """get_proposal() fetches a proposal by ID."""
    repo, project_id = planner_repo
    created = await repo.save_proposal(
        project_id=project_id,
        tasks_json=[{"title": "Test task"}],
    )

    fetched = await repo.get_proposal(created.id)

    assert fetched is not None
    assert fetched.id == created.id
    assert fetched.tasks_json == [{"title": "Test task"}]


async def test_get_proposal_returns_none_for_missing(planner_repo) -> None:
    """get_proposal() returns None for non-existent ID."""
    repo, _ = planner_repo
    assert await repo.get_proposal("no-such-id") is None


async def test_list_pending_returns_drafts_only(planner_repo) -> None:
    """list_pending() returns only draft proposals, newest first."""
    repo, project_id = planner_repo
    p1 = await repo.save_proposal(
        project_id=project_id,
        tasks_json=[{"title": "First"}],
    )
    p2 = await repo.save_proposal(
        project_id=project_id,
        tasks_json=[{"title": "Second"}],
    )
    await repo.update_status(p1.id, ProposalStatus.APPROVED)

    pending = await repo.list_pending(project_id)

    assert len(pending) == 1
    assert pending[0].id == p2.id


async def test_list_pending_filters_by_repo(planner_repo) -> None:
    """list_pending() filters by repo_id when provided."""
    repo, project_id = planner_repo
    await repo.save_proposal(
        project_id=project_id,
        repo_id="repo-a",
        tasks_json=[{"title": "In repo A"}],
    )
    await repo.save_proposal(
        project_id=project_id,
        repo_id="repo-b",
        tasks_json=[{"title": "In repo B"}],
    )

    repo_a_pending = await repo.list_pending(project_id, repo_id="repo-a")
    repo_b_pending = await repo.list_pending(project_id, repo_id="repo-b")

    assert len(repo_a_pending) == 1
    assert repo_a_pending[0].tasks_json[0]["title"] == "In repo A"
    assert len(repo_b_pending) == 1
    assert repo_b_pending[0].tasks_json[0]["title"] == "In repo B"


async def test_update_status_transitions(planner_repo) -> None:
    """update_status() transitions proposal status correctly."""
    repo, project_id = planner_repo
    proposal = await repo.save_proposal(
        project_id=project_id,
        tasks_json=[{"title": "Status test"}],
    )

    updated = await repo.update_status(proposal.id, ProposalStatus.APPROVED)

    assert updated is not None
    assert updated.status == ProposalStatus.APPROVED
    assert updated.updated_at >= proposal.created_at


async def test_update_status_returns_none_for_missing(planner_repo) -> None:
    """update_status() returns None for non-existent proposal."""
    repo, _ = planner_repo
    assert await repo.update_status("no-such-id", ProposalStatus.REJECTED) is None


async def test_delete_proposal(planner_repo) -> None:
    """delete_proposal() removes a proposal and returns True."""
    repo, project_id = planner_repo
    proposal = await repo.save_proposal(
        project_id=project_id,
        tasks_json=[{"title": "To delete"}],
    )

    deleted = await repo.delete_proposal(proposal.id)

    assert deleted is True
    assert await repo.get_proposal(proposal.id) is None


async def test_delete_proposal_returns_false_for_missing(planner_repo) -> None:
    """delete_proposal() returns False for non-existent ID."""
    repo, _ = planner_repo
    assert await repo.delete_proposal("no-such-id") is False


async def test_save_proposal_with_no_repo_id(planner_repo) -> None:
    """save_proposal() works when repo_id is None."""
    repo, project_id = planner_repo
    proposal = await repo.save_proposal(
        project_id=project_id,
        tasks_json=[{"title": "No repo"}],
    )

    assert proposal.repo_id is None
    pending = await repo.list_pending(project_id)
    assert len(pending) == 1
