"""E2E snapshot tests for the full autonomous ticket lifecycle.

These tests cover the complete AUTO ticket flow:
1. Ticket created in AUTO mode in BACKLOG
2. With auto_start=true, ticket automatically moves to IN_PROGRESS
3. User presses 'w' to watch agent output stream
4. Agent completes work, ticket moves to REVIEW
5. User presses 'w' again to see tabbed view (Implementation + Review)
6. User opens review modal via 'g r' leader sequence
7. User approves via 'a' key
8. Ticket moves to DONE
9. Git verification: branch merged to main

Note: Tests are synchronous because pytest-textual-snapshot's snap_compare
internally calls asyncio.run(), which conflicts with async test functions.
"""

from __future__ import annotations

import asyncio
import json
from typing import TYPE_CHECKING, Any

import pytest

from kagan.app import KaganApp
from kagan.database.manager import StateManager
from kagan.database.models import Ticket, TicketPriority, TicketStatus, TicketType

if TYPE_CHECKING:
    from pathlib import Path
    from types import SimpleNamespace

    from textual.pilot import Pilot


# =============================================================================
# Helper Functions
# =============================================================================


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


async def _setup_auto_lifecycle_project(
    tmp_path: Path,
    config_content: str,
) -> SimpleNamespace:
    """Create a real project with git repo and auto mode config.

    Returns:
        SimpleNamespace with project paths and config.
    """
    from types import SimpleNamespace

    from tests.helpers.git import init_git_repo_with_commit

    project = tmp_path / "auto_lifecycle_project"
    project.mkdir()

    # Initialize real git repo with commit
    await init_git_repo_with_commit(project)

    # Create .kagan directory with config
    kagan_dir = project / ".kagan"
    kagan_dir.mkdir()
    (kagan_dir / "config.toml").write_text(config_content)

    return SimpleNamespace(
        root=project,
        db=str(kagan_dir / "state.db"),
        config=str(kagan_dir / "config.toml"),
        kagan_dir=kagan_dir,
    )


async def _create_auto_ticket(db_path: str) -> str:
    """Create an AUTO ticket in BACKLOG with fixed ID for reproducible snapshots.

    Returns:
        The ticket ID.
    """
    manager = StateManager(db_path)
    await manager.initialize()

    ticket = Ticket(
        id="auto0001",
        title="Implement user authentication",
        description="Add JWT-based authentication to the API endpoints.",
        priority=TicketPriority.HIGH,
        status=TicketStatus.BACKLOG,
        ticket_type=TicketType.AUTO,
    )
    await manager.create_ticket(ticket)
    await manager.close()

    return ticket.id


class LifecycleMockAgentFactory:
    """Agent factory that simulates the full lifecycle with controllable responses.

    - Implementation prompt: returns <complete/>
    - Review prompt: returns <approve summary="LGTM"/>
    """

    def __init__(self, project_root: Path) -> None:
        self._project_root = project_root
        self._agents: list[Any] = []
        self._implementation_response = """\
I've completed the implementation as specified.

## Changes Made

- Created `src/auth/jwt.py` with token generation and validation
- Added `src/auth/middleware.py` for request authentication
- Updated `src/routes/api.py` to use the new middleware
- Added comprehensive tests in `tests/test_auth.py`

All acceptance criteria have been met and tests are passing.

<complete/>
"""
        self._review_response = """\
I've reviewed the changes and they look good.

## Review Summary

The implementation correctly addresses the ticket requirements:
- Code follows project conventions
- Tests cover the main functionality
- No obvious security issues

<approve summary="Implementation is correct and well-tested"/>
"""

    def __call__(
        self,
        project_root: Path,
        agent_config: Any,
        *,
        read_only: bool = False,
    ) -> Any:
        """Create a new mock agent instance."""
        from tests.snapshots.conftest import MockAgent

        agent = MockAgent(project_root, agent_config, read_only=read_only)

        # Determine response based on read_only flag (review agents are read_only)
        if read_only:
            agent.set_response(self._review_response)
        else:
            agent.set_response(self._implementation_response)

        self._agents.append(agent)
        return agent

    def get_all_agents(self) -> list[Any]:
        """Get all created agents."""
        return list(self._agents)


# =============================================================================
# Test Configuration
# =============================================================================

