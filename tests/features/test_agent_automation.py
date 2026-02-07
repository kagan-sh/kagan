"""Feature tests for Agent and Automation.

Tests organized by user-facing features, not implementation layers.
Each test validates a complete user journey or critical behavior.

Covers:
- Agent spawn limits and PAIR safeguards
- Agent stopping
- Execution runs (blocked handling, log persistence)
- Signal parsing (blocked/reject/default continue)
- AutomationService queue management
- Session management (PAIR tasks)
"""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock

from tests.helpers.mocks import create_mock_workspace_service, create_test_config

from kagan.agents.signals import Signal, parse_signal
from kagan.bootstrap import InMemoryEventBus
from kagan.core.models.enums import ExecutionKind, TaskStatus, TaskType
from kagan.paths import get_worktree_base_dir
from kagan.services.automation import AutomationService, RunningTaskState
from kagan.services.executions import ExecutionServiceImpl
from kagan.services.queued_messages import QueuedMessageServiceImpl
from kagan.services.tasks import TaskService

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.adapters.db.repositories import TaskRepository


def build_automation(
    state_manager: TaskRepository,
    workspace_service,
    config,
    *,
    agent_factory=None,
    session_service=None,
    queued_message_service=None,
) -> AutomationService:
    """Helper to build AutomationService with a fresh event bus."""
    event_bus = InMemoryEventBus()
    task_service = TaskService(state_manager, event_bus)
    execution_service = ExecutionServiceImpl(state_manager)
    if agent_factory is None:
        return AutomationService(
            task_service,
            workspace_service,
            config,
            session_service=session_service,
            execution_service=execution_service,
            event_bus=event_bus,
            queued_message_service=queued_message_service,
        )
    return AutomationService(
        task_service,
        workspace_service,
        config,
        session_service=session_service,
        execution_service=execution_service,
        agent_factory=agent_factory,
        event_bus=event_bus,
        queued_message_service=queued_message_service,
    )


class TestSignalParsing:
    """Agent signals are correctly parsed from output."""

    def test_parse_blocked_signal_with_reason(self):
        """<blocked reason="..."/> signal extracts reason."""
        output = 'Cannot proceed. <blocked reason="Missing API key configuration"/>'
        result = parse_signal(output)

        assert result.signal == Signal.BLOCKED
        assert result.reason == "Missing API key configuration"

    def test_parse_reject_signal_with_reason(self):
        """<reject reason="..."/> extracts rejection reason."""
        output = '<reject reason="Missing error handling in critical path"/>'
        result = parse_signal(output)

        assert result.signal == Signal.REJECT
        assert result.reason == "Missing error handling in critical path"

    def test_no_signal_defaults_to_continue(self):
        """Output without signal defaults to CONTINUE."""
        output = "Just some text without any signal"
        result = parse_signal(output)

        assert result.signal == Signal.CONTINUE


class TestQueueLanes:
    """Queue lanes remain isolated per task."""

    async def test_implementation_review_and_planner_queues_are_independent(self) -> None:
        queue = QueuedMessageServiceImpl()
        await queue.queue_message("task-1", "impl-msg", lane="implementation")
        await queue.queue_message("task-1", "review-msg", lane="review")
        await queue.queue_message("task-1", "planner-msg", lane="planner")

        impl = await queue.take_queued("task-1", lane="implementation")
        assert impl is not None
        assert impl.content == "impl-msg"

        review_status = await queue.get_status("task-1", lane="review")
        assert review_status.has_queued is True

        review = await queue.take_queued("task-1", lane="review")
        assert review is not None
        assert review.content == "review-msg"

        planner = await queue.take_queued("task-1", lane="planner")
        assert planner is not None
        assert planner.content == "planner-msg"


class TestAgentSpawning:
    """Agent spawn behaviors not covered by UI snapshots."""

    async def test_pair_task_not_auto_spawned(
        self, state_manager: TaskRepository, task_factory, git_repo: Path
    ):
        """PAIR tasks don't auto-spawn agents when moved to IN_PROGRESS."""
        task = task_factory(
            title="Pair task",
            status=TaskStatus.BACKLOG,
            task_type=TaskType.PAIR,
        )
        await state_manager.create(task)

        config = create_test_config()
        worktrees = create_mock_workspace_service()
        mock_factory = MagicMock()
        scheduler = build_automation(
            state_manager,
            worktrees,
            config,
            agent_factory=mock_factory,
        )
        await scheduler.start()

        await scheduler.handle_status_change(task.id, TaskStatus.BACKLOG, TaskStatus.IN_PROGRESS)
        await asyncio.sleep(0.1)

        mock_factory.assert_not_called()
        await scheduler.stop()


