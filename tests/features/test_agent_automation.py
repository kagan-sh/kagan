"""Feature tests for Agent and Automation.

Tests organized by user-facing features, not implementation layers.
Each test validates a complete user journey or critical behavior.

Covers:
- Agent spawn limits and PAIR safeguards
- Agent stopping
- Iteration loop (blocked/max iterations)
- Signal parsing (blocked/reject/default continue)
- AutomationServiceImpl queue management
- Session management (PAIR tasks)
"""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock

from tests.helpers.mocks import create_mock_workspace_service, create_test_config

from kagan.agents.signals import Signal, parse_signal
from kagan.bootstrap import InMemoryEventBus
from kagan.core.models.enums import TaskStatus, TaskType
from kagan.paths import get_worktree_base_dir
from kagan.services.automation import AutomationServiceImpl, RunningTaskState
from kagan.services.tasks import TaskServiceImpl

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
) -> AutomationServiceImpl:
    """Helper to build AutomationServiceImpl with a fresh event bus."""
    event_bus = InMemoryEventBus()
    task_service = TaskServiceImpl(state_manager, event_bus)
    if agent_factory is None:
        return AutomationServiceImpl(
            task_service,
            workspace_service,
            config,
            session_service=session_service,
            event_bus=event_bus,
        )
    return AutomationServiceImpl(
        task_service,
        workspace_service,
        config,
        session_service=session_service,
        agent_factory=agent_factory,
        event_bus=event_bus,
    )


# =============================================================================
# Feature: Signal Parsing
# =============================================================================


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


# =============================================================================
# Feature: Agent Spawning
# =============================================================================


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

        # PAIR tasks should not trigger agent factory
        mock_factory.assert_not_called()
        await scheduler.stop()

    async def test_spawn_respects_max_concurrent_agents(
        self, state_manager: TaskRepository, task_factory, git_repo: Path
    ):
        """AutomationServiceImpl respects max_concurrent_agents limit."""
        config = create_test_config(max_concurrent=1)
        worktrees = create_mock_workspace_service()

        scheduler = build_automation(state_manager, worktrees, config)

        # Manually add a running task to simulate at capacity
        scheduler._running["existing-task"] = RunningTaskState()

        task = task_factory(
            title="Should wait",
            status=TaskStatus.IN_PROGRESS,
            task_type=TaskType.AUTO,
        )
        await state_manager.create(task)

        # At capacity - should not spawn
        await scheduler.start()
        await scheduler._process_event(task.id, None, TaskStatus.IN_PROGRESS)

        # New task should not be running (at capacity)
        assert task.id not in scheduler._running


# =============================================================================
# Feature: Agent Stopping
# =============================================================================


class TestAgentStopping:
    """Agents can be stopped manually or via status changes."""

    async def test_stop_running_agent(self, state_manager: TaskRepository):
        """stop_task stops a running agent."""
        config = create_test_config()
        worktrees = create_mock_workspace_service()

        scheduler = build_automation(state_manager, worktrees, config)
        await scheduler.start()

        # Simulate running task
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

        # Add running task
        mock_agent = MagicMock()
        mock_agent.stop = AsyncMock()
        mock_task = MagicMock()
        mock_task.done = MagicMock(return_value=True)
        state = RunningTaskState(agent=mock_agent, task=mock_task)
        scheduler._running["test-task"] = state

        # Move to BACKLOG should stop
        await scheduler._process_event("test-task", TaskStatus.IN_PROGRESS, TaskStatus.BACKLOG)

        assert "test-task" not in scheduler._running
        await scheduler.stop()


# =============================================================================
# Feature: Iteration Loop
# =============================================================================


class TestIterationLoop:
    """Agent runs iterations until complete/blocked/max."""

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

        config = create_test_config(max_iterations=3)
        worktrees = create_mock_workspace_service()
        await worktrees.create(task.id)

        # Create mock factory that returns blocked signal
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
        assert fetched.block_reason == "Missing API key"

    async def test_max_iterations_moves_to_backlog(
        self, state_manager: TaskRepository, task_factory, git_repo: Path
    ):
        """Reaching max iterations moves task to BACKLOG."""
        task = await state_manager.create(
            task_factory(
                title="Will timeout",
                status=TaskStatus.IN_PROGRESS,
                task_type=TaskType.AUTO,
            )
        )

        config = create_test_config(max_iterations=2)
        worktrees = create_mock_workspace_service()
        await worktrees.create(task.id)

        # Create mock that always returns continue
        def continue_factory(project_root, agent_config, **kwargs):
            from kagan.acp.buffers import AgentBuffers

            mock = MagicMock()
            buffers = AgentBuffers()
            buffers.append_response("Still working... <continue/>")
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
            agent_factory=continue_factory,
        )

        await scheduler._run_task_loop(task)

        fetched = await state_manager.get(task.id)
        assert fetched is not None
        assert fetched.status == TaskStatus.BACKLOG

    async def test_iteration_count_tracked(
        self, state_manager: TaskRepository, task_factory, git_repo: Path, mock_agent_factory
    ):
        """Iteration count is incremented and persisted."""
        task = await state_manager.create(
            task_factory(
                title="Track iterations",
                status=TaskStatus.IN_PROGRESS,
                task_type=TaskType.AUTO,
            )
        )

        config = create_test_config(max_iterations=3)
        worktrees = create_mock_workspace_service()
        await worktrees.create(task.id)

        scheduler = build_automation(
            state_manager,
            worktrees,
            config,
            agent_factory=mock_agent_factory,
        )

        await scheduler._run_task_loop(task)

        fetched = await state_manager.get(task.id)
        assert fetched is not None
        # Should have run at least 1 iteration before completing
        assert fetched.total_iterations >= 1


