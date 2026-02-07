"""Reactive automation service for AUTO task execution."""

from __future__ import annotations

import asyncio
import contextlib
import weakref
from dataclasses import dataclass
from datetime import datetime
from typing import TYPE_CHECKING

from kagan.agents.agent_factory import AgentFactory, create_agent
from kagan.agents.output import serialize_agent_output
from kagan.agents.prompt import build_prompt
from kagan.agents.prompt_loader import get_review_prompt
from kagan.agents.signals import Signal, SignalResult, parse_signal
from kagan.constants import MODAL_TITLE_MAX_LENGTH
from kagan.core.events import DomainEvent, EventBus, TaskStatusChanged
from kagan.core.models.enums import (
    ExecutionKind,
    ExecutionRunReason,
    ExecutionStatus,
    NotificationSeverity,
    SessionStatus,
    SessionType,
    TaskStatus,
    TaskType,
)
from kagan.debug_log import log
from kagan.git_utils import get_git_user_identity
from kagan.limits import AGENT_TIMEOUT_LONG

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from kagan.acp.agent import Agent
    from kagan.adapters.git.operations import GitOperationsAdapter
    from kagan.config import AgentConfig, KaganConfig
    from kagan.services.executions import ExecutionService
    from kagan.services.merges import MergeService
    from kagan.services.queued_messages import QueuedMessage, QueuedMessageService
    from kagan.services.sessions import SessionService
    from kagan.services.tasks import TaskService
    from kagan.services.types import TaskLike
    from kagan.services.workspaces import WorkspaceService


@dataclass(slots=True)
class RunningTaskState:
    """State for a currently running task."""

    task: asyncio.Task[None] | None = None
    agent: Agent | None = None
    run_count: int = 0
    review_agent: Agent | None = None
    is_reviewing: bool = False
    session_id: str | None = None
    execution_id: str | None = None


@dataclass(slots=True)
class AutomationEvent:
    """Queue item for automation worker."""

    kind: ExecutionKind
    task_id: str
    old_status: TaskStatus | None = None
    new_status: TaskStatus | None = None


