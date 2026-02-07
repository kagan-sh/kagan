"""Snapshot tests for modal dialog flows.

These tests cover modal interactions:
- Help modal (F1) with tabbed content
- Settings modal (,) with configuration options
- Task details modal (n, e, v) for create/edit/view
- Confirm modal (x) for delete confirmation
- Diff modal for review approve/reject

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


async def _setup_modal_test_tasks(db_path: str) -> None:
    """Pre-populate database with tasks for modal testing.

    Uses fixed IDs for snapshot reproducibility.
    """
    manager = TaskRepository(db_path)
    await manager.initialize()
    project_id = manager.default_project_id
    if project_id is None:
        raise RuntimeError("TaskRepository defaults not initialized")
    repo_id = manager.default_repo_id

    tasks = [
        Task(
            id="modal001",
            project_id=project_id,
            repo_id=repo_id,
            title="Test task for modals",
            description="A task to test modal interactions with detailed content.",
            priority=TaskPriority.MEDIUM,
            status=TaskStatus.BACKLOG,
            task_type=TaskType.PAIR,
            acceptance_criteria=["First criterion", "Second criterion"],
            created_at=SNAPSHOT_TIME,
            updated_at=SNAPSHOT_TIME,
        ),
        Task(
            id="modal002",
            project_id=project_id,
            repo_id=repo_id,
            title="Review task for diff modal",
            description="This task is ready for review and diff viewing.",
            priority=TaskPriority.HIGH,
            status=TaskStatus.REVIEW,
            task_type=TaskType.AUTO,
            created_at=SNAPSHOT_TIME,
            updated_at=SNAPSHOT_TIME,
        ),
        Task(
            id="modal003",
            project_id=project_id,
            repo_id=repo_id,
            title="Another backlog task",
            description="Secondary test task",
            priority=TaskPriority.LOW,
            status=TaskStatus.BACKLOG,
            task_type=TaskType.AUTO,
            created_at=SNAPSHOT_TIME,
            updated_at=SNAPSHOT_TIME,
        ),
    ]

    for task in tasks:
        await manager.create(task)
    await manager.close()


class TestModalFlow:
    """Snapshot tests for modal dialog flows."""

    @pytest.fixture
    def modal_app(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        monkeypatch: pytest.MonkeyPatch,
    ) -> KaganApp:
        """Create app with pre-populated tasks for modal testing."""
        # Mock tmux
        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)

        # Set up tasks synchronously
        loop = asyncio.new_event_loop()
        try:
            loop.run_until_complete(_setup_modal_test_tasks(snapshot_project.db))
        finally:
            loop.close()

        return KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            project_root=snapshot_project.root,
            agent_factory=mock_acp_agent_factory,
        )

    # =========================================================================
    # Help Modal Tests (F1)
    # =========================================================================

    @pytest.mark.snapshot
    def test_help_modal_displayed(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """F1 opens help modal with tabbed content."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open help modal
            await pilot.press("f1")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_help_modal_navigation_tab(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Tab navigates through different help sections."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open help modal
            await pilot.press("f1")
            await pilot.pause()
            # Navigate to second tab
            await pilot.press("tab")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_help_modal_close_escape(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Escape closes help modal and returns to kanban board."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open help modal
            await pilot.press("f1")
            await pilot.pause()
            # Close with escape
            await pilot.press("escape")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    # =========================================================================
    # Settings Modal Tests (,)
    # =========================================================================

    @pytest.mark.snapshot
    def test_settings_modal_displayed(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Comma opens settings modal."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open settings modal
            await pilot.press("comma")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_settings_modal_form_fields(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Settings form shows configuration options with tab navigation."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open settings modal
            await pilot.press("comma")
            await pilot.pause()
            # Tab through form fields to show focus
            await pilot.press("tab")
            await pilot.pause()
            await pilot.press("tab")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_settings_modal_close_escape(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Escape closes settings modal without saving."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open settings modal
            await pilot.press("comma")
            await pilot.pause()
            # Close with escape
            await pilot.press("escape")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    # =========================================================================
    # Task Details Modal Tests (n, e, v)
    # =========================================================================

    @pytest.mark.snapshot
    def test_task_create_modal_empty(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Pressing 'n' opens empty create form."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open create task modal
            await pilot.press("n")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_task_create_modal_filled(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Create form with typed content."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open create task modal
            await pilot.press("n")
            await pilot.pause()
            # Type a title
            for char in "New test task":
                await pilot.press(char)
            await pilot.pause()
            # Tab to description
            await pilot.press("tab")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_task_edit_modal(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Pressing 'e' opens edit modal for existing task."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Edit currently focused task
            await pilot.press("e")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_task_view_modal(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Pressing 'v' opens view-only modal."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # View task details
            await pilot.press("v")
            # Multiple pauses to ensure modal is fully mounted
            await pilot.pause()
            await pilot.pause()
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_task_view_modal_with_acceptance_criteria(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """View modal displays acceptance criteria for task."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # First task (modal001) has acceptance criteria
            await pilot.press("v")
            # Multiple pauses to ensure modal is fully mounted
            await pilot.pause()
            await pilot.pause()
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    # =========================================================================
    # Confirm Modal Tests (x)
    # =========================================================================

    @pytest.mark.snapshot
    def test_confirm_delete_modal(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Pressing 'x' shows delete confirmation modal."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open delete confirmation
            await pilot.press("x")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_confirm_modal_cancel(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Confirm modal can be cancelled with 'n' or escape."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open delete confirmation
            await pilot.press("x")
            await pilot.pause()
            # Cancel with n
            await pilot.press("n")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    # =========================================================================
    # Diff Modal Tests (D)
    # =========================================================================

    @pytest.mark.snapshot
    def test_diff_modal_displayed(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Pressing 'D' on review task shows diff modal."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Navigate to REVIEW column (right twice from BACKLOG)
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()
            # Open diff modal
            await pilot.press("D")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_diff_modal_with_leader_key(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Leader key 'g d' also opens diff modal for review task."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Navigate to REVIEW column
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()
            # Use leader key sequence g d
            await pilot.press("g")
            await pilot.pause()
            await pilot.press("d")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    # =========================================================================
    # Auto Task Modal Tests (N)
    # =========================================================================

    @pytest.mark.snapshot
    def test_auto_task_create_modal(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Pressing 'N' (Shift+N) opens create modal with AUTO type preselected."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open create AUTO task modal
            await pilot.press("N")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    # =========================================================================
    # Modal Close Behavior Tests
    # =========================================================================

    @pytest.mark.snapshot
    def test_task_modal_escape_closes(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Escape closes task modal without saving."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open view modal
            await pilot.press("v")
            await pilot.pause()
            # Close with escape
            await pilot.press("escape")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_edit_modal_toggle(
        self,
        modal_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """View modal can toggle to edit mode with 'e'."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open view modal
            await pilot.press("v")
            await pilot.pause()
            # Toggle to edit mode
            await pilot.press("e")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(modal_app, terminal_size=(cols, rows), run_before=run_before)
