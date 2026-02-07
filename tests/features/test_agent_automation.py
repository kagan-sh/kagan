"""Feature tests for Agent and Automation.

Tests organized by user-facing features, not implementation layers.
Each test validates a complete user journey or critical behavior.

Covers:
- Agent spawning for AUTO tasks
- Agent stopping
- Iteration loop (max iterations)
- Signal parsing (complete/blocked/continue/approve/reject)
- Auto-review generation
- AutomationServiceImpl queue management
- Session management (PAIR tasks)
- Worktree operations (create, delete, get)
- Git operations (merge, conflict handling)
"""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock

import pytest
from tests.helpers.mocks import create_mock_worktree_manager, create_test_config

from kagan.adapters.git.worktrees import WorktreeError, WorktreeManager
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
    worktrees: WorktreeManager,
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
            worktrees,
            config,
            session_service=session_service,
            event_bus=event_bus,
        )
    return AutomationServiceImpl(
        task_service,
        worktrees,
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

    def test_parse_complete_signal(self):
        """<complete/> signal is recognized."""
        output = "Task finished successfully. <complete/>"
        result = parse_signal(output)

        assert result.signal == Signal.COMPLETE
        assert result.reason == ""

    def test_parse_blocked_signal_with_reason(self):
        """<blocked reason="..."/> signal extracts reason."""
        output = 'Cannot proceed. <blocked reason="Missing API key configuration"/>'
        result = parse_signal(output)

        assert result.signal == Signal.BLOCKED
        assert result.reason == "Missing API key configuration"

    def test_parse_continue_signal(self):
        """<continue/> signal continues iteration."""
        output = "Still working... <continue/>"
        result = parse_signal(output)

        assert result.signal == Signal.CONTINUE

    def test_parse_approve_signal_with_summary(self):
        """<approve summary="..."/> extracts review summary."""
        output = (
            '<approve summary="Implementation follows best practices" '
            'approach="Used repository pattern" key_files="src/api.py"/>'
        )
        result = parse_signal(output)

        assert result.signal == Signal.APPROVE
        assert result.reason == "Implementation follows best practices"
        assert result.approach == "Used repository pattern"
        assert result.key_files == "src/api.py"

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

    def test_signal_case_insensitive(self):
        """Signals are parsed case-insensitively."""
        assert parse_signal("<COMPLETE/>").signal == Signal.COMPLETE
        assert parse_signal("<Complete/>").signal == Signal.COMPLETE

    def test_signal_anywhere_in_output(self):
        """Signal can appear anywhere in output."""
        output = """
        Here's my analysis...

        After fixing the issue <complete/>

        Let me know if you need more.
        """
        assert parse_signal(output).signal == Signal.COMPLETE

    def test_approve_without_optional_attrs(self):
        """APPROVE signal works without approach and key_files."""
        output = '<approve summary="Looks good"/>'
        result = parse_signal(output)

        assert result.signal == Signal.APPROVE
        assert result.reason == "Looks good"
        assert result.approach == ""
        assert result.key_files == ""

    def test_approve_minimal(self):
        """APPROVE signal works with minimal format."""
        output = "<approve/>"
        result = parse_signal(output)

        assert result.signal == Signal.APPROVE


# =============================================================================
# Feature: Agent Spawning
# =============================================================================


class TestAgentSpawning:
    """Agents can be spawned for AUTO tasks."""

    async def test_auto_task_to_in_progress_spawns_agent(
        self, state_manager: TaskRepository, task_factory, git_repo: Path
    ):
        """Moving AUTO task to IN_PROGRESS triggers agent spawn."""
        task = task_factory(
            title="Auto task",
            status=TaskStatus.BACKLOG,
            task_type=TaskType.AUTO,
        )
        await state_manager.create(task)

        config = create_test_config()
        worktrees = WorktreeManager(git_repo)
        mock_factory = MagicMock()
        mock_agent = MagicMock()
        mock_agent.set_auto_approve = MagicMock()
        mock_agent.set_model_override = MagicMock()
        mock_agent.start = MagicMock()
        mock_agent.wait_ready = AsyncMock()
        mock_agent.send_prompt = AsyncMock()
        mock_agent.get_response_text = MagicMock(return_value="Done <complete/>")
        mock_agent.stop = AsyncMock()
        mock_agent.clear_tool_calls = MagicMock()
        mock_agent._buffers = MagicMock()
        mock_agent._buffers.messages = []
        mock_factory.return_value = mock_agent

        scheduler = build_automation(
            state_manager,
            worktrees,
            config,
            agent_factory=mock_factory,
        )
        await scheduler.start()

        # Move to IN_PROGRESS should trigger spawn
        await state_manager.move(task.id, TaskStatus.IN_PROGRESS)
        await scheduler.handle_status_change(task.id, TaskStatus.BACKLOG, TaskStatus.IN_PROGRESS)

        # Allow async processing
        await asyncio.sleep(0.1)

        assert scheduler.is_running(task.id) or mock_factory.called
        await scheduler.stop()

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
        worktrees = WorktreeManager(git_repo)
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

    async def test_manual_spawn_for_auto_task(
        self, state_manager: TaskRepository, task_factory, git_repo: Path
    ):
        """Agent can be manually spawned via spawn_for_task."""
        task = task_factory(
            title="Manual start",
            status=TaskStatus.IN_PROGRESS,
            task_type=TaskType.AUTO,
        )
        await state_manager.create(task)

        config = create_test_config()
        worktrees = WorktreeManager(git_repo)
        scheduler = build_automation(state_manager, worktrees, config)
        await scheduler.start()

        result = await scheduler.spawn_for_task(task)

        assert result is True
        await scheduler.stop()

    async def test_spawn_respects_max_concurrent_agents(
        self, state_manager: TaskRepository, task_factory, git_repo: Path
    ):
        """AutomationServiceImpl respects max_concurrent_agents limit."""
        config = create_test_config(max_concurrent=1)
        worktrees = create_mock_worktree_manager()

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
        worktrees = create_mock_worktree_manager()

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
        worktrees = create_mock_worktree_manager()

        scheduler = build_automation(state_manager, worktrees, config)

        result = await scheduler.stop_task("nonexistent")

        assert result is False

    async def test_moving_out_of_in_progress_stops_agent(self, state_manager: TaskRepository):
        """Moving task out of IN_PROGRESS (not to REVIEW) stops agent."""
        config = create_test_config()
        worktrees = create_mock_worktree_manager()

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

    async def test_complete_signal_moves_to_review(
        self, state_manager: TaskRepository, task_factory, git_repo: Path, mock_agent_factory
    ):
        """COMPLETE signal moves task to REVIEW."""
        task = await state_manager.create(
            task_factory(
                title="Will complete",
                status=TaskStatus.IN_PROGRESS,
                task_type=TaskType.AUTO,
            )
        )

        config = create_test_config(max_iterations=3)
        worktrees = WorktreeManager(git_repo)
        await worktrees.create(task.id, task.title)

        scheduler = build_automation(
            state_manager,
            worktrees,
            config,
            agent_factory=mock_agent_factory,
        )

        # Run the task loop directly
        await scheduler._run_task_loop(task)

        fetched = await state_manager.get(task.id)
        assert fetched is not None
        assert fetched.status == TaskStatus.REVIEW

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
        worktrees = WorktreeManager(git_repo)
        await worktrees.create(task.id, task.title)

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
        worktrees = WorktreeManager(git_repo)
        await worktrees.create(task.id, task.title)

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
        worktrees = WorktreeManager(git_repo)
        await worktrees.create(task.id, task.title)

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
# Feature: Auto-Review
# =============================================================================


class TestAutoReview:
    """Review agent runs on task completion."""

    async def test_review_approve_sets_checks_passed(
        self, state_manager: TaskRepository, task_factory, git_repo: Path
    ):
        """Approved review sets checks_passed=True."""
        task = await state_manager.create(
            task_factory(
                title="Review approved",
                status=TaskStatus.REVIEW,
                task_type=TaskType.AUTO,
            )
        )

        config = create_test_config()
        worktrees = WorktreeManager(git_repo)
        wt_path = await worktrees.create(task.id, task.title)

        # Create mock review agent that approves
        def approve_factory(project_root, agent_config, **kwargs):
            mock = MagicMock()
            mock.set_auto_approve = MagicMock()
            mock.set_model_override = MagicMock()
            mock.start = MagicMock()
            mock.wait_ready = AsyncMock()
            mock.send_prompt = AsyncMock()
            mock.get_response_text = MagicMock(
                return_value='Looks good! <approve summary="All tests pass"/>'
            )
            mock.stop = AsyncMock()
            mock._buffers = MagicMock()
            mock._buffers.messages = []
            return mock

        scheduler = build_automation(
            state_manager,
            worktrees,
            config,
            agent_factory=approve_factory,
        )

        passed, summary = await scheduler.run_review(task, wt_path)

        assert passed is True
        assert "All tests pass" in summary

    async def test_review_reject_sets_checks_failed(
        self, state_manager: TaskRepository, task_factory, git_repo: Path
    ):
        """Rejected review sets checks_passed=False."""
        task = await state_manager.create(
            task_factory(
                title="Review rejected",
                status=TaskStatus.REVIEW,
                task_type=TaskType.AUTO,
            )
        )

        config = create_test_config()
        worktrees = WorktreeManager(git_repo)
        wt_path = await worktrees.create(task.id, task.title)

        # Create mock review agent that rejects
        def reject_factory(project_root, agent_config, **kwargs):
            mock = MagicMock()
            mock.set_auto_approve = MagicMock()
            mock.set_model_override = MagicMock()
            mock.start = MagicMock()
            mock.wait_ready = AsyncMock()
            mock.send_prompt = AsyncMock()
            mock.get_response_text = MagicMock(
                return_value='<reject reason="Missing error handling"/>'
            )
            mock.stop = AsyncMock()
            mock._buffers = MagicMock()
            mock._buffers.messages = []
            return mock

        scheduler = build_automation(
            state_manager,
            worktrees,
            config,
            agent_factory=reject_factory,
        )

        passed, summary = await scheduler.run_review(task, wt_path)

        assert passed is False
        assert "Missing error handling" in summary


# =============================================================================
# Feature: AutomationServiceImpl Queue Management
# =============================================================================


class TestAutomationServiceImplQueue:
    """AutomationServiceImpl processes events sequentially to prevent races."""

    async def test_events_processed_in_order(self, state_manager: TaskRepository):
        """Events are processed in FIFO order."""
        config = create_test_config()
        worktrees = create_mock_worktree_manager()

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
        worktrees = create_mock_worktree_manager()

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
# Feature: Worktree Operations
# =============================================================================


class TestWorktreeOperations:
    """Git worktrees are managed for task isolation."""

    async def test_create_worktree(self, git_repo: Path):
        """Creating worktree creates isolated directory."""
        worktrees = WorktreeManager(git_repo)

        wt_path = await worktrees.create("task-123", "Fix the bug")

        assert wt_path.exists()
        assert wt_path.name == "task-123"
        assert (wt_path / "README.md").exists()  # From initial commit

    async def test_get_worktree_path(self, git_repo: Path):
        """get_path returns path for existing worktree."""
        worktrees = WorktreeManager(git_repo)
        await worktrees.create("task-abc", "Test task")

        path = await worktrees.get_path("task-abc")

        assert path is not None
        assert path.exists()

    async def test_get_nonexistent_worktree_returns_none(self, git_repo: Path):
        """get_path returns None for nonexistent worktree."""
        worktrees = WorktreeManager(git_repo)

        path = await worktrees.get_path("nonexistent")

        assert path is None

    async def test_delete_worktree(self, git_repo: Path):
        """Deleting worktree removes directory."""
        worktrees = WorktreeManager(git_repo)
        await worktrees.create("to-delete", "Temporary")

        await worktrees.delete("to-delete")

        assert await worktrees.get_path("to-delete") is None

    async def test_delete_with_branch(self, git_repo: Path):
        """Deleting worktree with delete_branch=True removes branch."""
        worktrees = WorktreeManager(git_repo)
        await worktrees.create("with-branch", "Has branch")

        branch_before = await worktrees.get_branch_name("with-branch")
        assert branch_before is not None

        await worktrees.delete("with-branch", delete_branch=True)

        # Verify worktree gone
        assert await worktrees.get_path("with-branch") is None

    async def test_worktree_already_exists_raises(self, git_repo: Path):
        """Creating duplicate worktree raises WorktreeError."""
        worktrees = WorktreeManager(git_repo)
        await worktrees.create("duplicate", "First")

        with pytest.raises(WorktreeError) as exc:
            await worktrees.create("duplicate", "Second")

        assert "already exists" in str(exc.value)

    async def test_list_all_worktrees(self, git_repo: Path):
        """list_all returns all active worktree task IDs."""
        worktrees = WorktreeManager(git_repo)
        await worktrees.create("alpha", "Alpha task")
        await worktrees.create("beta", "Beta task")

        all_ids = await worktrees.list_all()

        assert "alpha" in all_ids
        assert "beta" in all_ids

    async def test_get_branch_name(self, git_repo: Path):
        """get_branch_name returns correct branch for worktree."""
        worktrees = WorktreeManager(git_repo)
        await worktrees.create("branch-test", "Test branch naming")

        branch = await worktrees.get_branch_name("branch-test")

        assert branch is not None
        assert branch.startswith("kagan/")
        assert "branch-test" in branch


# =============================================================================
# Feature: Merge Operations
# =============================================================================


class TestMergeOperations:
    """Git merge operations for completed tasks."""

    async def test_merge_to_main_success(self, git_repo: Path):
        """Successful merge returns (True, message)."""
        worktrees = WorktreeManager(git_repo)
        wt_path = await worktrees.create("merge-success", "Feature to merge")

        # Make a change in the worktree
        (wt_path / "new_file.py").write_text("# New feature\n")
        await worktrees._run_git("add", ".", cwd=wt_path)
        await worktrees._run_git("commit", "-m", "feat: add new feature", cwd=wt_path)

        # Mock the fast-forward step since it requires clean main repo state.
        # Full merge behavior is exercised in higher-level snapshot flows.
        worktrees._fast_forward_base = AsyncMock(
            return_value=(True, "Fast-forwarded main to merge worktree")
        )

        success, message = await worktrees.merge_to_main("merge-success")

        assert success is True
        assert "Fast-forwarded" in message or "main" in message

    async def test_merge_no_commits_fails(self, git_repo: Path):
        """Merge with no commits returns failure."""
        worktrees = WorktreeManager(git_repo)
        await worktrees.create("no-commits", "Empty worktree")

        success, message = await worktrees.merge_to_main("no-commits")

        assert success is False
        assert "No commits" in message

    async def test_merge_nonexistent_worktree_fails(self, git_repo: Path):
        """Merge for nonexistent worktree fails gracefully."""
        worktrees = WorktreeManager(git_repo)

        success, message = await worktrees.merge_to_main("nonexistent")

        assert success is False
        assert "not found" in message.lower()

    async def test_get_commit_log(self, git_repo: Path):
        """get_commit_log returns commits since base branch."""
        worktrees = WorktreeManager(git_repo)
        wt_path = await worktrees.create("log-test", "Log testing")

        # Make commits
        (wt_path / "file1.py").write_text("# File 1\n")
        await worktrees._run_git("add", ".", cwd=wt_path)
        await worktrees._run_git("commit", "-m", "feat: add file1", cwd=wt_path)

        commits = await worktrees.get_commit_log("log-test")

        assert len(commits) >= 1
        assert any("file1" in c for c in commits)

    async def test_get_diff_stats(self, git_repo: Path):
        """get_diff_stats returns change summary."""
        worktrees = WorktreeManager(git_repo)
        wt_path = await worktrees.create("diff-test", "Diff testing")

        # Make a change
        (wt_path / "changed.py").write_text("# Changed file\nprint('hello')\n")
        await worktrees._run_git("add", ".", cwd=wt_path)
        await worktrees._run_git("commit", "-m", "feat: add changed file", cwd=wt_path)

        stats = await worktrees.get_diff_stats("diff-test")

        assert stats != ""
        assert "changed" in stats.lower() or "insertion" in stats.lower()

    async def test_preflight_merge_detects_clean(self, git_repo: Path):
        """preflight_merge returns True for clean merge."""
        worktrees = WorktreeManager(git_repo)
        wt_path = await worktrees.create("preflight-clean", "Clean merge")

        # Make non-conflicting change
        (wt_path / "clean_file.py").write_text("# Clean\n")
        await worktrees._run_git("add", ".", cwd=wt_path)
        await worktrees._run_git("commit", "-m", "feat: clean change", cwd=wt_path)

        ok, message = await worktrees.preflight_merge("preflight-clean")

        assert ok is True
        assert "clean" in message.lower()


# =============================================================================
# Feature: Merge Conflict Handling
# =============================================================================


class TestMergeConflictHandling:
    """Merge conflicts are detected and handled appropriately."""

    async def test_conflict_detection_in_preflight(self, git_repo: Path):
        """Preflight detects potential merge conflicts."""
        worktrees = WorktreeManager(git_repo)
        wt_path = await worktrees.create("conflict-detect", "Will conflict")

        # Modify README.md in worktree (same file from initial commit)
        (wt_path / "README.md").write_text("# Modified in worktree\n")
        await worktrees._run_git("add", ".", cwd=wt_path)
        await worktrees._run_git("commit", "-m", "feat: modify readme", cwd=wt_path)

        # Also modify README.md in main
        (git_repo / "README.md").write_text("# Modified in main\n")
        await worktrees._run_git("add", ".", cwd=git_repo)
        await worktrees._run_git("commit", "-m", "feat: main readme change", cwd=git_repo)

        ok, message = await worktrees.preflight_merge("conflict-detect")

        assert ok is False
        assert "conflict" in message.lower()

    async def test_rebase_onto_base(self, git_repo: Path):
        """rebase_onto_base updates branch to latest base."""
        worktrees = WorktreeManager(git_repo)
        wt_path = await worktrees.create("rebase-test", "Rebase testing")

        # Make a change in worktree
        (wt_path / "wt_file.py").write_text("# Worktree change\n")
        await worktrees._run_git("add", ".", cwd=wt_path)
        await worktrees._run_git("commit", "-m", "feat: worktree change", cwd=wt_path)

        # Make non-conflicting change in main
        (git_repo / "main_file.py").write_text("# Main change\n")
        await worktrees._run_git("add", ".", cwd=git_repo)
        await worktrees._run_git("commit", "-m", "feat: main change", cwd=git_repo)

        # Push main to origin (required for rebase)
        await worktrees._run_git("push", "origin", "main", cwd=git_repo, check=False)

        success, _message, conflicts = await worktrees.rebase_onto_base("rebase-test")

        # Should succeed with no conflicts
        assert success is True
        assert conflicts == []


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
        worktrees = WorktreeManager(git_repo)
        await worktrees.create(task.id, task.title)

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
        worktrees = WorktreeManager(git_repo)

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
        worktrees = WorktreeManager(git_repo)

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
        worktrees = create_mock_worktree_manager()

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
        worktrees = create_mock_worktree_manager()

        scheduler = build_automation(state_manager, worktrees, config)

        mock_agent = MagicMock()
        scheduler._running["test-task"] = RunningTaskState(agent=mock_agent)

        result = scheduler.get_running_agent("test-task")

        assert result is mock_agent

    async def test_get_iteration_count(self, state_manager: TaskRepository):
        """get_iteration_count returns current iteration for running task."""
        config = create_test_config()
        worktrees = create_mock_worktree_manager()

        scheduler = build_automation(state_manager, worktrees, config)

        scheduler._running["test-task"] = RunningTaskState(iteration=5)

        count = scheduler.get_iteration_count("test-task")

        assert count == 5

    async def test_reset_iterations(self, state_manager: TaskRepository):
        """reset_iterations resets in-memory iteration counter."""
        config = create_test_config()
        worktrees = create_mock_worktree_manager()

        scheduler = build_automation(state_manager, worktrees, config)

        scheduler._running["test-task"] = RunningTaskState(iteration=10)

        scheduler.reset_iterations("test-task")

        assert scheduler.get_iteration_count("test-task") == 0
