from __future__ import annotations

import asyncio
import contextlib
import weakref
from typing import TYPE_CHECKING

from kagan.adapters.db.repositories.base import RepositoryClosing
from kagan.agents.output import serialize_agent_output
from kagan.agents.prompt import build_prompt
from kagan.agents.signals import Signal, SignalResult, parse_signal
from kagan.constants import MODAL_TITLE_MAX_LENGTH
from kagan.core.events import (
    AutomationAgentAttached,
    AutomationReviewAgentAttached,
    AutomationTaskEnded,
    AutomationTaskStarted,
    DomainEvent,
    EventBus,
    TaskStatusChanged,
)
from kagan.core.models.enums import (
    ExecutionKind,
    ExecutionRunReason,
    ExecutionStatus,
    NotificationSeverity,
    SessionStatus,
    SessionType,
    TaskStatus,
)
from kagan.core.time import utc_now
from kagan.debug_log import log
from kagan.git_utils import get_git_user_identity
from kagan.limits import AGENT_TIMEOUT_LONG
from kagan.utils.background_tasks import BackgroundTasks

from .policy import can_spawn_new_agent, is_auto_task, should_stop_running_on_status_change
from .reviewer import AutomationReviewer
from .state import AutomationEvent, RunningTaskState

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from kagan.acp import Agent
    from kagan.adapters.db.repositories import ExecutionRepository
    from kagan.adapters.git.operations import GitOperationsProtocol
    from kagan.agents.agent_factory import AgentFactory
    from kagan.config import AgentConfig, KaganConfig
    from kagan.services.queued_messages import QueuedMessage, QueuedMessageService
    from kagan.services.runtime import RuntimeService, RuntimeTaskView
    from kagan.services.sessions import SessionService
    from kagan.services.tasks import TaskService
    from kagan.services.types import TaskLike
    from kagan.services.workspaces import WorkspaceService


