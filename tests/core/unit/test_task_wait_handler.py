"""Unit tests for handle_task_wait request handler."""

from __future__ import annotations

import asyncio


async def test_task_wait_not_found(api_env):
    """tasks.wait returns TASK_NOT_FOUND for missing task."""
    from kagan.core.request_handlers import handle_task_wait

    _, api, _ = api_env
    result = await handle_task_wait(api, {"task_id": "nonexistent"})
    assert result["success"] is False
    assert result["code"] == "TASK_NOT_FOUND"


async def test_task_wait_invalid_timeout_bool(api_env):
    """tasks.wait rejects boolean timeout_seconds."""
    from kagan.core.request_handlers import handle_task_wait

    _, api, _ = api_env
    task = await api.create_task("Wait test", "desc")
    result = await handle_task_wait(api, {"task_id": task.id, "timeout_seconds": True})
    assert result["code"] == "INVALID_TIMEOUT"
    assert result["changed"] is False


async def test_task_wait_invalid_timeout_negative(api_env):
    """tasks.wait rejects negative timeout_seconds."""
    from kagan.core.request_handlers import handle_task_wait

    _, api, _ = api_env
    task = await api.create_task("Wait test", "desc")
    result = await handle_task_wait(api, {"task_id": task.id, "timeout_seconds": -1})
    assert result["code"] == "INVALID_TIMEOUT"


async def test_task_wait_timeout_exceeds_max(api_env):
    """tasks.wait rejects timeout_seconds exceeding server max."""
    from kagan.core.request_handlers import handle_task_wait

    _, api, _ = api_env
    task = await api.create_task("Wait test", "desc")
    result = await handle_task_wait(api, {"task_id": task.id, "timeout_seconds": 9999})
    assert result["code"] == "INVALID_TIMEOUT"
    assert "exceeds" in result["message"]


async def test_task_wait_invalid_status_filter(api_env):
    """tasks.wait rejects invalid wait_for_status values."""
    from kagan.core.request_handlers import handle_task_wait

    _, api, _ = api_env
    task = await api.create_task("Wait test", "desc")
    result = await handle_task_wait(
        api, {"task_id": task.id, "wait_for_status": ["INVALID_STATUS"]}
    )
    assert result["code"] == "INVALID_PARAMS"


async def test_task_wait_already_at_status(api_env):
    """tasks.wait returns immediately if task already at target status."""
    from kagan.core.request_handlers import handle_task_wait

    _, api, _ = api_env
    task = await api.create_task("Wait test", "desc")
    # Task starts at BACKLOG
    result = await handle_task_wait(api, {"task_id": task.id, "wait_for_status": ["BACKLOG"]})
    assert result["changed"] is True
    assert result["timed_out"] is False
    assert result["code"] == "ALREADY_AT_STATUS"
    assert result["current_status"] == "BACKLOG"
    assert result["task"] is not None
    assert result["task"]["id"] == task.id


async def test_task_wait_race_safe_changed_since_cursor(api_env):
    """tasks.wait detects change since from_updated_at cursor."""
    from kagan.core.request_handlers import handle_task_wait

    _, api, _ = api_env
    task = await api.create_task("Wait test", "desc")
    # Use a stale cursor
    result = await handle_task_wait(
        api,
        {
            "task_id": task.id,
            "from_updated_at": "2020-01-01T00:00:00+00:00",
        },
    )
    assert result["changed"] is True
    assert result["code"] == "CHANGED_SINCE_CURSOR"


async def test_task_wait_timeout(api_env):
    """tasks.wait returns timed_out=True after timeout elapses."""
    from kagan.core.request_handlers import handle_task_wait

    _, api, _ = api_env
    task = await api.create_task("Wait test", "desc")
    result = await handle_task_wait(api, {"task_id": task.id, "timeout_seconds": 0.05})
    assert result["timed_out"] is True
    assert result["changed"] is False
    assert result["code"] == "WAIT_TIMEOUT"
    assert result["previous_status"] == "BACKLOG"


async def test_task_wait_event_driven_wakeup(api_env):
    """tasks.wait wakes up on TaskStatusChanged event."""
    from kagan.core.models.enums import TaskStatus
    from kagan.core.request_handlers import handle_task_wait

    _, api, _ctx = api_env
    task = await api.create_task("Wait test", "desc")

    async def _change_status_after_delay():
        await asyncio.sleep(0.05)
        await api.move_task(task.id, TaskStatus.IN_PROGRESS)

    change_task = asyncio.create_task(_change_status_after_delay())
    result = await handle_task_wait(api, {"task_id": task.id, "timeout_seconds": 5})
    await change_task

    assert result["changed"] is True
    assert result["timed_out"] is False
    assert result["code"] == "TASK_CHANGED"
    assert result["current_status"] == "IN_PROGRESS"
    assert result["task"] is not None


