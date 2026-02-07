"""Reactive automation service for AUTO task execution."""

from __future__ import annotations

import asyncio
import contextlib
import weakref
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal, Protocol

from kagan.agents.agent_factory import AgentFactory, create_agent
from kagan.agents.output import build_merge_conflict_note, serialize_agent_output
from kagan.agents.prompt import build_prompt
from kagan.agents.prompt_loader import get_review_prompt
from kagan.agents.signals import Signal, SignalResult, parse_signal
from kagan.constants import MODAL_TITLE_MAX_LENGTH
from kagan.core.events import DomainEvent, EventBus, TaskStatusChanged
from kagan.core.models.enums import MergeReadiness, TaskStatus, TaskType
from kagan.debug_log import log
from kagan.git_utils import get_git_user_identity
from kagan.limits import AGENT_TIMEOUT_LONG

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from kagan.acp.agent import Agent
    from kagan.config import AgentConfig, KaganConfig
    from kagan.services.merges import MergeService
    from kagan.services.sessions import SessionServiceImpl
    from kagan.services.tasks import TaskService
    from kagan.services.types import TaskLike
    from kagan.services.workspaces import WorkspaceService


class AutomationService(Protocol):
    """Reactive automation service that responds to domain events."""

    async def start(self) -> None:
        """Start background automation tasks."""

    async def stop(self) -> None:
        """Stop background automation tasks."""

    async def handle_event(self, event: DomainEvent) -> None:
        """Process a domain event and trigger actions."""

    async def initialize_existing_tasks(self) -> None:
        """Spawn agents for existing in-progress AUTO tasks."""

    @property
    def running_tasks(self) -> set[str]:
        """Return the IDs of tasks currently running."""

    def is_running(self, task_id: str) -> bool:
        """Return True if the task is currently running."""

    def get_iteration_count(self, task_id: str) -> int:
        """Return the current iteration count for a task."""

    def get_running_agent(self, task_id: str) -> Agent | None:
        """Return the running agent for a task, if any."""

    def is_reviewing(self, task_id: str) -> bool:
        """Return True if the task is currently in review phase."""

    def get_review_agent(self, task_id: str) -> Agent | None:
        """Return the running review agent for a task, if any."""

    async def stop_task(self, task_id: str) -> bool:
        """Stop automation for a task and return success."""

    async def spawn_for_task(self, task: TaskLike) -> bool:
        """Spawn automation for a task."""


@dataclass(slots=True)
class RunningTaskState:
    """State for a currently running task."""

    task: asyncio.Task[None] | None = None
    agent: Agent | None = None
    iteration: int = 0
    review_agent: Agent | None = None  # Review agent for watching
    is_reviewing: bool = False  # Currently in review phase


