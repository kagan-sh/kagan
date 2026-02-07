"""Snapshot tests for modal screens.

These tests cover:
- AgentOutputModal streaming display
- ReviewModal initial state
- DiffModal with content

Note: Tests are synchronous because pytest-textual-snapshot's snap_compare
internally calls asyncio.run(), which conflicts with async test functions.
"""

from __future__ import annotations

import asyncio
import json
from datetime import datetime
from typing import TYPE_CHECKING, Any

import pytest

from kagan.adapters.db.repositories import TaskRepository
from kagan.adapters.db.schema import Task
from kagan.app import KaganApp
from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType
from tests.snapshots.helpers import wait_for_screen

if TYPE_CHECKING:
    from pathlib import Path
    from types import SimpleNamespace

    from textual.pilot import Pilot


# =============================================================================
# Helper Functions
# =============================================================================

SNAPSHOT_TIME = datetime(2024, 1, 1, 12, 0, 0)


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

    config_dir = tmp_path / "kagan-config"
    config_dir.mkdir()
    data_dir = tmp_path / "kagan-data"
    data_dir.mkdir()
    config_path = config_dir / "config.toml"
    config_path.write_text(config_content)

    return SimpleNamespace(
        root=project,
        db=str(data_dir / "kagan.db"),
        config=str(config_path),
    )


async def _create_auto_task(db_path: str, project_root: Path) -> str:
    """Create an AUTO task in BACKLOG with fixed ID for reproducible snapshots.

    Args:
        db_path: Path to the database file.
        project_root: Root path of the git repository.

    Returns:
        The task ID.
    """
    from kagan.adapters.db.repositories import RepoRepository

    manager = TaskRepository(db_path, project_root=project_root)
    await manager.initialize()

    # Explicitly create test project
    project_id = await manager.ensure_test_project("Auto Lifecycle Test Project")

    # Link repo to project so _startup_screen_decision finds it
    assert manager._session_factory is not None
    repo_repo = RepoRepository(manager._session_factory)
    repo, _ = await repo_repo.get_or_create(project_root, default_branch="main")
    if repo.id:
        await repo_repo.add_to_project(project_id, repo.id, is_primary=True)

    task = Task(
        id="auto0001",
        project_id=project_id,
        title="Implement user authentication",
        description="Add JWT-based authentication to the API endpoints.",
        priority=TaskPriority.HIGH,
        status=TaskStatus.BACKLOG,
        task_type=TaskType.AUTO,
        created_at=SNAPSHOT_TIME,
        updated_at=SNAPSHOT_TIME,
    )
    await manager.create(task)
    await manager.close()

    return task.id


class LifecycleMockAgentFactory:
    """Agent factory that simulates the full lifecycle with controllable responses.

    - Implementation prompt: returns <complete/>
    - Review prompt: returns <approve summary="LGTM"/>
    """

    def __init__(self, project_root: Path) -> None:
        self._project_root = project_root
        self._agents: list[Any] = []
        self._response_delay = 0.0
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

The implementation correctly addresses the task requirements:
- Code follows project conventions
- Tests cover the main functionality
- No obvious security issues