class AutomationEngine:
    _tasks: TaskService
    _workspaces: WorkspaceService
    _config: KaganConfig
    _sessions: SessionService | None
    _executions: ExecutionRepository | None
    _queued: QueuedMessageService | None
    _running: dict[str, RunningTaskState]
    _on_task_changed: Callable[[], None] | None
    _on_error: Callable[[str, str], None] | None
    _notifier: Callable[[str, str, NotificationSeverity], None] | None
    _agent_factory: AgentFactory
    _event_bus: EventBus | None
    _git: GitOperationsProtocol | None
    _runtime_service: RuntimeService
    _event_queue: asyncio.Queue[AutomationEvent]
    _worker_task: asyncio.Task[None] | None
    _event_task: asyncio.Task[None] | None
    _background_tasks: BackgroundTasks
    _started: bool
    _reviewer: AutomationReviewer

    def __init__(
        self,
        *,
        task_service: TaskService,
        workspace_service: WorkspaceService,
        config: KaganConfig,
        runtime_service: RuntimeService,
        session_service: SessionService | None = None,
        execution_service: ExecutionRepository | None = None,
        on_task_changed: Callable[[], None] | None = None,
        on_error: Callable[[str, str], None] | None = None,
        notifier: Callable[[str, str, NotificationSeverity], None] | None = None,
        agent_factory: AgentFactory,
        event_bus: EventBus | None = None,
        queued_message_service: QueuedMessageService | None = None,
        git_adapter: GitOperationsProtocol | None = None,
    ) -> None:
        self._tasks = task_service
        self._workspaces = workspace_service
        self._config = config
        self._sessions = session_service
        self._executions = execution_service
        self._queued = queued_message_service
        self._running = {}
        self._on_task_changed = on_task_changed
        self._on_error = on_error
        self._notifier = notifier
        self._agent_factory = agent_factory
        self._event_bus = event_bus
        self._git = git_adapter
        self._runtime_service = runtime_service

        self._event_queue = asyncio.Queue()
        self._worker_task = None
        self._event_task = None
        self._background_tasks = BackgroundTasks()
        self._started = False

        self._reviewer = AutomationReviewer(
            task_service=task_service,
            workspace_service=workspace_service,
            config=config,
            execution_service=execution_service,
            notifier=notifier,
            agent_factory=agent_factory,
            git_adapter=git_adapter,
            runtime_service=runtime_service,
            get_agent_config=self._get_agent_config,
            apply_model_override=self._apply_model_override,
            set_review_agent=self._set_review_agent,
            notify_task_changed=self._notify_task_changed,
        )

    async def start(self) -> None:
        """Start the automation event processing loop."""
        if self._started:
            return
        self._started = True
        self._worker_task = asyncio.create_task(self._worker_loop())
        if self._event_bus:
            self._event_task = asyncio.create_task(self._event_loop())
        log.info("Automation service started (reactive mode)")

    def _check_runtime_view_consistency(self, task_id: str, *, phase: str) -> None:
        """Log lifecycle mismatches between worker and runtime view state."""
        view = self._runtime_service.get(task_id)

        has_state = task_id in self._running
        has_view = view is not None and view.is_running
        if has_state != has_view:
            log.warning(
                f"Runtime state mismatch at {phase} for task {task_id}: "
                f"has_state={has_state}, has_view={has_view}"
            )

    def _runtime_view(self, task_id: str) -> RuntimeTaskView | None:
        return self._runtime_service.get(task_id)

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

        if not is_auto_task(task.task_type):
            return

        if should_stop_running_on_status_change(old_status=old_status, new_status=new_status):
            await self._stop_if_running(task_id)

    async def _process_spawn(self, task_id: str) -> None:
        """Handle explicit spawn requests from the UI."""
        if task_id in self._running:
            log.debug(f"Task {task_id} already running")
            return

        task = await self._tasks.get_task(task_id)
        if task is None:
            return
        if not is_auto_task(task.task_type):
            return

        max_agents = self._config.general.max_concurrent_agents
        if not can_spawn_new_agent(running_count=len(self._running), max_agents=max_agents):
            log.debug(f"At capacity ({max_agents}), task {task.id[:8]} will not start")
            return

        await self._spawn(task)

    async def _spawn(self, task: TaskLike) -> None:
        """Spawn an agent for a task."""
        title = task.title[:MODAL_TITLE_MAX_LENGTH]
        log.info(f"Spawning agent for AUTO task {task.id}: {title}")

        state = RunningTaskState()
        self._running[task.id] = state
        self._runtime_service.mark_started(task.id)
        self._check_runtime_view_consistency(task.id, phase="mark_started")
        await self._publish_runtime_event(AutomationTaskStarted(task_id=task.id))

        runner_task = asyncio.create_task(self._run_task_loop(task))
        state.task = runner_task

        runner_task.add_done_callback(self._make_done_callback(task.id))

    async def _stop_if_running(self, task_id: str) -> None:
        """Stop agent if running."""
        state = self._running.get(task_id)
        if state is None:
            return

        log.info(f"Stopping agent for task {task_id}")
        runtime_view = self._runtime_view(task_id)

        if runtime_view is not None and runtime_view.running_agent is not None:
            await runtime_view.running_agent.stop()
        if runtime_view is not None and runtime_view.review_agent is not None:
            await runtime_view.review_agent.stop()

        state.pending_respawn = False
        task_handle = state.task
        if task_handle is not None and not task_handle.done():
            task_handle.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await task_handle

        if task_id in self._running:
            await self._remove_running_state(task_id)

    def _handle_task_done(self, task_id: str, task: asyncio.Task[None]) -> None:
        """Handle agent task completion."""
        del task
        self._remove_running_state_soon(task_id)

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
        return self._runtime_service.running_tasks()

    def is_running(self, task_id: str) -> bool:
        """Check if a task is currently being processed."""
        view = self._runtime_view(task_id)
        return view.is_running if view is not None else False

    def get_running_agent(self, task_id: str) -> Agent | None:
        """Get the running agent for a task (for watch functionality).

        Returns None if the task is not running.
        May also return None during brief initialization window when task
        is running but agent hasn't been created yet.
        """
        view = self._runtime_view(task_id)
        if view is None or not view.is_running:
            return None
        return view.running_agent

    async def wait_for_running_agent(
        self,
        task_id: str,
        *,
        timeout: float = 2.0,
        interval: float = 0.05,
    ) -> Agent | None:
        """Wait for a task's running agent reference to become available.

        Returns None if the task stops running or timeout is reached.
        """
        if timeout <= 0:
            return self.get_running_agent(task_id)

        loop = asyncio.get_running_loop()
        deadline = loop.time() + timeout
        seen_running = False

        while True:
            if self.is_running(task_id):
                seen_running = True
            agent = self.get_running_agent(task_id)
            if agent is not None:
                return agent
            if seen_running and not self.is_running(task_id):
                return None
            if loop.time() >= deadline:
                return None
            await asyncio.sleep(interval)

    def get_execution_id(self, task_id: str) -> str | None:
        """Get execution id for a running task."""
        view = self._runtime_view(task_id)
        return view.execution_id if view is not None else None

    def get_run_count(self, task_id: str) -> int:
        """Get current run count for a task."""
        view = self._runtime_view(task_id)
        return view.run_count if view is not None else 0

    def is_reviewing(self, task_id: str) -> bool:
        """Check if task is currently in review phase."""
        view = self._runtime_view(task_id)
        return view.is_reviewing if view is not None else False

    def get_review_agent(self, task_id: str) -> Agent | None:
        """Get the running review agent for a task (for watch functionality)."""
        view = self._runtime_view(task_id)
        return view.review_agent if view is not None else None

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
        if not is_auto_task(task.task_type):
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
            runtime_view = self._runtime_view(task_id)
            if runtime_view is not None and runtime_view.running_agent is not None:
                await runtime_view.running_agent.stop()
            if runtime_view is not None and runtime_view.review_agent is not None:
                await runtime_view.review_agent.stop()
            state.pending_respawn = False
            if state.task is not None and not state.task.done():
                state.task.cancel()
                with contextlib.suppress(asyncio.CancelledError):
                    await state.task
            if task_id in self._running:
                await self._remove_running_state(task_id)
        await self._background_tasks.shutdown()
        if self._sessions is not None:
            shutdown = getattr(self._sessions, "shutdown", None)
            if shutdown is not None:
                await shutdown()
        self._started = False

    def _notify_task_changed(self) -> None:
        """Notify that a task has changed status."""
        if self._on_task_changed:
            self._on_task_changed()

    def _notify_error(self, task_id: str, message: str) -> None:
        """Notify that an error occurred for a task."""
        if self._on_error:
            self._on_error(task_id, message)

    async def _publish_runtime_event(self, event: DomainEvent) -> None:
        """Publish automation runtime events when event bus is configured."""
        if self._event_bus is None:
            return
        await self._event_bus.publish(event)

    def _publish_runtime_event_soon(self, event: DomainEvent) -> None:
        """Schedule runtime event publication from sync callback contexts."""
        if self._event_bus is None:
            return
        with contextlib.suppress(RuntimeError):
            self._background_tasks.spawn(self._event_bus.publish(event))

    async def _remove_running_state(self, task_id: str) -> None:
        """Remove running state and emit lifecycle-ended event once."""
        removed = self._running.pop(task_id, None)
        if removed is None:
            return
        self._runtime_service.mark_ended(task_id)
        self._check_runtime_view_consistency(task_id, phase="mark_ended")
        await self._publish_runtime_event(AutomationTaskEnded(task_id=task_id))
        if removed.pending_respawn:
            await self._event_queue.put(AutomationEvent(kind=ExecutionKind.SPAWN, task_id=task_id))

    def _remove_running_state_soon(self, task_id: str) -> None:
        """Sync variant of running-state removal for task callbacks."""
        removed = self._running.pop(task_id, None)
        if removed is None:
            return
        self._runtime_service.mark_ended(task_id)
        self._check_runtime_view_consistency(task_id, phase="mark_ended_sync")
        self._publish_runtime_event_soon(AutomationTaskEnded(task_id=task_id))
        if removed.pending_respawn:
            with contextlib.suppress(RuntimeError):
                self._background_tasks.spawn(
                    self._event_queue.put(
                        AutomationEvent(kind=ExecutionKind.SPAWN, task_id=task_id)
                    )
                )

    async def _set_running_agent(self, task_id: str, agent: Agent) -> None:
        """Attach implementation agent to runtime state and emit attach event."""
        state = self._running.get(task_id)
        if state is None:
            return
        previous = self._runtime_view(task_id)
        is_first_attach = previous is None or previous.running_agent is None
        self._runtime_service.attach_running_agent(task_id, agent)
        self._check_runtime_view_consistency(task_id, phase="attach_running_agent")
        if is_first_attach:
            await self._publish_runtime_event(AutomationAgentAttached(task_id=task_id))

    async def _set_review_agent(self, task_id: str, agent: Agent) -> None:
        """Attach review agent to runtime state and emit attach event."""
        state = self._running.get(task_id)
        if state is None:
            return
        previous = self._runtime_view(task_id)
        is_first_attach = previous is None or previous.review_agent is None
        self._runtime_service.attach_review_agent(task_id, agent)
        self._check_runtime_view_consistency(task_id, phase="attach_review_agent")
        if is_first_attach:
            await self._publish_runtime_event(AutomationReviewAgentAttached(task_id=task_id))

    async def run_review(
        self, task: TaskLike, wt_path: Path, execution_id: str
    ) -> tuple[bool, str]:
        return await self._reviewer.run_review(task, wt_path, execution_id)

    async def _handle_complete(self, task: TaskLike) -> None:
        await self._reviewer._handle_complete(task)

    async def _handle_blocked(self, task: TaskLike, reason: str) -> None:
        await self._reviewer._handle_blocked(task, reason)

    async def _run_task_loop(self, task: TaskLike) -> None:
        """Run a single execution for a task."""
        log.info(f"Starting task loop for {task.id}")
        self._notify_error(task.id, "Agent starting...")
        final_status: ExecutionStatus | None = None
        agent: Agent | None = None
        execution_id: str | None = None
        session_id: str | None = None

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
            session_id = session_record.id

            execution = await self._executions.create_execution(
                session_id=session_id,
                run_reason=ExecutionRunReason.CODINGAGENT,
                executor_action={},
            )
            execution_id = execution.id

            state = self._running.get(task.id)
            if state:
                state.session_id = session_id
            self._runtime_service.set_execution(task.id, execution_id, 0)

            user_name, user_email = await get_git_user_identity()
            log.debug(f"Git user identity: {user_name} <{user_email}>")

            agent_config = self._get_agent_config(task)
            log.debug(f"Agent config: {agent_config.name}")
            run_count = await self._executions.count_executions_for_task(task.id)
            log.info(f"Starting run for {task.id}, run={run_count}")

            self._runtime_service.set_execution(task.id, execution_id, run_count)

            signal, agent = await self._run_execution(
                task,
                wt_path,
                agent_config,
                run_count,
                execution_id,
                user_name=user_name,
                user_email=user_email,
            )

            log.debug(f"Task {task.id} run {run_count} signal: {signal}")

            if signal.signal == Signal.COMPLETE:
                final_status = ExecutionStatus.COMPLETED

                state = self._running.get(task.id)
                queued = await self._take_implementation_queue(
                    task.id,
                    session_id,
                )
                if queued is not None:
                    await self._append_queued_message_to_scratchpad(task.id, queued.content)
                    await self._update_task_status(task.id, TaskStatus.IN_PROGRESS)
                    self._notify_task_changed()
                    log.info(f"Task {task.id} has queued messages, re-spawning")
                    if state is not None:
                        state.pending_respawn = True
                    else:
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
            if self._executions and execution_id is not None:
                with contextlib.suppress(RepositoryClosing):
                    await self._executions.update_execution(
                        execution_id,
                        status=final_status or ExecutionStatus.FAILED,
                        completed_at=utc_now(),
                    )
            if session_id is not None:
                with contextlib.suppress(RepositoryClosing):
                    await self._tasks.close_session_record(session_id, status=SessionStatus.CLOSED)
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

        await self._set_running_agent(task.id, agent)

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
            await self._executions.append_execution_log(execution_id, serialized_output)
            await self._executions.append_agent_turn(
                execution_id,
                prompt=prompt,
                summary=response,
            )

        progress_note = f"\n\n--- Run {run_count} ---\n{response[-2000:]}"
        await self._tasks.update_scratchpad(task.id, scratchpad + progress_note)

        return (signal_result, agent)


def _truncate_queue_payload(content: str, max_chars: int = 8000) -> str:
    """Trim queued messages to preserve prompt budget."""
    if len(content) <= max_chars:
        return content
    prefix = "[queued context truncated]\n"
    return f"{prefix}{content[-(max_chars - len(prefix)) :]}"