async def test_task_wait_status_filter_waits_for_target(api_env):
    """tasks.wait with status filter ignores non-matching transitions."""
    from kagan.core.models.enums import TaskStatus
    from kagan.core.request_handlers import handle_task_wait

    _, api, _ctx = api_env
    task = await api.create_task("Wait test", "desc")

    async def _move_through_statuses():
        await asyncio.sleep(0.05)
        # Move to IN_PROGRESS (not target)
        await api.move_task(task.id, TaskStatus.IN_PROGRESS)
        await asyncio.sleep(0.05)
        # Move to REVIEW (target)
        await api.move_task(task.id, TaskStatus.REVIEW)

    change_task = asyncio.create_task(_move_through_statuses())
    result = await handle_task_wait(
        api,
        {
            "task_id": task.id,
            "wait_for_status": ["REVIEW", "DONE"],
            "timeout_seconds": 5,
        },
    )
    await change_task

    assert result["changed"] is True
    assert result["current_status"] == "REVIEW"


async def test_task_wait_handler_cleanup_on_timeout(api_env):
    """Handler removes event listener after timeout."""
    from kagan.core.bootstrap import InMemoryEventBus
    from kagan.core.request_handlers import handle_task_wait

    _, api, ctx = api_env
    task = await api.create_task("Wait test", "desc")

    bus = ctx.event_bus
    assert isinstance(bus, InMemoryEventBus)
    handlers_before = len(bus._handlers)

    await handle_task_wait(api, {"task_id": task.id, "timeout_seconds": 0.05})

    handlers_after = len(bus._handlers)
    assert handlers_after == handlers_before, "Event handler was not cleaned up"


async def test_task_wait_task_deleted_during_wait(api_env):
    """tasks.wait returns TASK_DELETED when task is deleted during wait."""
    from kagan.core.events import TaskDeleted
    from kagan.core.request_handlers import handle_task_wait

    _, api, ctx = api_env
    task = await api.create_task("Wait test", "desc")

    async def _emit_delete_after_delay():
        await asyncio.sleep(0.05)
        await ctx.event_bus.publish(TaskDeleted(task_id=task.id))

    emit_task = asyncio.create_task(_emit_delete_after_delay())
    result = await handle_task_wait(api, {"task_id": task.id, "timeout_seconds": 5})
    await emit_task

    assert result["changed"] is True
    assert result["code"] == "TASK_DELETED"
    assert result["task"] is None


async def test_task_wait_default_timeout_from_config(api_env):
    """tasks.wait uses configured default timeout when none specified."""
    from kagan.core.models.enums import TaskStatus
    from kagan.core.request_handlers import handle_task_wait

    _, api, ctx = api_env
    ctx.config.general.tasks_wait_default_timeout_seconds = 1
    ctx.config.general.tasks_wait_max_timeout_seconds = 1
    task = await api.create_task("Wait test", "desc")

    async def _change_status_after_delay():
        await asyncio.sleep(0.05)
        await api.move_task(task.id, TaskStatus.IN_PROGRESS)

    change_task = asyncio.create_task(_change_status_after_delay())
    result = await handle_task_wait(api, {"task_id": task.id})
    await change_task

    assert result["changed"] is True
    assert result["timed_out"] is False
    assert result["current_status"] == "IN_PROGRESS"


async def test_task_wait_non_list_status_filter(api_env):
    """tasks.wait rejects unsupported wait_for_status types."""
    from kagan.core.request_handlers import handle_task_wait

    _, api, _ = api_env
    task = await api.create_task("Wait test", "desc")
    result = await handle_task_wait(api, {"task_id": task.id, "wait_for_status": 123})
    assert result["code"] == "INVALID_PARAMS"
    assert "wait_for_status" in result["message"]


async def test_task_wait_accepts_timeout_string(api_env):
    """tasks.wait accepts numeric timeout_seconds sent as string."""
    from kagan.core.request_handlers import handle_task_wait

    _, api, _ = api_env
    task = await api.create_task("Wait test", "desc")
    result = await handle_task_wait(api, {"task_id": task.id, "timeout_seconds": "0.05"})
    assert result["timed_out"] is True
    assert result["code"] == "WAIT_TIMEOUT"


async def test_task_wait_accepts_csv_status_filter(api_env):
    """tasks.wait accepts comma-separated wait_for_status strings."""
    from kagan.core.request_handlers import handle_task_wait

    _, api, _ = api_env
    task = await api.create_task("Wait test", "desc")
    result = await handle_task_wait(api, {"task_id": task.id, "wait_for_status": "BACKLOG,REVIEW"})
    assert result["changed"] is True
    assert result["code"] == "ALREADY_AT_STATUS"
    assert result["current_status"] == "BACKLOG"


async def test_task_wait_accepts_json_status_filter_string(api_env):
    """tasks.wait accepts JSON list strings for wait_for_status."""
    from kagan.core.request_handlers import handle_task_wait

    _, api, _ = api_env
    task = await api.create_task("Wait test", "desc")
    result = await handle_task_wait(api, {"task_id": task.id, "wait_for_status": '["BACKLOG"]'})
    assert result["changed"] is True
    assert result["code"] == "ALREADY_AT_STATUS"