<approve summary="Implementation is correct and well-tested"/>
"""

    def set_response_delay(self, delay: float) -> None:
        """Set artificial delay before agent responses."""
        self._response_delay = max(delay, 0.0)

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
        if self._response_delay > 0:
            original_send = agent.send_prompt

            async def _delayed_send(prompt: str) -> str | None:
                await asyncio.sleep(self._response_delay)
                return await original_send(prompt)

            agent.send_prompt = _delayed_send

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
# Test Classes
# =============================================================================


class TestAgentOutputModal:
    """Snapshot tests for AgentOutputModal."""

    @pytest.fixture
    def auto_mode_project(
        self,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
    ) -> SimpleNamespace:
        """Create project with auto mode enabled and mock agent."""
        from types import SimpleNamespace as NS

        # Run async setup synchronously
        loop = asyncio.new_event_loop()
        try:
            project = loop.run_until_complete(
                _setup_auto_lifecycle_project(tmp_path, AUTO_MODE_CONFIG)
            )
            task_id = loop.run_until_complete(_create_auto_task(project.db, project.root))
        finally:
            loop.close()

        # Mock tmux
        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)

        # Create lifecycle mock factory
        mock_factory = LifecycleMockAgentFactory(project.root)

        return NS(
            root=project.root,
            db=project.db,
            config=project.config,
            task_id=task_id,
            mock_factory=mock_factory,
            sessions=sessions,
        )

    def _create_app(self, project: SimpleNamespace) -> KaganApp:
        """Create KaganApp with the project configuration."""
        return KaganApp(
            db_path=project.db,
            config_path=project.config,
            project_root=project.root,
            agent_factory=project.mock_factory,
        )

    @pytest.mark.snapshot
    def test_agent_output_modal_streaming(
        self,
        auto_mode_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """AgentOutputModal shows streaming agent output."""
        app = self._create_app(auto_mode_project)

        async def run_before(pilot: Pilot) -> None:
            from kagan.app import KaganApp
            from kagan.ui.modals.agent_output import AgentOutputModal
            from kagan.ui.screens.kanban import KanbanScreen
            from kagan.ui.widgets.streaming_output import StreamingOutput

            auto_mode_project.mock_factory.set_response_delay(3.0)
            await pilot.pause()
            from kagan.ui.screens.kanban import focus as kanban_focus

            screen = pilot.app.screen
            assert isinstance(screen, KanbanScreen)
            kanban_focus.focus_first_card(screen)
            await pilot.pause()
            # Start agent first
            await pilot.press("a")
            await pilot.pause()

            # Wait until agent is actually available (max 2 seconds)
            # This prevents "No agent running" message in watch modal
            kagan_app = pilot.app
            assert isinstance(kagan_app, KaganApp)
            max_wait = 10.0
            waited = 0.0
            agent = None
            while waited < max_wait:
                agent = kagan_app.ctx.automation_service.get_running_agent(
                    auto_mode_project.task_id
                )
                if agent is not None:
                    break
                await asyncio.sleep(0.05)
                waited += 0.05
            if agent is None:
                raise TimeoutError("Agent did not start in time")

            task = await kagan_app.ctx.task_service.get_task(auto_mode_project.task_id)
            if task is None:
                raise RuntimeError("Task not found for watch modal")
            iteration = kagan_app.ctx.automation_service.get_iteration_count(task.id)
            await pilot.app.push_screen(
                AgentOutputModal(
                    task=task,
                    agent=agent,
                    iteration=iteration,
                )
            )
            await wait_for_screen(pilot, AgentOutputModal, timeout=5.0)
            max_wait = 5.0
            waited = 0.0
            while waited < max_wait:
                await pilot.pause()
                output = pilot.app.screen.query_one("#agent-output", StreamingOutput)
                if list(output.children):
                    break
                await asyncio.sleep(0.1)
                waited += 0.1
            else:
                raise TimeoutError("Agent output did not mount")

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)


class TestReviewModal:
    """Snapshot tests for ReviewModal."""

    @pytest.fixture
    def review_project(
        self,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
    ) -> SimpleNamespace:
        """Create project with a task already in REVIEW status."""
        from types import SimpleNamespace as NS

        from kagan.adapters.db.repositories import RepoRepository

        # Run async setup synchronously
        loop = asyncio.new_event_loop()
        try:
            project = loop.run_until_complete(
                _setup_auto_lifecycle_project(tmp_path, AUTO_MODE_CONFIG)
            )
            # Create task directly in REVIEW status
            manager = TaskRepository(project.db, project_root=project.root)
            loop.run_until_complete(manager.initialize())

            # Explicitly create test project and link repo
            project_id = loop.run_until_complete(manager.ensure_test_project("Review Test Project"))

            # Link repo to project
            assert manager._session_factory is not None
            repo_repo = RepoRepository(manager._session_factory)
            repo, _ = loop.run_until_complete(
                repo_repo.get_or_create(project.root, default_branch="main")
            )
            if repo.id:
                loop.run_until_complete(
                    repo_repo.add_to_project(project_id, repo.id, is_primary=True)
                )

            task = Task(
                id="review01",
                project_id=project_id,
                title="Add user profile endpoint",
                description="Create GET /api/users/profile endpoint.",
                priority=TaskPriority.HIGH,
                status=TaskStatus.REVIEW,
                task_type=TaskType.AUTO,
                checks_passed=True,
                review_summary="Implementation is correct and well-tested",
                created_at=SNAPSHOT_TIME,
                updated_at=SNAPSHOT_TIME,
            )
            loop.run_until_complete(manager.create(task))

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
                manager.append_agent_log(task.id, "implementation", 1, impl_log)
            )

            review_log = json.dumps(
                {
                    "response_text": 'Approved. <approve summary="LGTM"/>',
                    "messages": [
                        {"type": "response", "content": "Changes look good."},
                    ],
                }
            )
            loop.run_until_complete(manager.append_agent_log(task.id, "review", 1, review_log))

            loop.run_until_complete(manager.close())
        finally:
            loop.close()

        # Mock tmux
        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)

        # Create mock factory
        mock_factory = LifecycleMockAgentFactory(project.root)

        return NS(
            root=project.root,
            db=project.db,
            config=project.config,
            task_id="review01",
            mock_factory=mock_factory,
            sessions=sessions,
        )

    def _create_app(self, project: SimpleNamespace) -> KaganApp:
        """Create KaganApp with the project configuration."""
        return KaganApp(
            db_path=project.db,
            config_path=project.config,
            project_root=project.root,
            agent_factory=project.mock_factory,
        )

    @pytest.mark.snapshot
    def test_review_modal_initial(
        self,
        review_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """ReviewModal opened via 'r' key on REVIEW task."""
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


class TestDiffModal:
    """Snapshot tests for DiffModal."""

    @pytest.fixture
    def review_project(
        self,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
    ) -> SimpleNamespace:
        """Create project with a task already in REVIEW status."""
        from types import SimpleNamespace as NS

        from kagan.adapters.db.repositories import RepoRepository

        # Run async setup synchronously
        loop = asyncio.new_event_loop()
        try:
            project = loop.run_until_complete(
                _setup_auto_lifecycle_project(tmp_path, AUTO_MODE_CONFIG)
            )
            # Create task directly in REVIEW status
            manager = TaskRepository(project.db, project_root=project.root)
            loop.run_until_complete(manager.initialize())

            # Explicitly create test project and link repo
            project_id = loop.run_until_complete(manager.ensure_test_project("Diff Test Project"))

            # Link repo to project
            assert manager._session_factory is not None
            repo_repo = RepoRepository(manager._session_factory)
            repo, _ = loop.run_until_complete(
                repo_repo.get_or_create(project.root, default_branch="main")
            )
            if repo.id:
                loop.run_until_complete(
                    repo_repo.add_to_project(project_id, repo.id, is_primary=True)
                )

            task = Task(
                id="review01",
                project_id=project_id,
                title="Add user profile endpoint",
                description="Create GET /api/users/profile endpoint.",
                priority=TaskPriority.HIGH,
                status=TaskStatus.REVIEW,
                task_type=TaskType.AUTO,
                checks_passed=True,
                review_summary="Implementation is correct and well-tested",
                created_at=SNAPSHOT_TIME,
                updated_at=SNAPSHOT_TIME,
            )
            loop.run_until_complete(manager.create(task))
            loop.run_until_complete(manager.close())
        finally:
            loop.close()

        # Mock tmux
        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)

        # Create mock factory
        mock_factory = LifecycleMockAgentFactory(project.root)

        return NS(
            root=project.root,
            db=project.db,
            config=project.config,
            task_id="review01",
            mock_factory=mock_factory,
            sessions=sessions,
        )

    def _create_app(self, project: SimpleNamespace) -> KaganApp:
        """Create KaganApp with the project configuration."""
        return KaganApp(
            db_path=project.db,
            config_path=project.config,
            project_root=project.root,
            agent_factory=project.mock_factory,
        )

    @pytest.mark.snapshot
    def test_diff_modal_with_content(
        self,
        review_project: SimpleNamespace,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """DiffModal opened via 'g d' leader sequence."""
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