class TestAgentStopping:
    """Agents can be stopped manually or via status changes."""

    async def test_stop_running_agent(self, state_manager: TaskRepository):
        """stop_task stops a running agent."""
        config = create_test_config()
        worktrees = create_mock_workspace_service()

        scheduler = build_automation(state_manager, worktrees, config)
        await scheduler.start()

        mock_agent = MagicMock()
        mock_agent.stop = AsyncMock()
        state = RunningTaskState(agent=mock_agent)
        scheduler._running["test-task"] = state

        result = await scheduler.stop_task("test-task")

        assert result is True
        await scheduler.stop()

    async def test_stop_nonexistent_returns_false(self, state_manager: TaskRepository):
        """Stopping non-running task returns False."""
        config = create_test_config()
        worktrees = create_mock_workspace_service()

        scheduler = build_automation(state_manager, worktrees, config)

        result = await scheduler.stop_task("nonexistent")

        assert result is False

    async def test_moving_out_of_in_progress_stops_agent(self, state_manager: TaskRepository):
        """Moving task out of IN_PROGRESS (not to REVIEW) stops agent."""
        config = create_test_config()
        worktrees = create_mock_workspace_service()

        scheduler = build_automation(state_manager, worktrees, config)
        await scheduler.start()

        mock_agent = MagicMock()
        mock_agent.stop = AsyncMock()
        mock_task = MagicMock()
        mock_task.done = MagicMock(return_value=True)
        state = RunningTaskState(agent=mock_agent, task=mock_task)
        scheduler._running["test-task"] = state

        await scheduler.handle_status_change(
            "test-task", TaskStatus.IN_PROGRESS, TaskStatus.BACKLOG
        )
        await asyncio.sleep(0.1)

        assert "test-task" not in scheduler._running
        await scheduler.stop()


