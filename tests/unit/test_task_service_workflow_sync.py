from __future__ import annotations

from kagan.core.models.enums import TaskStatus


async def test_sync_status_from_agent_complete_transitions_and_noops(
    state_manager,
    task_factory,
    task_service,
) -> None:
    task = task_factory(title="Workflow test", status=TaskStatus.IN_PROGRESS)
    created = await state_manager.create(task)

    updated = await task_service.sync_status_from_agent_complete(created.id, success=True)
    assert updated is not None
    assert updated.status == TaskStatus.REVIEW

    no_change = await task_service.sync_status_from_agent_complete(created.id, success=False)
    assert no_change is not None
    assert no_change.status == TaskStatus.REVIEW


async def test_sync_status_from_review_transitions(
    state_manager,
    task_factory,
    task_service,
) -> None:
    task = task_factory(title="Review pass", status=TaskStatus.REVIEW)
    created = await state_manager.create(task)

    approved = await task_service.sync_status_from_review_pass(created.id)
    assert approved is not None
    assert approved.status == TaskStatus.DONE

    # Rejection only transitions REVIEW -> IN_PROGRESS, so set it back first.
    moved_back = await task_service.update_fields(created.id, status=TaskStatus.REVIEW)
    assert moved_back is not None
    rejected = await task_service.sync_status_from_review_reject(created.id, reason="needs work")
    assert rejected is not None
    assert rejected.status == TaskStatus.IN_PROGRESS
