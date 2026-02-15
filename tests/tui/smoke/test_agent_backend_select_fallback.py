"""Tests for AgentBackendSelect value validation and fallback behavior."""

from __future__ import annotations

import asyncio

import pytest
from tests.helpers.wait import wait_for_screen
from textual.app import App, ComposeResult
from textual.widgets import Button, Input, Select, Static

from kagan.core.config import KaganConfig
from kagan.tui.ui.modals.task_details_modal import TaskDetailsModal


class _HarnessApp(App[None]):
    def __init__(self, config: KaganConfig) -> None:
        super().__init__()
        self.config = config
        self.kagan_app = self
        self._ctx = None

    def compose(self) -> ComposeResult:
        yield Static("host")


@pytest.mark.asyncio
async def test_task_details_modal_mounts_with_unknown_agent_backend() -> None:
    """TaskDetailsModal does not crash when task has an agent_backend not in options."""
    from datetime import datetime

    from kagan.core.adapters.db.schema import Task
    from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType

    now = datetime.now()
    task = Task(
        id="task-unknown-agent",
        project_id="proj-1",
        title="Unknown agent task",
        description="",
        status=TaskStatus.BACKLOG,
        priority=TaskPriority.MEDIUM,
        task_type=TaskType.AUTO,
        created_at=now,
        updated_at=now,
    )
    task.agent_backend = "nonexistent-agent"

    app = _HarnessApp(KaganConfig())

    async with app.run_test(size=(120, 40)) as pilot:
        loop = asyncio.get_running_loop()
        result_future: asyncio.Future[object | None] = loop.create_future()
        pilot.app.push_screen(
            TaskDetailsModal(task, start_editing=True),
            callback=lambda result: result_future.set_result(result),
        )
        modal = await wait_for_screen(pilot, TaskDetailsModal, timeout=5.0)

        # Modal should mount without InvalidSelectValueError.
        agent_select = modal.query_one("#agent-backend-select", Select)
        assert agent_select.value is not Select.BLANK

        modal.query_one("#title-input", Input).value = "Fixed title"
        modal.query_one("#save-btn", Button).press()

        result = await result_future

    assert isinstance(result, dict)
    # The agent_backend should be a valid agent key, not the original invalid one.
    assert result["agent_backend"] != "nonexistent-agent"