class TestExecutionRuns:
    """Agent runs and persists execution output."""

    async def test_blocked_signal_moves_to_backlog(
        self, state_manager: TaskRepository, task_factory, git_repo: Path
    ):
        """BLOCKED signal moves task to BACKLOG with reason."""
        task = await state_manager.create(
            task_factory(
                title="Will block",
                status=TaskStatus.IN_PROGRESS,
                task_type=TaskType.AUTO,
            )
        )

        config = create_test_config()
        worktrees = create_mock_workspace_service()
        await worktrees.create(task.id)
        assert state_manager._session_factory is not None
        from kagan.adapters.db.schema import Workspace
        from kagan.core.models.enums import WorkspaceStatus

        async with state_manager._session_factory() as session:
            workspace = Workspace(
                project_id=task.project_id,
                task_id=task.id,
                branch_name="automation/test",
                path="/tmp/worktree",
                status=WorkspaceStatus.ACTIVE,
            )
            session.add(workspace)
            await session.commit()
            await session.refresh(workspace)
        worktrees.list_workspaces.return_value = [workspace]

        def blocked_factory(project_root, agent_config, **kwargs):
            from kagan.acp.buffers import AgentBuffers

            mock = MagicMock()
            buffers = AgentBuffers()
            buffers.append_response('<blocked reason="Missing API key"/>')
            mock.set_auto_approve = MagicMock()
            mock.set_model_override = MagicMock()
            mock.start = MagicMock()
            mock.wait_ready = AsyncMock()
            mock.send_prompt = AsyncMock()
            mock.get_response_text = MagicMock(side_effect=buffers.get_response_text)
            mock.clear_tool_calls = MagicMock()
            mock.stop = AsyncMock()
            mock._buffers = buffers
            return mock

        scheduler = build_automation(
            state_manager,
            worktrees,
            config,
            agent_factory=blocked_factory,
        )

        await scheduler._run_task_loop(task)

        fetched = await state_manager.get(task.id)
        assert fetched is not None
        assert fetched.status == TaskStatus.BACKLOG
        scratchpad = await state_manager.get_scratchpad(task.id)
        assert "Missing API key" in scratchpad

    async def test_execution_logs_and_turns_saved(
        self, state_manager: TaskRepository, task_factory, git_repo: Path, mock_agent_factory
    ):
        """Execution logs and agent turns are persisted."""
        task = await state_manager.create(
            task_factory(
                title="Track execution output",
                status=TaskStatus.IN_PROGRESS,
                task_type=TaskType.AUTO,
            )
        )

        config = create_test_config()
        worktrees = create_mock_workspace_service()
        await worktrees.create(task.id)
        assert state_manager._session_factory is not None
        from kagan.adapters.db.schema import Workspace
        from kagan.core.models.enums import WorkspaceStatus

        async with state_manager._session_factory() as session:
            workspace = Workspace(
                project_id=task.project_id,
                task_id=task.id,
                branch_name="automation/test",
                path="/tmp/worktree",
                status=WorkspaceStatus.ACTIVE,
            )
            session.add(workspace)
            await session.commit()
            await session.refresh(workspace)
        worktrees.list_workspaces.return_value = [workspace]

        scheduler = build_automation(
            state_manager,
            worktrees,
            config,
            agent_factory=mock_agent_factory,
        )

        await scheduler._run_task_loop(task)

        fetched = await state_manager.get(task.id)
        assert fetched is not None
        execution = await state_manager.get_latest_execution_for_task(task.id)
        assert execution is not None
        logs = await state_manager.get_execution_logs(execution.id)
        assert logs is not None
        assert logs.logs
        turns = await state_manager.list_agent_turns(execution.id)
        assert turns

    async def test_queued_messages_restart_before_review_transition(
        self, state_manager: TaskRepository, task_factory, git_repo: Path, mock_agent_factory
    ) -> None:
        """Queued implementation follow-ups re-spawn without transitioning to REVIEW."""
        task = await state_manager.create(
            task_factory(
                title="Queue follow-up",
                status=TaskStatus.IN_PROGRESS,
                task_type=TaskType.AUTO,
            )
        )

        config = create_test_config()
        worktrees = create_mock_workspace_service()
        await worktrees.create(task.id)
        assert state_manager._session_factory is not None
        from kagan.adapters.db.schema import Workspace
        from kagan.core.models.enums import WorkspaceStatus

        async with state_manager._session_factory() as session:
            workspace = Workspace(
                project_id=task.project_id,
                task_id=task.id,
                branch_name="automation/test",
                path="/tmp/worktree",
                status=WorkspaceStatus.ACTIVE,
            )
            session.add(workspace)
            await session.commit()
            await session.refresh(workspace)
        worktrees.list_workspaces.return_value = [workspace]

        queued = QueuedMessageServiceImpl()
        await queued.queue_message(
            task.id,
            "Continue with queue feedback",
            lane="implementation",
        )

        scheduler = build_automation(
            state_manager,
            worktrees,
            config,
            agent_factory=mock_agent_factory,
            queued_message_service=queued,
        )
        handle_complete = AsyncMock()
        scheduler._handle_complete = handle_complete  # type: ignore[method-assign]

        await scheduler._run_task_loop(task)

        handle_complete.assert_not_awaited()
        fetched = await state_manager.get(task.id)
        assert fetched is not None
        assert fetched.status == TaskStatus.IN_PROGRESS
        scratchpad = await state_manager.get_scratchpad(task.id)
        assert "Continue with queue feedback" in scratchpad
        assert not scheduler._event_queue.empty()
        queued_event = scheduler._event_queue.get_nowait()
        assert queued_event.kind == ExecutionKind.SPAWN

        status = await queued.get_status(task.id, lane="implementation")
        assert status.has_queued is False


class TestSessionManagement:
    """PAIR tasks can open and manage tmux sessions."""

    async def test_create_session_for_pair_task(
        self,
        state_manager: TaskRepository,
        task_factory,
        task_service,
        mock_workspace_service,
        git_repo: Path,
        mock_tmux,
    ):
        """Creating session for PAIR task creates tmux session."""
        from kagan.services.sessions import SessionService

        task = await state_manager.create(
            task_factory(
                title="Pair work",
                status=TaskStatus.BACKLOG,
                task_type=TaskType.PAIR,
            )
        )

        config = create_test_config()
        worktree_path = get_worktree_base_dir() / "worktrees" / task.id
        worktree_path.mkdir(parents=True)
        assert state_manager._session_factory is not None
        from kagan.adapters.db.schema import Workspace
        from kagan.core.models.enums import WorkspaceStatus

        async with state_manager._session_factory() as session:
            workspace = Workspace(
                project_id=task.project_id,
                task_id=task.id,
                branch_name="pair/test",
                path=str(worktree_path),
                status=WorkspaceStatus.ACTIVE,
            )
            session.add(workspace)
            await session.commit()
            await session.refresh(workspace)
        mock_workspace_service.list_workspaces.return_value = [workspace]

        session_mgr = SessionService(git_repo, task_service, mock_workspace_service, config)
        session_name = await session_mgr.create_session(task, worktree_path)

        assert session_name == f"kagan-{task.id}"
        assert f"kagan-{task.id}" in mock_tmux