# =============================================================================
# Feature: AutomationServiceImpl Queue Management
# =============================================================================


class TestAutomationServiceImplQueue:
    """AutomationServiceImpl processes events sequentially to prevent races."""

    async def test_events_processed_in_order(self, state_manager: TaskRepository):
        """Events are processed in FIFO order."""
        config = create_test_config()
        worktrees = create_mock_workspace_service()

        processed_events: list[str] = []

        scheduler = build_automation(state_manager, worktrees, config)

        # Override process_event to track order
        async def tracking_process(task_id, old_status, new_status):
            processed_events.append(task_id)
            # Don't actually process to avoid side effects

        scheduler._process_event = tracking_process
        await scheduler.start()

        # Queue multiple events
        await scheduler.handle_status_change("task-1", None, TaskStatus.IN_PROGRESS)
        await scheduler.handle_status_change("task-2", None, TaskStatus.IN_PROGRESS)
        await scheduler.handle_status_change("task-3", None, TaskStatus.IN_PROGRESS)

        await asyncio.sleep(0.1)

        assert processed_events == ["task-1", "task-2", "task-3"]
        await scheduler.stop()

    async def test_scheduler_start_idempotent(self, state_manager: TaskRepository):
        """Starting scheduler multiple times is safe."""
        config = create_test_config()
        worktrees = create_mock_workspace_service()

        scheduler = build_automation(state_manager, worktrees, config)

        await scheduler.start()
        await scheduler.start()  # Should not crash or create duplicate workers

        assert scheduler._started is True
        await scheduler.stop()


# =============================================================================
# Feature: Session Management (PAIR Tasks)
# =============================================================================


class TestSessionManagement:
    """PAIR tasks can open and manage tmux sessions."""

    async def test_create_session_for_pair_task(
        self, state_manager: TaskRepository, task_factory, task_service, git_repo: Path, mock_tmux
    ):
        """Creating session for PAIR task creates tmux session."""
        from kagan.services.sessions import SessionServiceImpl

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

        session_mgr = SessionServiceImpl(git_repo, task_service, config)
        session_name = await session_mgr.create_session(task, worktree_path)

        assert session_name == f"kagan-{task.id}"
        assert f"kagan-{task.id}" in mock_tmux

    async def test_session_exists_check(self, task_service, git_repo: Path, mock_tmux):
        """session_exists correctly reports session state."""
        from kagan.services.sessions import SessionServiceImpl

        config = create_test_config()
        session_mgr = SessionServiceImpl(git_repo, task_service, config)

        # Session doesn't exist yet
        exists = await session_mgr.session_exists("nonexistent")
        assert exists is False

        # Create a session via mock
        mock_tmux["kagan-test-123"] = {"cwd": "", "env": {}}

        exists = await session_mgr.session_exists("test-123")
        assert exists is True

    async def test_kill_session_removes_and_marks_inactive(
        self, state_manager: TaskRepository, task_factory, task_service, git_repo: Path, mock_tmux
    ):
        """Killing session removes tmux session and marks inactive."""
        from kagan.services.sessions import SessionServiceImpl

        task = await state_manager.create(
            task_factory(
                title="To kill",
                status=TaskStatus.IN_PROGRESS,
                task_type=TaskType.PAIR,
                session_active=True,
            )
        )

        config = create_test_config()
        mock_tmux[f"kagan-{task.id}"] = {"cwd": "", "env": {}}

        session_mgr = SessionServiceImpl(git_repo, task_service, config)
        await session_mgr.kill_session(task.id)

        fetched = await state_manager.get(task.id)
        assert fetched is not None
        assert fetched.session_active is False


# =============================================================================
# Feature: Agent Blocked Handling
# =============================================================================