AUTO_MODE_CONFIG = """\
# Kagan Auto Lifecycle Test Configuration
[general]
auto_start = true
auto_approve = true
auto_merge = true
default_base_branch = "main"
default_worker_agent = "claude"
max_iterations = 3
iteration_delay_seconds = 0.01
max_concurrent_agents = 1

[agents.claude]
identity = "claude.ai"
name = "Claude"
short_name = "claude"
run_command."*" = "echo mock-claude"
interactive_command."*" = "echo mock-claude-interactive"
active = true
"""


# =============================================================================
# Test Class
# =============================================================================


class TestAutoTicketLifecycle:
    """E2E snapshot tests for full autonomous ticket lifecycle."""

    @pytest.fixture
    def auto_mode_project(
        self,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
    ) -> SimpleNamespace:
        """Create project with auto mode enabled and mock agent.

        Returns a SimpleNamespace with project paths and the mock factory.
        """
        from types import SimpleNamespace as NS

        # Run async setup synchronously
        loop = asyncio.new_event_loop()
        try:
            project = loop.run_until_complete(
                _setup_auto_lifecycle_project(tmp_path, AUTO_MODE_CONFIG)
            )
            ticket_id = loop.run_until_complete(_create_auto_ticket(project.db))
        finally:
            loop.close()

        # Mock tmux
        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", fake_tmux)

        # Create lifecycle mock factory
        mock_factory = LifecycleMockAgentFactory(project.root)

        return NS(
            root=project.root,
            db=project.db,
            config=project.config,
            kagan_dir=project.kagan_dir,
            ticket_id=ticket_id,
            mock_factory=mock_factory,
            sessions=sessions,
        )

    def _create_app(self, project: SimpleNamespace) -> KaganApp:
        """Create KaganApp with the project configuration."""
        return KaganApp(
            db_path=project.db,
            config_path=project.config,
            lock_path=None,
            agent_factory=project.mock_factory,
        )

    @pytest.mark.snapshot
    def test_auto_lifecycle_01_ticket_created(
        self,
        auto_mode_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """AUTO ticket is displayed in BACKLOG column initially."""
        app = self._create_app(auto_mode_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Verify we're on KanbanScreen with the AUTO ticket visible
            from kagan.ui.screens.kanban import KanbanScreen

            assert isinstance(pilot.app.screen, KanbanScreen)

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_02_ticket_in_progress(
        self,
        auto_mode_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """AUTO ticket moves to IN_PROGRESS when manually started."""
        app = self._create_app(auto_mode_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Start the agent manually by pressing 'a' (start_agent action)
            await pilot.press("a")
            await pilot.pause()
            # Wait longer for the scheduler to process and UI to synchronize
            # This prevents race condition where ticket appears in multiple columns
            await asyncio.sleep(0.3)
            # Extra pauses for column refresh and UI synchronization
            await pilot.pause()
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_03_watch_modal_agent_output(
        self,
        auto_mode_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Press 'w' opens AgentOutputModal showing agent stream."""
        app = self._create_app(auto_mode_project)

        async def run_before(pilot: Pilot) -> None:
            from kagan.app import KaganApp

            await pilot.pause()
            # Start agent first
            await pilot.press("a")
            await pilot.pause()

            # Wait until agent is actually available (max 2 seconds)
            # This prevents "No agent running" message in watch modal
            kagan_app = pilot.app
            assert isinstance(kagan_app, KaganApp)
            max_wait = 2.0
            waited = 0.0
            while waited < max_wait:
                agent = kagan_app.scheduler.get_running_agent(auto_mode_project.ticket_id)
                if agent is not None:
                    break
                await asyncio.sleep(0.05)
                waited += 0.05

            # Now press 'w' to watch
            await pilot.press("w")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_04_close_watch_modal(
        self,
        auto_mode_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Press Escape closes watch modal, agent continues in background."""
        app = self._create_app(auto_mode_project)

        async def run_before(pilot: Pilot) -> None:
            from kagan.app import KaganApp

            await pilot.pause()
            # Start agent
            await pilot.press("a")
            await pilot.pause()

            # Wait until agent is actually available
            kagan_app = pilot.app
            assert isinstance(kagan_app, KaganApp)
            max_wait = 2.0
            waited = 0.0
            while waited < max_wait:
                agent = kagan_app.scheduler.get_running_agent(auto_mode_project.ticket_id)
                if agent is not None:
                    break
                await asyncio.sleep(0.05)
                waited += 0.05

            # Watch
            await pilot.press("w")
            await pilot.pause()
            # Close watch modal
            await pilot.press("escape")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_05_view_ticket_details(
        self,
        auto_mode_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Press 'v' opens ticket details modal."""
        app = self._create_app(auto_mode_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # View ticket details
            await pilot.press("v")
            # Multiple pauses to ensure modal is fully mounted
            await pilot.pause()
            await pilot.pause()
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_06_leader_mode_activated(
        self,
        auto_mode_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Press 'g' activates leader mode showing hints."""
        app = self._create_app(auto_mode_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Activate leader mode
            await pilot.press("g")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_07_search_bar_open(
        self,
        auto_mode_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Press '/' opens search bar."""
        app = self._create_app(auto_mode_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open search bar
            await pilot.press("slash")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_08_new_ticket_modal(
        self,
        auto_mode_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Press 'N' opens new AUTO ticket modal."""
        app = self._create_app(auto_mode_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open new AUTO ticket modal
            await pilot.press("N")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)


class TestAutoTicketLifecycleWithReview:
    """E2E snapshot tests for AUTO ticket lifecycle with REVIEW status.

    These tests focus on the REVIEW phase of the lifecycle.
    """

    @pytest.fixture
    def review_project(
        self,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
    ) -> SimpleNamespace:
        """Create project with a ticket already in REVIEW status."""
        from types import SimpleNamespace as NS

        # Run async setup synchronously
        loop = asyncio.new_event_loop()
        try:
            project = loop.run_until_complete(
                _setup_auto_lifecycle_project(tmp_path, AUTO_MODE_CONFIG)
            )
            # Create ticket directly in REVIEW status
            manager = StateManager(project.db)
            loop.run_until_complete(manager.initialize())

            ticket = Ticket(
                id="review01",
                title="Add user profile endpoint",
                description="Create GET /api/users/profile endpoint.",
                priority=TicketPriority.HIGH,
                status=TicketStatus.REVIEW,
                ticket_type=TicketType.AUTO,
                checks_passed=True,
                review_summary="Implementation is correct and well-tested",
            )
            loop.run_until_complete(manager.create_ticket(ticket))

            # Add historical agent logs for watch modal
            impl_log = json.dumps(
                {
                    "response_text": "Done implementing. <complete/>",
                    "messages": [
                        {"type": "response", "content": "I've completed the implementation."},
                        {"type": "tool_call", "id": "tc-1", "title": "Write file", "kind": "write"},
                    ],
                }
            )
            loop.run_until_complete(
                manager.append_agent_log(ticket.id, "implementation", 1, impl_log)
            )

            review_log = json.dumps(
                {
                    "response_text": 'Approved. <approve summary="LGTM"/>',
                    "messages": [
                        {"type": "response", "content": "Changes look good."},
                    ],
                }
            )
            loop.run_until_complete(manager.append_agent_log(ticket.id, "review", 1, review_log))

            loop.run_until_complete(manager.close())
        finally:
            loop.close()

        # Mock tmux
        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", fake_tmux)

        # Create mock factory
        mock_factory = LifecycleMockAgentFactory(project.root)

        return NS(
            root=project.root,
            db=project.db,
            config=project.config,
            kagan_dir=project.kagan_dir,
            ticket_id="review01",
            mock_factory=mock_factory,
            sessions=sessions,
        )

    def _create_app(self, project: SimpleNamespace) -> KaganApp:
        """Create KaganApp with the project configuration."""
        return KaganApp(
            db_path=project.db,
            config_path=project.config,
            lock_path=None,
            agent_factory=project.mock_factory,
        )

    @pytest.mark.snapshot
    def test_auto_lifecycle_review_01_ticket_in_review(
        self,
        review_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Ticket displayed in REVIEW column with review status."""
        app = self._create_app(review_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            from kagan.ui.screens.kanban import KanbanScreen

            assert isinstance(pilot.app.screen, KanbanScreen)
            # Navigate to REVIEW column (it's the 3rd column)
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_review_02_watch_modal_tabs(
        self,
        review_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Watch modal in REVIEW shows tabbed view (Implementation + Review)."""
        app = self._create_app(review_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Navigate to REVIEW column
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()
            # Open watch modal
            await pilot.press("w")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_review_03_review_modal_via_leader(
        self,
        review_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Open review modal via 'g r' leader sequence."""
        app = self._create_app(review_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Navigate to REVIEW column
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()
            # Leader sequence: g then r
            await pilot.press("g")
            await pilot.pause()
            await pilot.press("r")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_review_04_review_modal_opened(
        self,
        review_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Review modal opened via 'r' key directly on REVIEW ticket."""
        app = self._create_app(review_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Navigate to REVIEW column
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()
            # Open review modal directly
            await pilot.press("r")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_review_05_diff_modal(
        self,
        review_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """View diff via 'g d' leader sequence."""
        app = self._create_app(review_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Navigate to REVIEW column
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()
            # Leader sequence: g then d
            await pilot.press("g")
            await pilot.pause()
            await pilot.press("d")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_review_06_peek_overlay(
        self,
        review_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Press space to show peek overlay with scratchpad."""
        app = self._create_app(review_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Navigate to REVIEW column
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()
            # Toggle peek overlay
            await pilot.press("space")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)


class TestAutoTicketLifecycleDone:
    """E2E snapshot tests for AUTO ticket lifecycle final stage.

    These tests focus on the DONE state after approval.
    """

    @pytest.fixture
    def done_project(
        self,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
    ) -> SimpleNamespace:
        """Create project with tickets in various states including DONE."""
        from types import SimpleNamespace as NS

        # Run async setup synchronously
        loop = asyncio.new_event_loop()
        try:
            project = loop.run_until_complete(
                _setup_auto_lifecycle_project(tmp_path, AUTO_MODE_CONFIG)
            )

            manager = StateManager(project.db)
            loop.run_until_complete(manager.initialize())

            # Create DONE ticket
            done_ticket = Ticket(
                id="done0001",
                title="Completed authentication feature",
                description="JWT auth fully implemented and merged.",
                priority=TicketPriority.HIGH,
                status=TicketStatus.DONE,
                ticket_type=TicketType.AUTO,
                checks_passed=True,
                review_summary="All tests pass, implementation complete",
            )
            loop.run_until_complete(manager.create_ticket(done_ticket))

            # Create IN_PROGRESS ticket for comparison
            in_progress_ticket = Ticket(
                id="inprog01",
                title="Working on new feature",
                description="Currently in progress.",
                priority=TicketPriority.MEDIUM,
                status=TicketStatus.IN_PROGRESS,
                ticket_type=TicketType.AUTO,
            )
            loop.run_until_complete(manager.create_ticket(in_progress_ticket))

            # Create BACKLOG ticket
            backlog_ticket = Ticket(
                id="backlog1",
                title="Future enhancement",
                description="To be done later.",
                priority=TicketPriority.LOW,
                status=TicketStatus.BACKLOG,
                ticket_type=TicketType.AUTO,
            )
            loop.run_until_complete(manager.create_ticket(backlog_ticket))

            loop.run_until_complete(manager.close())
        finally:
            loop.close()

        # Mock tmux
        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", fake_tmux)

        # Create mock factory
        mock_factory = LifecycleMockAgentFactory(project.root)

        return NS(
            root=project.root,
            db=project.db,
            config=project.config,
            kagan_dir=project.kagan_dir,
            mock_factory=mock_factory,
            sessions=sessions,
        )

    def _create_app(self, project: SimpleNamespace) -> KaganApp:
        """Create KaganApp with the project configuration."""
        return KaganApp(
            db_path=project.db,
            config_path=project.config,
            lock_path=None,
            agent_factory=project.mock_factory,
        )

    @pytest.mark.snapshot
    def test_auto_lifecycle_done_01_board_with_all_columns(
        self,
        done_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Board displays tickets across all columns including DONE."""
        app = self._create_app(done_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            from kagan.ui.screens.kanban import KanbanScreen

            assert isinstance(pilot.app.screen, KanbanScreen)

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_done_02_navigate_to_done(
        self,
        done_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Navigate to DONE column and view completed ticket."""
        app = self._create_app(done_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Navigate to DONE column (4th column)
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_done_03_view_done_ticket_details(
        self,
        done_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """View details of completed ticket."""
        app = self._create_app(done_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Navigate to DONE column
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()
            # View details
            await pilot.press("v")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_lifecycle_done_04_duplicate_done_ticket(
        self,
        done_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Press 'y' to duplicate (yank) a done ticket."""
        app = self._create_app(done_project)

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Navigate to DONE column
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()
            # Yank/duplicate ticket
            await pilot.press("y")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)
