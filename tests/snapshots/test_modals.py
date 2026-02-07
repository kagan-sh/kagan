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
from syrupy.extensions.image import SVGImageSnapshotExtension

from kagan.adapters.db.repositories import TaskRepository
from kagan.adapters.db.schema import Task
from kagan.app import KaganApp
from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType
from tests.helpers.journey_runner import bundle_snapshots, execute_test_actions
from tests.helpers.mocks import create_fake_tmux
from tests.helpers.wait import _ci_timeout, wait_for_screen
from tests.snapshots.conftest import _normalize_svg

if TYPE_CHECKING:
    from pathlib import Path
    from types import SimpleNamespace


SNAPSHOT_TIME = datetime(2024, 1, 1, 12, 0, 0)


_create_fake_tmux = create_fake_tmux


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

    project_id = await manager.ensure_test_project("Auto Lifecycle Test Project")

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

        if read_only:
            agent.set_response(self._review_response)
        else:
            agent.set_response(self._implementation_response)

        self._agents.append(agent)
        return agent

    def get_all_agents(self) -> list[Any]:
        """Get all created agents."""
        return list(self._agents)


AUTO_MODE_CONFIG = """\
# Kagan Auto Lifecycle Test Configuration
[general]
auto_review = true
auto_approve = true
default_base_branch = "main"
default_worker_agent = "claude"
max_concurrent_agents = 1

[agents.claude]
identity = "claude.ai"
name = "Claude"
short_name = "claude"
run_command."*" = "echo mock-claude"
interactive_command."*" = "echo mock-claude-interactive"
active = true
"""


class TestAgentOutputModal:
    @pytest.fixture
    def auto_mode_project(
        self,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
    ) -> SimpleNamespace:
        """Create project with auto mode enabled and mock agent."""
        from types import SimpleNamespace as NS

        async def _seed_auto_mode() -> tuple[SimpleNamespace, str]:
            project = await _setup_auto_lifecycle_project(tmp_path, AUTO_MODE_CONFIG)
            task_id = await _create_auto_task(project.db, project.root)
            return project, task_id

        project, task_id = asyncio.run(_seed_auto_mode())

        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)

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
        snapshot: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """AgentOutputModal shows streaming agent output."""
        app = self._create_app(auto_mode_project)

        async def run_flow() -> dict[str, str]:
            from kagan.app import KaganApp
            from kagan.ui.modals.agent_output import AgentOutputModal
            from kagan.ui.screens.kanban import KanbanScreen
            from kagan.ui.widgets.streaming_output import StreamingOutput

            auto_mode_project.mock_factory.set_response_delay(1.0)
            cols, rows = snapshot_terminal_size
            async with app.run_test(headless=True, size=(cols, rows)) as pilot:
                await pilot.pause()
                from kagan.ui.screens.kanban import focus as kanban_focus

                screen = pilot.app.screen
                assert isinstance(screen, KanbanScreen)
                kanban_focus.focus_first_card(screen)
                await pilot.pause()

                await pilot.press("a")
                await pilot.pause()

                kagan_app = pilot.app
                assert isinstance(kagan_app, KaganApp)
                max_wait = _ci_timeout(10.0)
                waited = 0.0
                agent = None
                while waited < max_wait:
                    await pilot.pause()
                    agent = kagan_app.ctx.automation_service.get_running_agent(
                        auto_mode_project.task_id
                    )
                    if agent is not None:
                        break
                    await asyncio.sleep(0.05)
                    waited += 0.05
                    await pilot.pause()
                if agent is None:
                    raise TimeoutError("Agent did not start in time")

                task = await kagan_app.ctx.task_service.get_task(auto_mode_project.task_id)
                if task is None:
                    raise RuntimeError("Task not found for watch modal")
                run_count = kagan_app.ctx.automation_service.get_run_count(task.id)
                execution_id = kagan_app.ctx.automation_service.get_execution_id(task.id)
                await pilot.app.push_screen(
                    AgentOutputModal(
                        task=task,
                        agent=agent,
                        execution_id=execution_id,
                        run_count=run_count,
                    )
                )
                await wait_for_screen(pilot, AgentOutputModal, timeout=5.0)
                max_wait = _ci_timeout(5.0)
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

                return await execute_test_actions(pilot, ["shot(agent_output)"])

        snapshots = asyncio.run(run_flow())
        assert snapshots, "No snapshots captured for agent output modal"
        snapshot = snapshot.use_extension(SVGImageSnapshotExtension)
        svg = snapshots.get("agent_output")
        if svg is None:
            raise AssertionError("Missing agent_output snapshot")
        snapshot.assert_match(_normalize_svg(svg))