class TestAgentBlocked:
    """Blocked agents are handled correctly."""

    async def test_blocked_stores_reason_in_scratchpad(
        self, state_manager: TaskRepository, task_factory, git_repo: Path
    ):
        """Blocked reason is appended to scratchpad."""
        task = await state_manager.create(
            task_factory(
                title="Will block",
                status=TaskStatus.IN_PROGRESS,
                task_type=TaskType.AUTO,
            )
        )
        await state_manager.update_scratchpad(task.id, "Initial notes")

        config = create_test_config()
        worktrees = create_mock_workspace_service()
        await worktrees.create(task.id)

        # Create mock that returns blocked
        def blocked_factory(project_root, agent_config, **kwargs):
            from kagan.acp.buffers import AgentBuffers

            mock = MagicMock()
            buffers = AgentBuffers()
            buffers.append_response('<blocked reason="Cannot access database"/>')
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

        scratchpad = await state_manager.get_scratchpad(task.id)
        assert "BLOCKED" in scratchpad
        assert "Cannot access database" in scratchpad


# =============================================================================
# Feature: Initialize Existing Tasks
# =============================================================================


class TestInitializeExisting:
    """AutomationServiceImpl initializes existing IN_PROGRESS AUTO tasks on startup."""

    async def test_existing_auto_tasks_queued_on_init(
        self, state_manager: TaskRepository, task_factory, git_repo: Path
    ):
        """Existing IN_PROGRESS AUTO tasks are queued for processing."""
        # Create existing task before scheduler starts
        task = await state_manager.create(
            task_factory(
                title="Already in progress",
                status=TaskStatus.IN_PROGRESS,
                task_type=TaskType.AUTO,
            )
        )

        config = create_test_config(auto_start=True)
        worktrees = create_mock_workspace_service()

        scheduler = build_automation(state_manager, worktrees, config)

        await scheduler.initialize_existing_tasks()

        # Verify event was queued
        event = scheduler._event_queue.get_nowait()
        assert event[0] == task.id
        assert event[2] == TaskStatus.IN_PROGRESS

    async def test_init_skipped_when_auto_start_disabled(
        self, state_manager: TaskRepository, task_factory, git_repo: Path
    ):
        """Initialization skipped when auto_start=False."""
        await state_manager.create(
            task_factory(
                title="Should not queue",
                status=TaskStatus.IN_PROGRESS,
                task_type=TaskType.AUTO,
            )
        )

        config = create_test_config(auto_start=False)
        worktrees = create_mock_workspace_service()

        scheduler = build_automation(state_manager, worktrees, config)

        await scheduler.initialize_existing_tasks()

        # Queue should be empty
        assert scheduler._event_queue.empty()


# =============================================================================
# Feature: Capacity Management
# =============================================================================


class TestCapacityManagement:
    """AutomationServiceImpl respects max_concurrent_agents limit."""

    async def test_waiting_tasks_processed_when_capacity_frees(
        self, state_manager: TaskRepository, task_factory, git_repo: Path
    ):
        """Waiting tasks are processed when running tasks complete."""
        config = create_test_config(max_concurrent=1)
        worktrees = create_mock_workspace_service()

        scheduler = build_automation(state_manager, worktrees, config)

        # Simulate one running task
        scheduler._running["running-1"] = RunningTaskState()

        # Create waiting task
        await state_manager.create(
            task_factory(
                title="Waiting",
                status=TaskStatus.IN_PROGRESS,
                task_type=TaskType.AUTO,
            )
        )

        # Free capacity
        await scheduler.start()
        await scheduler._stop_if_running("running-1")

        # Should trigger check for waiting tasks
        await asyncio.sleep(0.1)
        await scheduler.stop()

    async def test_get_running_agent_returns_agent(self, state_manager: TaskRepository):
        """get_running_agent returns the agent for a running task."""
        config = create_test_config()
        worktrees = create_mock_workspace_service()

        scheduler = build_automation(state_manager, worktrees, config)

        mock_agent = MagicMock()
        scheduler._running["test-task"] = RunningTaskState(agent=mock_agent)

        result = scheduler.get_running_agent("test-task")

        assert result is mock_agent

    async def test_get_iteration_count(self, state_manager: TaskRepository):
        """get_iteration_count returns current iteration for running task."""
        config = create_test_config()
        worktrees = create_mock_workspace_service()

        scheduler = build_automation(state_manager, worktrees, config)

        scheduler._running["test-task"] = RunningTaskState(iteration=5)

        count = scheduler.get_iteration_count("test-task")

        assert count == 5

    async def test_reset_iterations(self, state_manager: TaskRepository):
        """reset_iterations resets in-memory iteration counter."""
        config = create_test_config()
        worktrees = create_mock_workspace_service()

        scheduler = build_automation(state_manager, worktrees, config)

        scheduler._running["test-task"] = RunningTaskState(iteration=10)

        scheduler.reset_iterations("test-task")

        assert scheduler.get_iteration_count("test-task") == 0
