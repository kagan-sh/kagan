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
from datetime import datetime
from typing import TYPE_CHECKING, Any

import pytest

from kagan.adapters.db.repositories import TaskRepository
from kagan.adapters.db.schema import Task
from kagan.app import KaganApp
from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType

if TYPE_CHECKING:
    from types import SimpleNamespace

    from textual.pilot import Pilot

    from tests.snapshots.conftest import MockAgentFactory


def _create_fake_tmux(sessions: dict[str, Any]) -> object:
    """Create a fake tmux function that tracks session state."""

    async def fake_run_tmux(*args: str) -> str:
        if not args:
            return ""
        command, args_list = args[0], list(args)
        if command == "new-session" and "-s" in args_list:
            idx = args_list.index("-s")
            name = args_list[idx + 1] if idx + 1 < len(args_list) else None
            if name:
                cwd = args_list[args_list.index("-c") + 1] if "-c" in args_list else ""
                env: dict[str, str] = {}
                for i, val in enumerate(args_list):
                    if val == "-e" and i + 1 < len(args_list):
                        key, _, env_value = args_list[i + 1].partition("=")
                        env[key] = env_value
                sessions[name] = {"cwd": cwd, "env": env, "sent_keys": []}
        elif command == "kill-session" and "-t" in args_list:
            sessions.pop(args_list[args_list.index("-t") + 1], None)
        elif command == "send-keys" and "-t" in args_list:
            idx = args_list.index("-t")
            name = args_list[idx + 1]
            keys = args_list[idx + 2] if idx + 2 < len(args_list) else ""
            if name in sessions:
                sessions[name]["sent_keys"].append(keys)
        elif command == "list-sessions":
            return "\n".join(sorted(sessions.keys()))
        return ""

    return fake_run_tmux


SNAPSHOT_TIME = datetime(2024, 1, 1, 12, 0, 0)


async def _setup_kanban_tasks(db_path: str) -> None:
    """Pre-populate database with tasks in different columns.

    Uses fixed IDs for snapshot reproducibility.
    Note: snapshot_project fixture already creates the test project,
    so we just need to get the project_id here.
    """
    manager = TaskRepository(db_path)
    await manager.initialize()
    # The snapshot_project fixture already created the project and linked the repo
    # Just ensure we have a project_id to use
    project_id = manager.default_project_id
    if project_id is None:
        # Fallback: create test project if not already done
        project_id = await manager.ensure_test_project("Kanban Test Project")

    # Create tasks with fixed IDs for reproducible snapshots
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
    """Snapshot tests for Kanban screen user flows."""

    @pytest.fixture
    def kanban_app(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        monkeypatch: pytest.MonkeyPatch,
    ) -> KaganApp:
        """Create app with pre-populated tasks for kanban testing."""
        # Mock tmux
        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)

        # Set up tasks synchronously
        loop = asyncio.new_event_loop()
        try:
            loop.run_until_complete(_setup_kanban_tasks(snapshot_project.db))
        finally:
            loop.close()

        return KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            project_root=snapshot_project.root,
            agent_factory=mock_acp_agent_factory,
        )

    @pytest.mark.snapshot
    def test_kanban_board_with_tasks(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Kanban board displays tasks in their respective columns."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Verify we're on KanbanScreen (since we have tasks)
            from kagan.ui.screens.kanban import KanbanScreen

            assert isinstance(pilot.app.screen, KanbanScreen)

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_kanban_search_with_query(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """User types search query in search bar, tasks are filtered."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open search bar and type query
            await pilot.press("slash")
            await pilot.pause()
            # Type "backlog" to filter
            for char in "backlog":
                await pilot.press(char)
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_kanban_delete_confirmation(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Pressing 'x' shows delete confirmation modal."""

        async def run_before(pilot: Pilot) -> None:
            from kagan.ui.modals.confirm import ConfirmModal

            await pilot.pause()
            # Press 'x' to open delete confirmation modal
            await pilot.press("x")
            await pilot.pause()
            # Wait for the modal to appear on the screen stack
            max_wait = 5.0
            waited = 0.0
            while waited < max_wait:
                # Check if ConfirmModal is in the screen stack
                for screen in pilot.app.screen_stack:
                    if isinstance(screen, ConfirmModal):
                        # Modal is visible - take snapshot now BEFORE pressing Enter
                        await pilot.pause()
                        return
                await asyncio.sleep(0.1)
                waited += 0.1
            # Fallback - modal should be there by now
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)
