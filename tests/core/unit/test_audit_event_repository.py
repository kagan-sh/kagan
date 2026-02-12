"""Unit tests for AuditRepository."""

from __future__ import annotations

import asyncio
import tempfile
from pathlib import Path

import pytest

from kagan.core.adapters.db.repositories import AuditRepository, TaskRepository


@pytest.fixture
async def audit_repo():
    """Create a test AuditRepository backed by a temporary database."""
    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = Path(tmpdir) / "test_audit.db"
        task_repo = TaskRepository(db_path)
        await task_repo.initialize()

        repo = AuditRepository(task_repo.session_factory)
        yield repo
        await task_repo.close()


async def test_record_creates_event(audit_repo: AuditRepository) -> None:
    """record() creates an audit event and returns it with all fields populated."""
    event = await audit_repo.record(
        actor_type="user",
        actor_id="u-123",
        session_id="sess-abc",
        capability="tasks",
        command_name="create",
        payload_json='{"title": "hello"}',
        result_json='{"id": "t1"}',
        success=True,
    )

    assert event.id is not None
    assert len(event.id) == 8
    assert event.occurred_at is not None
    assert event.actor_type == "user"
    assert event.actor_id == "u-123"
    assert event.session_id == "sess-abc"
    assert event.capability == "tasks"
    assert event.command_name == "create"
    assert event.payload_json == '{"title": "hello"}'
    assert event.result_json == '{"id": "t1"}'
    assert event.success is True


async def test_list_events_descending_order(audit_repo: AuditRepository) -> None:
    """list_events() returns events in descending occurred_at order."""
    e1 = await audit_repo.record(
        actor_type="user",
        actor_id="u-1",
        capability="tasks",
        command_name="create",
    )
    await asyncio.sleep(0.01)
    e2 = await audit_repo.record(
        actor_type="agent",
        actor_id="a-1",
        capability="review",
        command_name="approve",
    )
    await asyncio.sleep(0.01)
    e3 = await audit_repo.record(
        actor_type="system",
        actor_id="sys",
        capability="tasks",
        command_name="update",
    )

    events = await audit_repo.list_events()

    assert len(events) == 3
    assert events[0].id == e3.id
    assert events[1].id == e2.id
    assert events[2].id == e1.id


async def test_list_events_filters_by_capability(audit_repo: AuditRepository) -> None:
    """list_events(capability=...) filters correctly."""
    await audit_repo.record(
        actor_type="user",
        actor_id="u-1",
        capability="tasks",
        command_name="create",
    )
    await audit_repo.record(
        actor_type="agent",
        actor_id="a-1",
        capability="review",
        command_name="approve",
    )
    await audit_repo.record(
        actor_type="user",
        actor_id="u-2",
        capability="review",
        command_name="reject",
    )

    review_events = await audit_repo.list_events(capability="review")

    assert len(review_events) == 2
    assert all(e.capability == "review" for e in review_events)

    task_events = await audit_repo.list_events(capability="tasks")

    assert len(task_events) == 1
    assert task_events[0].capability == "tasks"


async def test_cursor_pagination(audit_repo: AuditRepository) -> None:
    """Cursor pagination returns only events older than the cursor."""
    e1 = await audit_repo.record(
        actor_type="user",
        actor_id="u-1",
        capability="tasks",
        command_name="op1",
    )
    await asyncio.sleep(0.01)
    e2 = await audit_repo.record(
        actor_type="user",
        actor_id="u-1",
        capability="tasks",
        command_name="op2",
    )
    await asyncio.sleep(0.01)
    e3 = await audit_repo.record(
        actor_type="user",
        actor_id="u-1",
        capability="tasks",
        command_name="op3",
    )

    # Use e3's occurred_at as cursor — should return e2 and e1
    cursor = e3.occurred_at.isoformat()
    events = await audit_repo.list_events(cursor=cursor)

    assert len(events) == 2
    assert events[0].id == e2.id
    assert events[1].id == e1.id

    # Use e2's occurred_at as cursor — should return only e1
    cursor2 = e2.occurred_at.isoformat()
    events2 = await audit_repo.list_events(cursor=cursor2)

    assert len(events2) == 1
    assert events2[0].id == e1.id


async def test_list_events_respects_limit(audit_repo: AuditRepository) -> None:
    """list_events(limit=N) returns at most N events."""
    for i in range(5):
        await audit_repo.record(
            actor_type="user",
            actor_id="u-1",
            capability="tasks",
            command_name=f"op{i}",
        )

    events = await audit_repo.list_events(limit=3)

    assert len(events) == 3


async def test_record_with_failure(audit_repo: AuditRepository) -> None:
    """record() with success=False stores the failure correctly."""
    event = await audit_repo.record(
        actor_type="agent",
        actor_id="a-fail",
        capability="merge",
        command_name="execute",
        result_json='{"error": "conflict"}',
        success=False,
    )

    assert event.success is False
    assert event.result_json == '{"error": "conflict"}'

    events = await audit_repo.list_events()
    assert len(events) == 1
    assert events[0].success is False