class TestReviewModal:
    @pytest.fixture
    def review_project(
        self,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
    ) -> SimpleNamespace:
        """Create project with a task already in REVIEW status."""
        from types import SimpleNamespace as NS

        from kagan.adapters.db.repositories import RepoRepository

        async def _seed_review_data() -> SimpleNamespace:
            project = await _setup_auto_lifecycle_project(tmp_path, AUTO_MODE_CONFIG)

            manager = TaskRepository(project.db, project_root=project.root)
            await manager.initialize()

            project_id = await manager.ensure_test_project("Review Test Project")

            assert manager._session_factory is not None
            repo_repo = RepoRepository(manager._session_factory)
            repo, _ = await repo_repo.get_or_create(project.root, default_branch="main")
            if repo.id:
                await repo_repo.add_to_project(project_id, repo.id, is_primary=True)

            task = Task(
                id="review01",
                project_id=project_id,
                title="Add user profile endpoint",
                description="Create GET /api/users/profile endpoint.",
                priority=TaskPriority.HIGH,
                status=TaskStatus.REVIEW,
                task_type=TaskType.AUTO,
                created_at=SNAPSHOT_TIME,
                updated_at=SNAPSHOT_TIME,
            )
            await manager.create(task)

            from kagan.adapters.db.schema import Workspace
            from kagan.core.models.enums import ExecutionRunReason, SessionType, WorkspaceStatus

            worktree_path = project.root / "worktrees" / task.id
            worktree_path.mkdir(parents=True, exist_ok=True)

            workspace = Workspace(
                project_id=project_id,
                task_id=task.id,
                branch_name="review/review01",
                path=str(worktree_path),
                status=WorkspaceStatus.ACTIVE,
                created_at=SNAPSHOT_TIME,
                updated_at=SNAPSHOT_TIME,
            )
            session_factory = manager._session_factory
            assert session_factory is not None

            async with session_factory() as session:
                session.add(workspace)
                await session.commit()
                await session.refresh(workspace)

            session_record = await manager.create_session_record(
                workspace_id=workspace.id,
                session_type=SessionType.ACP,
                external_id=None,
            )
            execution = await manager.create_execution(
                session_id=session_record.id,
                run_reason=ExecutionRunReason.CODINGAGENT,
                executor_action={},
            )

            impl_log = json.dumps(
                {
                    "response_text": "Done implementing. <complete/>",
                    "messages": [
                        {"type": "response", "content": "I've completed the implementation."},
                        {"type": "tool_call", "id": "tc-1", "title": "Write file", "kind": "edit"},
                    ],
                }
            )
            await manager.append_execution_log(execution.id, impl_log)

            review_log = json.dumps(
                {
                    "response_text": 'Approved. <approve summary="LGTM"/>',
                    "messages": [
                        {"type": "response", "content": "Changes look good."},
                    ],
                }
            )
            await manager.append_execution_log(execution.id, review_log)

            await manager.close()
            return project

        project = asyncio.run(_seed_review_data())

        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)

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
    def test_review_and_diff_journey(
        self,
        review_project: SimpleNamespace,
        snapshot: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Review and Diff modals opened from REVIEW column."""
        app = self._create_app(review_project)

        async def run_flow() -> dict[str, str]:
            from kagan.ui.modals.diff import DiffModal
            from kagan.ui.modals.review import ReviewModal

            cols, rows = snapshot_terminal_size
            async with app.run_test(headless=True, size=(cols, rows)) as pilot:
                await pilot.pause()

                await pilot.press("right")
                await pilot.pause()
                await pilot.press("right")
                await pilot.pause()

                await pilot.press("r")
                await wait_for_screen(pilot, ReviewModal, timeout=5.0)
                snapshots = await execute_test_actions(pilot, ["shot(review_modal)"])
                await pilot.press("escape")
                await pilot.pause()

                await pilot.press("D")
                await wait_for_screen(pilot, DiffModal, timeout=5.0)
                snapshots.update(await execute_test_actions(pilot, ["shot(diff_modal)"]))
                return snapshots

        snapshots = asyncio.run(run_flow())
        assert snapshots, "No snapshots captured for review/diff journey"
        bundle = bundle_snapshots(snapshots, normalizer=_normalize_svg)
        assert snapshot == bundle
