"""Snapshot tests for Kanban screen user flows.

These tests cover the main Kanban interaction flows:
- Board display with tasks in columns
- Search functionality
- Delete confirmation modal

Note: Tests are synchronous because pytest-textual-snapshot's snap_compare
internally calls asyncio.run(), which conflicts with async test functions.
"""

from __future__ import annotations

import asyncio
import sys
from datetime import datetime
from typing import TYPE_CHECKING, Any

import pytest
from tests.helpers.journey_runner import bundle_snapshots, execute_test_actions, parse_actions
from tests.helpers.wait import wait_for_screen
from tests.tui.snapshot.conftest import _normalize_svg

from kagan.core.adapters.db.repositories import TaskRepository
from kagan.core.adapters.db.schema import Task
from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType
from kagan.tui.app import KaganApp

if TYPE_CHECKING:
    from types import SimpleNamespace

    from tests.tui.snapshot.conftest import MockAgentFactory


SNAPSHOT_TIME = datetime(2024, 1, 1, 12, 0, 0)
pytestmark = pytest.mark.usefixtures("global_mock_tmux")


async def _setup_kanban_tasks(db_path: str) -> None:
    """Pre-populate database with tasks in different columns.

    Uses fixed IDs for snapshot reproducibility.
    Note: snapshot_project fixture already creates the test project,
    so we just need to get the project_id here.
    """
    manager = TaskRepository(db_path)
    await manager.initialize()

    project_id = manager.default_project_id
    if project_id is None:
        project_id = await manager.ensure_test_project("Kanban Test Project")

    tasks = [
        Task(
            id="backlog1",
            project_id=project_id,
            title="Backlog task 1",
            description="First task in backlog",
            priority=TaskPriority.LOW,
            status=TaskStatus.BACKLOG,
            task_type=TaskType.PAIR,
            created_at=SNAPSHOT_TIME,
            updated_at=SNAPSHOT_TIME,
        ),
        Task(
            id="backlog2",
            project_id=project_id,
            title="Backlog task 2",
            description="Second task in backlog",
            priority=TaskPriority.HIGH,
            status=TaskStatus.BACKLOG,
            task_type=TaskType.AUTO,
            created_at=SNAPSHOT_TIME,
            updated_at=SNAPSHOT_TIME,
        ),
        Task(
            id="inprog01",
            project_id=project_id,
            title="In progress task",
            description="Currently working on this",
            priority=TaskPriority.HIGH,
            status=TaskStatus.IN_PROGRESS,
            task_type=TaskType.PAIR,
            created_at=SNAPSHOT_TIME,
            updated_at=SNAPSHOT_TIME,
        ),
        Task(
            id="review01",
            project_id=project_id,
            title="Review task",
            description="Ready for code review",
            priority=TaskPriority.MEDIUM,
            status=TaskStatus.REVIEW,
            task_type=TaskType.AUTO,
            created_at=SNAPSHOT_TIME,
            updated_at=SNAPSHOT_TIME,
        ),
        Task(
            id="done0001",
            project_id=project_id,
            title="Done task",
            description="Completed work",
            priority=TaskPriority.LOW,
            status=TaskStatus.DONE,
            task_type=TaskType.PAIR,
            created_at=SNAPSHOT_TIME,
            updated_at=SNAPSHOT_TIME,
        ),
    ]

    for task in tasks:
        await manager.create(task)
    await manager.close()


class TestKanbanFlow:
    @pytest.fixture
    def kanban_app(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
    ) -> KaganApp:
        """Create app with pre-populated tasks for kanban testing."""
        asyncio.run(_setup_kanban_tasks(snapshot_project.db))

        return KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            project_root=snapshot_project.root,
            agent_factory=mock_acp_agent_factory,
        )

    @pytest.mark.skipif(sys.platform == "win32", reason="Timing-sensitive; flaky on Windows CI")
    def test_kanban_journey(
        self,
        kanban_app: KaganApp,
        snapshot: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Scripted Kanban journey with multiple snapshots."""

        async def run_flow() -> dict[str, str]:
            from kagan.tui.ui.screens.kanban import KanbanScreen

            cols, rows = snapshot_terminal_size
            async with kanban_app.run_test(headless=True, size=(cols, rows)) as pilot:
                screen = await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
                screen.focus_first_card()
                snapshots = await execute_test_actions(
                    pilot,
                    parse_actions("shot(board) slash (backlog) shot(search) escape"),
                )
                return snapshots

        snapshots = asyncio.run(run_flow())
        assert snapshots, "No snapshots captured for kanban journey"
        bundle = bundle_snapshots(snapshots, normalizer=_normalize_svg)
        assert snapshot == bundle