class AutomationServiceImpl:
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
        session_service: SessionServiceImpl | None = None,
        merge_service: MergeService | None = None,
        on_task_changed: Callable[[], None] | None = None,
        on_iteration_changed: Callable[[str, int], None] | None = None,
        on_error: Callable[[str, str], None] | None = None,
        notifier: Callable[[str, str, Literal["information", "warning", "error"]], None]
        | None = None,
        agent_factory: AgentFactory = create_agent,
        event_bus: EventBus | None = None,
    ) -> None:
        self._tasks = task_service
        self._workspaces = workspace_service
        self._config = config
        self._sessions = session_service
        self._merge_service = merge_service
        self._running: dict[str, RunningTaskState] = {}
        self._on_task_changed = on_task_changed
        self._on_iteration_changed = on_iteration_changed
        self._on_error = on_error
        self._notifier = notifier
        self._agent_factory = agent_factory
        self._event_bus = event_bus

        # Event queue for reactive processing
        self._event_queue: asyncio.Queue[tuple[str, TaskStatus | None, TaskStatus | None]] = (
            asyncio.Queue()
        )
        self._worker_task: asyncio.Task[None] | None = None
        self._event_task: asyncio.Task[None] | None = None
        self._started = False

        # Lock to serialize merge operations (prevents race conditions when
        # multiple tasks complete around the same time)
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
            await self._event_queue.put((event.task_id, event.from_status, event.to_status))

    async def _event_loop(self) -> None:
        """Subscribe to domain events and enqueue relevant automation work."""
        assert self._event_bus is not None
        async for event in self._event_bus.subscribe(TaskStatusChanged):
            await self.handle_event(event)

    async def initialize_existing_tasks(self) -> None:
        """Spawn agents for existing IN_PROGRESS AUTO tasks.

        Called on startup to handle tasks that were already in progress
        before the automation service started listening for changes.
        Only runs if auto_start is enabled in config.
        """
        if not self._config.general.auto_start:
            log.info("auto_start disabled, skipping initialization of existing tasks")
            return

        tasks = await self._tasks.get_by_status(TaskStatus.IN_PROGRESS)
        for task in tasks:
            if task.task_type == TaskType.AUTO:
                log.info(f"Queueing existing IN_PROGRESS task: {task.id}")
                await self._event_queue.put((task.id, None, TaskStatus.IN_PROGRESS))

    async def handle_status_change(
        self, task_id: str, old_status: TaskStatus | None, new_status: TaskStatus | None
    ) -> None:
        """Handle a task status change event.

        Called by TaskService when task status changes.
        Queues the event for processing by the worker loop.
        """
        await self._event_queue.put((task_id, old_status, new_status))
        log.debug(f"Queued status change: {task_id} {old_status} -> {new_status}")

    async def _worker_loop(self) -> None:
        """Single worker that processes all events sequentially.

        This eliminates race conditions because all spawn/stop decisions
        happen in one place, one at a time.
        """
        log.info("Automation worker loop started")
        while True:
            try:
                task_id, old_status, new_status = await self._event_queue.get()
                await self._process_event(task_id, old_status, new_status)
            except asyncio.CancelledError:
                log.info("Automation worker loop cancelled")
                break
            except Exception as e:
                log.error(f"Error in automation worker: {e}")

    async def _process_event(
        self, task_id: str, old_status: TaskStatus | None, new_status: TaskStatus | None
    ) -> None:
        """Process a single status change event."""
        # Task deleted
        if new_status is None:
            await self._stop_if_running(task_id)
            return

        # Get full task to check type
        task = await self._tasks.get_task(task_id)
        if task is None:
            await self._stop_if_running(task_id)
            return

        # Only handle AUTO tasks
        if task.task_type != TaskType.AUTO:
            return

        # React to status
        if new_status == TaskStatus.IN_PROGRESS:
            await self._ensure_running(task)
        elif old_status == TaskStatus.IN_PROGRESS and new_status != TaskStatus.REVIEW:
            # Moved OUT of IN_PROGRESS to non-REVIEW status - stop if running
            # Don't stop for REVIEW transitions as that's part of normal completion flow
            await self._stop_if_running(task_id)

    async def _ensure_running(self, task: TaskLike) -> None:
        """Ensure an agent is running for this task."""
        if task.id in self._running:
            log.debug(f"Task {task.id} already running")
            return

        max_agents = self._config.general.max_concurrent_agents
        if len(self._running) >= max_agents:
            log.debug(
                f"At capacity ({max_agents}), task {task.id[:8]} will start when capacity frees"
            )
            return  # Don't re-queue - will be checked when capacity frees

        await self._spawn(task)

    async def _spawn(self, task: TaskLike) -> None:
        """Spawn an agent for a task. Called only from worker loop."""
        title = task.title[:MODAL_TITLE_MAX_LENGTH]
        log.info(f"Spawning agent for AUTO task {task.id}: {title}")

        # Reset review state from previous attempts
        await self._tasks.update_fields(
            task.id,
            checks_passed=None,
            review_summary=None,
            merge_failed=False,
            merge_error=None,
            last_error=None,
            block_reason=None,
        )

        # Clear previous agent logs for fresh retry
        await self._tasks.clear_agent_logs(task.id)

        # Add to _running BEFORE creating task to avoid race condition
        # where task checks _running before we've added the entry
        state = RunningTaskState()
        self._running[task.id] = state

        runner_task = asyncio.create_task(self._run_task_loop(task))
        state.task = runner_task

        runner_task.add_done_callback(self._make_done_callback(task.id))

    async def _stop_if_running(self, task_id: str) -> None:
        """Stop agent if running. Called only from worker loop."""
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

        if self._on_iteration_changed:
            self._on_iteration_changed(task_id, 0)

        # After freeing capacity, check for waiting IN_PROGRESS AUTO tasks
        asyncio.create_task(self._check_waiting_tasks())

    async def _check_waiting_tasks(self) -> None:
        """Check if any IN_PROGRESS AUTO tasks are waiting to start."""
        max_agents = self._config.general.max_concurrent_agents
        if len(self._running) >= max_agents:
            return

        # Get all tasks
        tasks = await self._tasks.list_tasks()
        for task in tasks:
            if (
                task.status == TaskStatus.IN_PROGRESS
                and task.task_type == TaskType.AUTO
                and task.id not in self._running
            ):
                # Trigger check for this task
                await self._event_queue.put((task.id, None, TaskStatus.IN_PROGRESS))
                return  # Only queue one at a time

    def _handle_task_done(self, task_id: str, task: asyncio.Task[None]) -> None:
        """Handle agent task completion."""
        self._running.pop(task_id, None)
        if self._on_iteration_changed:
            self._on_iteration_changed(task_id, 0)

    def _make_done_callback(self, task_id: str) -> Callable[[asyncio.Task[None]], None]:
        """Create task done callback with weak self reference."""
        weak_self = weakref.ref(self)

        def on_done(task: asyncio.Task[None]) -> None:
            service = weak_self()
            if service is not None:
                service._handle_task_done(task_id, task)

        return on_done

    # --- Public API (thread-safe via queue) ---

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

    def get_iteration_count(self, task_id: str) -> int:
        """Get current iteration count for a task."""
        state = self._running.get(task_id)
        return state.iteration if state else 0

    def reset_iterations(self, task_id: str) -> None:
        """Reset the session iteration counter for a task.

        This resets the in-memory "leash" counter used for the current session,
        not the lifetime total_iterations stored in the database.
        Called when a task is rejected and retried.
        """
        state = self._running.get(task_id)
        if state is not None:
            log.info(f"Resetting session iteration counter for task {task_id}")
            state.iteration = 0
            if self._on_iteration_changed:
                self._on_iteration_changed(task_id, 0)
        else:
            log.debug(f"Cannot reset iterations for {task_id}: not running")

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
        # Queue a "moved out of IN_PROGRESS" event
        await self._event_queue.put((task_id, TaskStatus.IN_PROGRESS, TaskStatus.BACKLOG))
        return True

    async def spawn_for_task(self, task: TaskLike) -> bool:
        """Manually request to spawn an agent for a task.

        Used by UI for manual agent starts. Returns True if spawn was queued.
        """
        if task.id in self._running:
            return False  # Already running
        if task.task_type != TaskType.AUTO:
            return False  # Only AUTO tasks

        # Queue a spawn event
        await self._event_queue.put((task.id, None, TaskStatus.IN_PROGRESS))
        return True

    async def stop(self) -> None:
        """Stop the automation service and all running agents."""
        log.info("Stopping automation service")

        # Stop event subscription loop
        if self._event_task and not self._event_task.done():
            self._event_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._event_task

        # Stop worker loop
        if self._worker_task and not self._worker_task.done():
            self._worker_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._worker_task

        # Stop all agents
        for task_id, state in list(self._running.items()):
            log.info(f"Stopping agent for task {task_id}")
            if state.agent is not None:
                await state.agent.stop()
            if state.task is not None and not state.task.done():
                state.task.cancel()

        self._running.clear()
        self._started = False

    # --- Internal: task processing loop ---

    def _notify_task_changed(self) -> None:
        """Notify that a task has changed status."""
        if self._on_task_changed:
            self._on_task_changed()

    def _notify_error(self, task_id: str, message: str) -> None:
        """Notify that an error occurred for a task."""
        if self._on_error:
            self._on_error(task_id, message)
        # Persist error to database
        _ = asyncio.create_task(self._tasks.update_fields(task_id, last_error=message[:500]))

    async def _run_task_loop(self, task: TaskLike) -> None:
        """Run the iterative loop for a task until completion."""
        log.info(f"Starting task loop for {task.id}")
        self._notify_error(task.id, "Agent starting...")

        try:
            # Ensure worktree exists
            wt_path = await self._workspaces.get_path(task.id)
            if wt_path is None:
                log.info(f"Creating worktree for {task.id}")
                try:
                    wt_path = await self._workspaces.create(
                        task.id, base_branch=self._config.general.default_base_branch
                    )
                except ValueError as exc:
                    # Common errors: no repos, project not found
                    error_msg = str(exc)
                    log.error(f"Workspace creation failed for task {task.id}: {error_msg}")
                    self._notify_error(task.id, error_msg)
                    self._notify_user(
                        f"❌ {error_msg}",
                        title="Cannot Start Agent",
                        severity="error",
                    )
                    await self._update_task_status(task.id, TaskStatus.BACKLOG)
                    return
                except Exception as exc:
                    # Check for common git-related errors
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
                        severity="error",
                    )
                    await self._update_task_status(task.id, TaskStatus.BACKLOG)
                    return
            log.info(f"Worktree path: {wt_path}")

            # Get git user identity for Co-authored-by attribution
            user_name, user_email = await get_git_user_identity()
            log.debug(f"Git user identity: {user_name} <{user_email}>")

            # Get agent config
            agent_config = self._get_agent_config(task)
            log.debug(f"Agent config: {agent_config.name}")
            max_iterations = self._config.general.max_iterations
            log.info(f"Starting iterations for {task.id}, max={max_iterations}")

            agent: Agent | None = None

            for iteration in range(1, max_iterations + 1):
                # Increment lifetime total_iterations in database
                await self._tasks.increment_total_iterations(task.id)

                # Update iteration count in running state
                state = self._running.get(task.id)
                if state:
                    state.iteration = iteration
                if self._on_iteration_changed:
                    self._on_iteration_changed(task.id, iteration)
                log.debug(f"Task {task.id} iteration {iteration}/{max_iterations}")

                signal, agent = await self._run_iteration(
                    task,
                    wt_path,
                    agent_config,
                    iteration,
                    max_iterations,
                    agent=agent,
                    user_name=user_name,
                    user_email=user_email,
                )

                # Update agent in running state
                if agent is not None:
                    state = self._running.get(task.id)
                    if state:
                        state.agent = agent

                log.debug(f"Task {task.id} iteration {iteration} signal: {signal}")

                if signal.signal == Signal.COMPLETE:
                    log.info(f"Task {task.id} completed, moving to REVIEW")
                    await self._handle_complete(task)
                    return
                elif signal.signal == Signal.BLOCKED:
                    log.warning(f"Task {task.id} blocked: {signal.reason}")
                    self._notify_error(task.id, f"Blocked: {signal.reason}")
                    await self._handle_blocked(task, signal.reason)
                    return

                await asyncio.sleep(self._config.general.iteration_delay_seconds)

            log.warning(f"Task {task.id} reached max iterations")
            self._notify_error(task.id, "Reached max iterations without completing")
            await self._handle_max_iterations(task)

        except asyncio.CancelledError:
            log.info(f"Task {task.id} cancelled")
            raise
        except Exception as e:
            import traceback

            tb = traceback.format_exc()
            log.error(f"Exception in task loop for {task.id}: {e}")
            log.error(f"Traceback:\n{tb}")
            self._notify_error(task.id, f"Agent failed: {e}")
            await self._update_task_status(task.id, TaskStatus.BACKLOG)
        finally:
            log.info(f"Task loop ended for {task.id}")

    def _get_agent_config(self, task: TaskLike) -> AgentConfig:
        """Get agent config for a task."""
        return task.get_agent_config(self._config)

    def _notify_user(
        self, message: str, title: str, severity: Literal["information", "warning", "error"]
    ) -> None:
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
        # Inline model resolution
        model = None
        if "claude" in agent_config.identity.lower():
            model = self._config.general.default_model_claude
        elif "opencode" in agent_config.identity.lower():
            model = self._config.general.default_model_opencode

        if model:
            agent.set_model_override(model)
            log.info(f"Applied model override for {context}: {model}")

    async def _auto_merge(self, task: TaskLike) -> None:
        """Auto-merge task to main and move to DONE.

        Only called when auto_merge config is enabled.
        If merge fails due to conflict and auto_retry_on_merge_conflict is also enabled,
        attempts to rebase the branch and retry the task from IN_PROGRESS.

        Uses a lock to serialize merge operations, preventing race conditions when
        multiple tasks complete around the same time.
        """
        async with self._merge_lock:
            log.info(f"Acquired merge lock for task {task.id}")
            if self._merge_service is None:
                log.warning("Auto-merge requested but merge service is not configured")
                await self._tasks.update_fields(
                    task.id,
                    merge_failed=True,
                    merge_error="Auto-merge unavailable: merge service not configured",
                    merge_readiness=MergeReadiness.BLOCKED,
                )
                await self._tasks.append_event(
                    task.id,
                    "merge",
                    "Auto-merge unavailable: merge service not configured",
                )
                self._notify_task_changed()
                return
            base = self._config.general.default_base_branch
            success, message = await self._merge_service.merge_task(task)

            if success:
                log.info(f"Auto-merged task {task.id}: {task.title}")
                await self._tasks.append_event(task.id, "merge", f"Auto-merged to {base}")
            else:
                # Check if this is a merge conflict and auto-retry is enabled
                is_conflict = "conflict" in message.lower()
                should_retry = is_conflict and self._config.general.auto_retry_on_merge_conflict

                if should_retry:
                    log.info(f"Merge conflict for {task.id}, attempting rebase and retry")
                    await self._tasks.append_event(
                        task.id, "merge", f"Auto-merge conflict: {message}"
                    )
                    await self._handle_merge_conflict_retry(task, base, message)
                else:
                    # Standard failure handling - stay in REVIEW with error
                    log.warning(f"Auto-merge failed for {task.id}: {message}")
                    await self._tasks.update_fields(
                        task.id,
                        merge_failed=True,
                        merge_error=message[:500] if message else "Unknown error",
                        merge_readiness=MergeReadiness.BLOCKED,
                    )
                    await self._tasks.append_event(
                        task.id, "merge", f"Auto-merge failed: {message}"
                    )
                    self._notify_user(
                        f"⚠ Merge failed: {message[:50]}",
                        title="Merge Error",
                        severity="error",
                    )

            self._notify_task_changed()
            log.info(f"Released merge lock for task {task.id}")

    async def _handle_merge_conflict_retry(
        self, task: TaskLike, base_branch: str, original_error: str
    ) -> None:
        """Handle merge conflict by rebasing and sending task back to IN_PROGRESS.

        This gives the agent a chance to resolve conflicts after rebasing onto
        the latest base branch.
        """
        wt_path = await self._workspaces.get_path(task.id)
        if wt_path is None:
            log.error(f"Cannot retry {task.id}: worktree not found")
            await self._tasks.update_fields(
                task.id,
                merge_failed=True,
                merge_error="Worktree not found for conflict retry",
            )
            return

        # Get info about what changed on base branch (for context)
        files_on_base = await self._workspaces.get_files_changed_on_base(task.id, base_branch)

        # Attempt to rebase onto latest base branch
        rebase_success, rebase_msg, conflict_files = await self._workspaces.rebase_onto_base(
            task.id, base_branch
        )

        if not rebase_success and conflict_files:
            # Rebase had conflicts - this is expected, the agent needs to fix them
            # The rebase was aborted, so we'll let the agent handle it manually
            log.info(f"Rebase conflict for {task.id}, agent will resolve: {conflict_files}")

        # Build detailed context for the scratchpad
        scratchpad = await self._tasks.get_scratchpad(task.id)
        conflict_note = build_merge_conflict_note(
            original_error=original_error,
            rebase_success=rebase_success,
            rebase_msg=rebase_msg,
            conflict_files=conflict_files,
            files_on_base=files_on_base,
            base_branch=base_branch,
        )
        await self._tasks.update_scratchpad(task.id, scratchpad + conflict_note)

        # Clear the review state since we're retrying
        await self._tasks.update_fields(
            task.id,
            status=TaskStatus.IN_PROGRESS,
            checks_passed=None,
            review_summary=None,
            merge_failed=False,
            merge_error=None,
            merge_readiness=MergeReadiness.RISK,
        )
        await self._tasks.append_event(
            task.id, "merge", "Merge conflict retry: moved back to IN_PROGRESS"
        )

        # Notify user about the retry
        self._notify_user(
            f"🔄 Merge conflict - retrying: {task.title[:30]}",
            title="Auto-Retry",
            severity="warning",
        )

        log.info(f"Task {task.id} sent back to IN_PROGRESS for merge conflict resolution")

        # Queue the task for processing (it's now IN_PROGRESS again)
        await self._event_queue.put((task.id, TaskStatus.REVIEW, TaskStatus.IN_PROGRESS))

    async def _update_task_status(self, task_id: str, status: TaskStatus) -> None:
        """Update task status."""
        await self._tasks.update_fields(task_id, status=status)

    # --- Methods merged from TaskRunner ---

    async def run_review(self, task: TaskLike, wt_path: Path) -> tuple[bool, str]:
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

        # Apply model override for review
        self._apply_model_override(agent, agent_config, f"review of task {task.id}")

        agent.start()

        try:
            await agent.wait_ready(timeout=AGENT_TIMEOUT_LONG)
            await agent.send_prompt(prompt)
            response = agent.get_response_text()

            # Persist review logs
            serialized_output = serialize_agent_output(agent)
            await self._tasks.append_agent_log(task.id, "review", 1, serialized_output)

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
            await agent.stop()

    async def _run_iteration(
        self,
        task: TaskLike,
        wt_path: Path,
        agent_config: AgentConfig,
        iteration: int,
        max_iterations: int,
        agent: Agent | None = None,
        user_name: str = "Developer",
        user_email: str = "developer@localhost",
    ) -> tuple[SignalResult, Agent | None]:
        """Run a single iteration for a task.

        Returns:
            Tuple of (signal_result, agent) where agent is the created/reused agent.
        """
        # Get or create agent
        if agent is None:
            agent = self._agent_factory(wt_path, agent_config)
            agent.set_auto_approve(self._config.general.auto_approve)

            # Apply model override
            self._apply_model_override(agent, agent_config, f"task {task.id}")

            agent.start()

            # Expose the agent immediately so watch mode can attach during startup.
            state = self._running.get(task.id)
            if state:
                state.agent = agent

            try:
                await agent.wait_ready(timeout=AGENT_TIMEOUT_LONG)
            except TimeoutError:
                log.error(f"Agent timeout for task {task.id}")
                return (parse_signal('<blocked reason="Agent failed to start"/>'), None)
        else:
            # Re-sync auto_approve from config
            agent.set_auto_approve(self._config.general.auto_approve)

        # Build prompt with scratchpad context
        scratchpad = await self._tasks.get_scratchpad(task.id)
        prompt = build_prompt(
            task=task,
            iteration=iteration,
            max_iterations=max_iterations,
            scratchpad=scratchpad,
            user_name=user_name,
            user_email=user_email,
        )

        # Send prompt and get response
        log.info(f"Sending prompt to agent for task {task.id}, iteration {iteration}")
        try:
            await agent.send_prompt(prompt)
        except Exception as e:
            log.error(f"Agent prompt failed for {task.id}: {e}")
            return (parse_signal(f'<blocked reason="Agent error: {e}"/>'), agent)
        finally:
            # Clear tool calls to prevent memory accumulation
            agent.clear_tool_calls()

        # Get response and parse signal
        response = agent.get_response_text()
        signal_result = parse_signal(response)

        # Persist FULL agent output as JSON
        serialized_output = serialize_agent_output(agent)
        await self._tasks.append_agent_log(task.id, "implementation", iteration, serialized_output)

        # Update scratchpad with progress
        progress_note = f"\n\n--- Iteration {iteration} ---\n{response[-2000:]}"
        await self._tasks.update_scratchpad(task.id, scratchpad + progress_note)

        return (signal_result, agent)

    async def _handle_complete(self, task: TaskLike) -> None:
        """Handle task completion - move to REVIEW immediately, then run review."""
        # Move to REVIEW status IMMEDIATELY
        await self._tasks.update_fields(
            task.id,
            status=TaskStatus.REVIEW,
            merge_failed=False,
            merge_error=None,
            merge_readiness=MergeReadiness.RISK,
        )
        self._notify_task_changed()

        wt_path = await self._workspaces.get_path(task.id)
        checks_passed = False
        review_summary = ""

        if wt_path is not None:
            checks_passed, review_summary = await self.run_review(task, wt_path)

            status = "approved" if checks_passed else "rejected"
            log.info(f"Task {task.id} review: {status}")

            # Emit toast notification for review result
            if checks_passed:
                self._notify_user(
                    f"✓ Review passed: {task.title[:30]}",
                    title="Review Complete",
                    severity="information",
                )
            else:
                self._notify_user(
                    f"✗ Review failed: {review_summary[:50]}",
                    title="Review Complete",
                    severity="warning",
                )

        # Update task with review results
        review_updates = {
            "checks_passed": checks_passed,
            "review_summary": review_summary,
        }
        if not checks_passed:
            review_updates["merge_readiness"] = MergeReadiness.BLOCKED
        await self._tasks.update_fields(task.id, **review_updates)
        self._notify_task_changed()
        review_event = "Review passed" if checks_passed else f"Review failed: {review_summary}"
        await self._tasks.append_event(task.id, "review", review_event[:200])

        # Auto-merge if enabled and review passed
        if checks_passed and self._config.general.auto_merge:
            log.info(f"Auto-merge enabled, merging task {task.id}")
            await self._auto_merge(task)

    async def _handle_blocked(self, task: TaskLike, reason: str) -> None:
        """Handle blocked task - move back to BACKLOG with reason."""
        scratchpad = await self._tasks.get_scratchpad(task.id)
        block_note = f"\n\n--- BLOCKED ---\nReason: {reason}\n"
        await self._tasks.update_scratchpad(task.id, scratchpad + block_note)

        await self._tasks.update_fields(
            task.id, status=TaskStatus.BACKLOG, block_reason=reason[:500]
        )
        self._notify_task_changed()

    async def _handle_max_iterations(self, task: TaskLike) -> None:
        """Handle task that reached max iterations."""
        scratchpad = await self._tasks.get_scratchpad(task.id)
        max_iter_note = (
            f"\n\n--- MAX ITERATIONS ---\n"
            f"Reached {self._config.general.max_iterations} iterations without completion.\n"
        )
        await self._tasks.update_scratchpad(task.id, scratchpad + max_iter_note)

        await self._update_task_status(task.id, TaskStatus.BACKLOG)
        self._notify_task_changed()

    async def _build_review_prompt(self, task: TaskLike) -> str:
        """Build review prompt from template with commits and diff."""
        base = self._config.general.default_base_branch
        commits = await self._workspaces.get_commit_log(task.id, base)
        diff_summary = await self._workspaces.get_diff_stats(task.id, base)

        return get_review_prompt(
            title=task.title,
            task_id=task.id,
            description=task.description or "",
            commits="\n".join(f"- {c}" for c in commits) if commits else "No commits",
            diff_summary=diff_summary or "No changes",
        )