class AutomationService:
    """Reactive automation service for AUTO task processing.

    Instead of polling, reacts to task status changes via a queue.
    Single worker loop processes all spawn/stop requests sequentially,
    eliminating race conditions.
    """

    def __init__(
        self,
        task_service: TaskService,
        workspace_service: WorkspaceService,
        config: KaganConfig,
        session_service: SessionService | None = None,
        execution_service: ExecutionService | None = None,
        merge_service: MergeService | None = None,
        on_task_changed: Callable[[], None] | None = None,
        on_error: Callable[[str, str], None] | None = None,
        notifier: Callable[[str, str, NotificationSeverity], None] | None = None,
        agent_factory: AgentFactory = create_agent,
        event_bus: EventBus | None = None,
        queued_message_service: QueuedMessageService | None = None,
        git_adapter: GitOperationsAdapter | None = None,
    ) -> None:
        self._tasks = task_service
        self._workspaces = workspace_service
        self._config = config
        self._sessions = session_service
        self._executions = execution_service
        self._merge_service = merge_service
        self._queued = queued_message_service
        self._running: dict[str, RunningTaskState] = {}
        self._on_task_changed = on_task_changed
        self._on_error = on_error
        self._notifier = notifier
        self._agent_factory = agent_factory
        self._event_bus = event_bus
        self._git = git_adapter

        self._event_queue: asyncio.Queue[AutomationEvent] = asyncio.Queue()
        self._worker_task: asyncio.Task[None] | None = None
        self._event_task: asyncio.Task[None] | None = None
        self._started = False

        self._merge_lock = asyncio.Lock()

    @property
    def merge_lock(self) -> asyncio.Lock:
        """Lock for serializing merge operations."""
        return self._merge_lock

    def set_merge_service(self, merge_service: MergeService) -> None:
        """Attach merge service after initialization to avoid circular wiring."""
        self._merge_service = merge_service

    async def start(self) -> None:
        """Start the automation event processing loop."""
        if self._started:
            return
        self._started = True
        self._worker_task = asyncio.create_task(self._worker_loop())
        if self._event_bus:
            self._event_task = asyncio.create_task(self._event_loop())
        log.info("Automation service started (reactive mode)")

    async def handle_event(self, event: DomainEvent) -> None:
        """Process a domain event and trigger actions."""
        if isinstance(event, TaskStatusChanged):
            await self._event_queue.put(
                AutomationEvent(
                    kind=ExecutionKind.STATUS,
                    task_id=event.task_id,
                    old_status=event.from_status,
                    new_status=event.to_status,
                )
            )

    async def _event_loop(self) -> None:
        """Subscribe to domain events and enqueue relevant automation work."""
        assert self._event_bus is not None
        async for event in self._event_bus.subscribe(TaskStatusChanged):
            await self.handle_event(event)

    async def handle_status_change(
        self, task_id: str, old_status: TaskStatus | None, new_status: TaskStatus | None
    ) -> None:
        """Handle a task status change event.

        Called by TaskService when task status changes.
        Queues the event for processing by the worker loop.
        """
        await self._event_queue.put(
            AutomationEvent(
                kind=ExecutionKind.STATUS,
                task_id=task_id,
                old_status=old_status,
                new_status=new_status,
            )
        )
        log.debug(f"Queued status change: {task_id} {old_status} -> {new_status}")

    async def _worker_loop(self) -> None:
        """Single worker that processes all events sequentially.

        This eliminates race conditions because all spawn/stop decisions
        happen in one place, one at a time.
        """
        log.info("Automation worker loop started")
        while True:
            try:
                event = await self._event_queue.get()
                await self._process_event(event)
            except asyncio.CancelledError:
                log.info("Automation worker loop cancelled")
                break
            except Exception as e:
                log.error(f"Error in automation worker: {e}")

    async def _process_event(self, event: AutomationEvent) -> None:
        """Process a queued automation event."""
        if event.kind == ExecutionKind.SPAWN:
            await self._process_spawn(event.task_id)
            return

        await self._process_status_event(event.task_id, event.old_status, event.new_status)

    async def _process_status_event(
        self, task_id: str, old_status: TaskStatus | None, new_status: TaskStatus | None
    ) -> None:
        """Process a single status change event."""
        if new_status is None:
            await self._stop_if_running(task_id)
            return

        task = await self._tasks.get_task(task_id)
        if task is None:
            await self._stop_if_running(task_id)
            return

        if task.task_type != TaskType.AUTO:
            return

        if old_status == TaskStatus.IN_PROGRESS and new_status != TaskStatus.REVIEW:
            # Don't stop for REVIEW transitions as that's part of normal completion flow
            await self._stop_if_running(task_id)

    async def _process_spawn(self, task_id: str) -> None:
        """Handle explicit spawn requests from the UI."""
        if task_id in self._running:
            log.debug(f"Task {task_id} already running")
            return

        task = await self._tasks.get_task(task_id)
        if task is None:
            return
        if task.task_type != TaskType.AUTO:
            return

        max_agents = self._config.general.max_concurrent_agents
        if len(self._running) >= max_agents:
            log.debug(f"At capacity ({max_agents}), task {task.id[:8]} will not start")
            return

        await self._spawn(task)

    async def _spawn(self, task: TaskLike) -> None:
        """Spawn an agent for a task."""
        title = task.title[:MODAL_TITLE_MAX_LENGTH]
        log.info(f"Spawning agent for AUTO task {task.id}: {title}")

        state = RunningTaskState()
        self._running[task.id] = state

        runner_task = asyncio.create_task(self._run_task_loop(task))
        state.task = runner_task

        runner_task.add_done_callback(self._make_done_callback(task.id))

    async def _stop_if_running(self, task_id: str) -> None:
        """Stop agent if running."""
        state = self._running.get(task_id)
        if state is None:
            return

        log.info(f"Stopping agent for task {task_id}")

        if state.agent is not None:
            await state.agent.stop()

        if state.task is not None and not state.task.done():
            state.task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await state.task

        self._running.pop(task_id, None)

    def _handle_task_done(self, task_id: str, task: asyncio.Task[None]) -> None:
        """Handle agent task completion."""
        self._running.pop(task_id, None)

    def _make_done_callback(self, task_id: str) -> Callable[[asyncio.Task[None]], None]:
        """Create task done callback with weak self reference."""
        weak_self = weakref.ref(self)

        def on_done(task: asyncio.Task[None]) -> None:
            service = weak_self()
            if service is not None:
                service._handle_task_done(task_id, task)

        return on_done

    @property
    def running_tasks(self) -> set[str]:
        """Get set of currently running task IDs."""
        return set(self._running.keys())

    def is_running(self, task_id: str) -> bool:
        """Check if a task is currently being processed."""
        return task_id in self._running

    def get_running_agent(self, task_id: str) -> Agent | None:
        """Get the running agent for a task (for watch functionality).

        Returns None if the task is not running (not in _running).
        May also return None during brief initialization window when task
        is in _running but agent hasn't been created yet.
        """
        if task_id not in self._running:
            return None
        state = self._running[task_id]
        return state.agent

    def get_execution_id(self, task_id: str) -> str | None:
        """Get execution id for a running task."""
        state = self._running.get(task_id)
        return state.execution_id if state else None

    def get_run_count(self, task_id: str) -> int:
        """Get current run count for a task."""
        state = self._running.get(task_id)
        return state.run_count if state else 0

    def is_reviewing(self, task_id: str) -> bool:
        """Check if task is currently in review phase."""
        state = self._running.get(task_id)
        return state.is_reviewing if state else False

    def get_review_agent(self, task_id: str) -> Agent | None:
        """Get the running review agent for a task (for watch functionality)."""
        state = self._running.get(task_id)
        return state.review_agent if state else None

    async def stop_task(self, task_id: str) -> bool:
        """Request to stop a task. Returns True if was running."""
        if task_id not in self._running:
            return False

        await self._event_queue.put(
            AutomationEvent(
                kind=ExecutionKind.STATUS,
                task_id=task_id,
                old_status=TaskStatus.IN_PROGRESS,
                new_status=TaskStatus.BACKLOG,
            )
        )
        return True

    async def spawn_for_task(self, task: TaskLike) -> bool:
        """Manually request to spawn an agent for a task.

        Used by UI for manual agent starts. Returns True if spawn was queued.
        """
        if task.id in self._running:
            return False
        if task.task_type != TaskType.AUTO:
            return False

        await self._event_queue.put(AutomationEvent(kind=ExecutionKind.SPAWN, task_id=task.id))
        return True

    async def stop(self) -> None:
        """Stop the automation service and all running agents."""
        log.info("Stopping automation service")

        if self._event_task and not self._event_task.done():
            self._event_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._event_task

        if self._worker_task and not self._worker_task.done():
            self._worker_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._worker_task

        for task_id, state in list(self._running.items()):
            log.info(f"Stopping agent for task {task_id}")
            if state.agent is not None:
                await state.agent.stop()
            if state.task is not None and not state.task.done():
                state.task.cancel()

        self._running.clear()
        self._started = False

    def _notify_task_changed(self) -> None:
        """Notify that a task has changed status."""
        if self._on_task_changed:
            self._on_task_changed()

    def _notify_error(self, task_id: str, message: str) -> None:
        """Notify that an error occurred for a task."""
        if self._on_error:
            self._on_error(task_id, message)

    async def _run_task_loop(self, task: TaskLike) -> None:
        """Run a single execution for a task."""
        log.info(f"Starting task loop for {task.id}")
        self._notify_error(task.id, "Agent starting...")
        final_status: ExecutionStatus | None = None
        agent: Agent | None = None

        try:
            wt_path = await self._workspaces.get_path(task.id)
            if wt_path is None:
                log.info(f"Creating worktree for {task.id}")
                try:
                    wt_path = await self._workspaces.create(
                        task.id,
                        base_branch=task.base_branch or self._config.general.default_base_branch,
                    )
                except ValueError as exc:
                    error_msg = str(exc)
                    log.error(f"Workspace creation failed for task {task.id}: {error_msg}")
                    self._notify_error(task.id, error_msg)
                    self._notify_user(
                        f"❌ {error_msg}",
                        title="Cannot Start Agent",
                        severity=NotificationSeverity.ERROR,
                    )
                    await self._update_task_status(task.id, TaskStatus.BACKLOG)
                    return
                except Exception as exc:
                    error_str = str(exc).lower()
                    if "not a git repository" in error_str or "fatal:" in error_str:
                        error_msg = f"Repository is not a valid git repo: {exc}"
                    else:
                        error_msg = f"Failed to create workspace: {exc}"
                    log.error(f"Workspace creation failed for task {task.id}: {exc}")
                    self._notify_error(task.id, error_msg)
                    self._notify_user(
                        f"❌ {error_msg}",
                        title="Cannot Start Agent",
                        severity=NotificationSeverity.ERROR,
                    )
                    await self._update_task_status(task.id, TaskStatus.BACKLOG)
                    return
            log.info(f"Worktree path: {wt_path}")

            if self._executions is None:
                raise RuntimeError("Execution service is required for automation runs")

            workspaces = await self._workspaces.list_workspaces(task_id=task.id)
            if not workspaces:
                raise RuntimeError(f"No workspace record found for task {task.id}")
            workspace_id = workspaces[0].id

            session_record = await self._tasks.create_session_record(
                workspace_id=workspace_id,
                session_type=SessionType.ACP,
                external_id=None,
            )

            execution = await self._executions.create_execution(
                session_id=session_record.id,
                run_reason=ExecutionRunReason.CODINGAGENT,
                executor_action={},
            )

            state = self._running.get(task.id)
            if state:
                state.session_id = session_record.id
                state.execution_id = execution.id
                state.run_count = 0

            user_name, user_email = await get_git_user_identity()
            log.debug(f"Git user identity: {user_name} <{user_email}>")

            agent_config = self._get_agent_config(task)
            log.debug(f"Agent config: {agent_config.name}")
            run_count = await self._executions.count_executions_for_task(task.id)
            log.info(f"Starting run for {task.id}, run={run_count}")

            state = self._running.get(task.id)
            if state:
                state.run_count = run_count

            signal, agent = await self._run_execution(
                task,
                wt_path,
                agent_config,
                run_count,
                execution.id,
                user_name=user_name,
                user_email=user_email,
            )

            if agent is not None:
                state = self._running.get(task.id)
                if state:
                    state.agent = agent

            log.debug(f"Task {task.id} run {run_count} signal: {signal}")

            if signal.signal == Signal.COMPLETE:
                final_status = ExecutionStatus.COMPLETED

                state = self._running.get(task.id)
                queued = await self._take_implementation_queue(
                    task.id,
                    state.session_id if state else None,
                )
                if queued is not None:
                    await self._append_queued_message_to_scratchpad(task.id, queued.content)
                    await self._update_task_status(task.id, TaskStatus.IN_PROGRESS)
                    self._notify_task_changed()
                    log.info(f"Task {task.id} has queued messages, re-spawning")
                    await self._event_queue.put(
                        AutomationEvent(kind=ExecutionKind.SPAWN, task_id=task.id)
                    )
                    return

                log.info(f"Task {task.id} completed, moving to REVIEW")
                await self._handle_complete(task)
                return
            if signal.signal == Signal.BLOCKED:
                log.warning(f"Task {task.id} blocked: {signal.reason}")
                self._notify_error(task.id, f"Blocked: {signal.reason}")
                await self._handle_blocked(task, signal.reason)
                final_status = ExecutionStatus.FAILED
                return

            log.info(f"Task {task.id} run {run_count} complete; awaiting next run")
            final_status = ExecutionStatus.COMPLETED

        except asyncio.CancelledError:
            log.info(f"Task {task.id} cancelled")
            final_status = ExecutionStatus.KILLED
            raise
        except Exception as e:
            import traceback

            tb = traceback.format_exc()
            log.error(f"Exception in task loop for {task.id}: {e}")
            log.error(f"Traceback:\n{tb}")
            self._notify_error(task.id, f"Agent failed: {e}")
            await self._update_task_status(task.id, TaskStatus.BACKLOG)
            final_status = ExecutionStatus.FAILED
        finally:
            if agent is not None:
                await agent.stop()
            state = self._running.get(task.id)
            if state and self._executions and state.execution_id:
                await self._executions.update_execution(
                    state.execution_id,
                    status=final_status or ExecutionStatus.FAILED,
                    completed_at=datetime.now(),
                )
            if state and state.session_id:
                await self._tasks.close_session_record(
                    state.session_id, status=SessionStatus.CLOSED
                )
            log.info(f"Task loop ended for {task.id}")

    def _get_agent_config(self, task: TaskLike) -> AgentConfig:
        """Get agent config for a task."""
        return task.get_agent_config(self._config)

    def _notify_user(self, message: str, title: str, severity: NotificationSeverity) -> None:
        """Send a notification to the user via the app if available.

        Args:
            message: The notification message.
            title: The notification title.
            severity: The severity level (information, warning, error).
        """
        if self._notifier is not None:
            self._notifier(message, title, severity)

    def _apply_model_override(self, agent: Agent, agent_config: AgentConfig, context: str) -> None:
        """Apply model override to agent if configured.

        Args:
            agent: The agent to configure.
            agent_config: The agent configuration.
            context: Context string for logging (e.g., "task ABC-123" or "review").
        """

        model = None
        if "claude" in agent_config.identity.lower():
            model = self._config.general.default_model_claude
        elif "opencode" in agent_config.identity.lower():
            model = self._config.general.default_model_opencode

        if model:
            agent.set_model_override(model)
            log.info(f"Applied model override for {context}: {model}")

    async def _update_task_status(self, task_id: str, status: TaskStatus) -> None:
        """Update task status."""
        await self._tasks.update_fields(task_id, status=status)

    async def _take_implementation_queue(
        self,
        task_id: str,
        session_id: str | None,
    ) -> QueuedMessage | None:
        if self._queued is None:
            return None
        queued = await self._queued.take_queued(task_id, lane="implementation")
        if queued is not None:
            return queued
        if session_id is None:
            return None
        return await self._queued.take_queued(session_id, lane="implementation")

    async def _append_queued_message_to_scratchpad(self, task_id: str, content: str) -> None:
        scratchpad = await self._tasks.get_scratchpad(task_id)
        user_note = f"\n\n--- USER MESSAGE ---\n{_truncate_queue_payload(content)}"
        await self._tasks.update_scratchpad(task_id, scratchpad + user_note)

    async def run_review(
        self, task: TaskLike, wt_path: Path, execution_id: str
    ) -> tuple[bool, str]:
        """Run agent-based review and return (passed, summary).

        Args:
            task: The task to review.
            wt_path: Path to the worktree.

        Returns:
            Tuple of (passed, summary).
        """
        agent_config = self._get_agent_config(task)
        prompt = await self._build_review_prompt(task)

        agent = self._agent_factory(wt_path, agent_config, read_only=True)
        agent.set_auto_approve(True)

        self._apply_model_override(agent, agent_config, f"review of task {task.id}")

        agent.start()

        state = self._running.get(task.id)
        if state:
            state.review_agent = agent
            state.is_reviewing = True

        try:
            await agent.wait_ready(timeout=AGENT_TIMEOUT_LONG)
            await agent.send_prompt(prompt)
            response = agent.get_response_text()

            serialized_output = serialize_agent_output(agent)
            if self._executions is not None:
                await self._executions.append_log(execution_id, serialized_output)
                await self._executions.append_agent_turn(
                    execution_id,
                    prompt=prompt,
                    summary=response,
                )

            signal = parse_signal(response)
            if signal.signal == Signal.APPROVE:
                return True, signal.reason
            elif signal.signal == Signal.REJECT:
                return False, signal.reason
            else:
                return False, "No review signal found in agent response"
        except TimeoutError:
            log.error(f"Review agent timeout for task {task.id}")
            return False, "Review agent timed out"
        except Exception as e:
            log.error(f"Review agent failed for {task.id}: {e}")
            return False, f"Review agent error: {e}"
        finally:
            state = self._running.get(task.id)
            if state:
                state.review_agent = None
                state.is_reviewing = False
            await agent.stop()

    async def _run_execution(
        self,
        task: TaskLike,
        wt_path: Path,
        agent_config: AgentConfig,
        run_count: int,
        execution_id: str,
        user_name: str = "Developer",
        user_email: str = "developer@localhost",
    ) -> tuple[SignalResult, Agent | None]:
        """Run a single execution for a task.

        Returns:
            Tuple of (signal_result, agent) where agent is the created agent.
        """
        agent = self._agent_factory(wt_path, agent_config)
        # Worker agents always auto-approve: they run in isolated worktrees
        # with path-confined file access. The auto_approve config setting
        # only governs the interactive planner agent.
        agent.set_auto_approve(True)

        self._apply_model_override(agent, agent_config, f"task {task.id}")

        agent.start()

        state = self._running.get(task.id)
        if state:
            state.agent = agent

        try:
            await agent.wait_ready(timeout=AGENT_TIMEOUT_LONG)
        except TimeoutError:
            log.error(f"Agent timeout for task {task.id}")
            return (parse_signal('<blocked reason="Agent failed to start"/>'), agent)

        scratchpad = await self._tasks.get_scratchpad(task.id)
        prompt = build_prompt(
            task=task,
            run_count=run_count,
            scratchpad=scratchpad,
            user_name=user_name,
            user_email=user_email,
        )

        log.info(f"Sending prompt to agent for task {task.id}, run {run_count}")
        try:
            await agent.send_prompt(prompt)
        except Exception as e:
            log.error(f"Agent prompt failed for {task.id}: {e}")
            return (parse_signal(f'<blocked reason="Agent error: {e}"/>'), agent)
        finally:
            agent.clear_tool_calls()

        response = agent.get_response_text()
        signal_result = parse_signal(response)

        serialized_output = serialize_agent_output(agent)
        if self._executions is not None:
            await self._executions.append_log(execution_id, serialized_output)
            await self._executions.append_agent_turn(
                execution_id,
                prompt=prompt,
                summary=response,
            )

        progress_note = f"\n\n--- Run {run_count} ---\n{response[-2000:]}"
        await self._tasks.update_scratchpad(task.id, scratchpad + progress_note)

        return (signal_result, agent)

    async def _handle_complete(self, task: TaskLike) -> None:
        """Handle task completion - auto-commit leftover changes, move to REVIEW, then review."""
        # Safety net: commit any uncommitted changes the agent left behind
        wt_path = await self._workspaces.get_path(task.id)
        if wt_path is not None and self._git is not None:
            if await self._git.has_uncommitted_changes(str(wt_path)):
                short_id = task.id[:8]
                await self._git.commit_all(
                    str(wt_path),
                    f"chore: adding uncommitted agent changes ({short_id})",
                )
                log.info(f"Auto-committed leftover changes for task {task.id}")

        await self._tasks.update_fields(task.id, status=TaskStatus.REVIEW)
        self._notify_task_changed()

        if not self._config.general.auto_review:
            log.info(f"Auto review disabled, skipping review for task {task.id}")
            return

        wt_path = await self._workspaces.get_path(task.id)
        review_passed = False
        review_note = ""
        review_attempted = False
        execution_id = None
        state = self._running.get(task.id)
        if state:
            execution_id = state.execution_id

        if wt_path is not None and execution_id is not None:
            review_passed, review_note = await self.run_review(task, wt_path, execution_id)
            review_attempted = True

            status = "approved" if review_passed else "rejected"
            log.info(f"Task {task.id} review: {status}")

            if review_passed:
                self._notify_user(
                    f"✓ Review passed: {task.title[:30]}",
                    title="Review Complete",
                    severity=NotificationSeverity.INFORMATION,
                )
            else:
                self._notify_user(
                    f"✗ Review failed: {review_note[:50]}",
                    title="Review Complete",
                    severity=NotificationSeverity.WARNING,
                )

        if review_note:
            scratchpad = await self._tasks.get_scratchpad(task.id)
            note = f"\n\n--- REVIEW ---\n{review_note}"
            await self._tasks.update_scratchpad(task.id, scratchpad + note)
            self._notify_task_changed()

        if review_attempted and execution_id is not None and self._executions is not None:
            review_result = {
                "status": "approved" if review_passed else "rejected",
                "summary": review_note,
                "completed_at": datetime.now().isoformat(),
            }
            await self._executions.update_execution(
                execution_id,
                metadata={"review_result": review_result},
            )

    async def _handle_blocked(self, task: TaskLike, reason: str) -> None:
        """Handle blocked task - move back to BACKLOG with reason."""
        scratchpad = await self._tasks.get_scratchpad(task.id)
        block_note = f"\n\n--- BLOCKED ---\nReason: {reason}\n"
        await self._tasks.update_scratchpad(task.id, scratchpad + block_note)

        await self._tasks.update_fields(task.id, status=TaskStatus.BACKLOG)
        self._notify_task_changed()

    async def _build_review_prompt(self, task: TaskLike) -> str:
        """Build review prompt from template with commits and diff."""
        base = task.base_branch or self._config.general.default_base_branch
        commits = await self._workspaces.get_commit_log(task.id, base)
        diff_summary = await self._workspaces.get_diff_stats(task.id, base)

        return get_review_prompt(
            title=task.title,
            task_id=task.id,
            description=task.description or "",
            commits="\n".join(f"- {c}" for c in commits) if commits else "No commits",
            diff_summary=diff_summary or "No changes",
        )


def _truncate_queue_payload(content: str, max_chars: int = 8000) -> str:
    """Trim queued messages to preserve prompt budget."""
    if len(content) <= max_chars:
        return content
    prefix = "[queued context truncated]\n"
    return f"{prefix}{content[-(max_chars - len(prefix)) :]}"
