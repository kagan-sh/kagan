"""AutomationEngine: high-level lifecycle orchestration for AUTO tasks."""

from __future__ import annotations

import asyncio
import contextlib
import re
import weakref
from collections import deque
from dataclasses import dataclass
from typing import TYPE_CHECKING

from kagan.core.adapters.db.repositories.base import RepositoryClosing
from kagan.core.agents.output import serialize_agent_messages, serialize_agent_output
from kagan.core.agents.prompt_builders import build_prompt, get_review_prompt
from kagan.core.agents.signals import Signal, SignalResult, parse_signal
from kagan.core.constants import MODAL_TITLE_MAX_LENGTH
from kagan.core.debug_log import log
from kagan.core.events import (
    AutomationAgentAttached,
    AutomationReviewAgentAttached,
    AutomationTaskEnded,
    AutomationTaskStarted,
    DomainEvent,
    EventBus,
    TaskStatusChanged,
)
from kagan.core.git_utils import get_git_user_identity
from kagan.core.limits import AGENT_TIMEOUT_LONG
from kagan.core.models.enums import (
    ExecutionRunReason,
    ExecutionStatus,
    NotificationSeverity,
    SessionStatus,
    SessionType,
    TaskStatus,
    TaskType,
)
from kagan.core.services.permission_policy import AgentPermissionScope, resolve_auto_approve
from kagan.core.services.queued_messages import QueuedMessageServiceImpl
from kagan.core.time import utc_now
from kagan.core.utils import BackgroundTasks, truncate_queue_payload

if TYPE_CHECKING:
    from collections.abc import Awaitable, Callable
    from datetime import datetime
    from pathlib import Path

    from kagan.core.acp import Agent
    from kagan.core.adapters.db.repositories import ExecutionRepository
    from kagan.core.adapters.git.operations import GitOperationsProtocol
    from kagan.core.agents.agent_factory import AgentFactory
    from kagan.core.config import AgentConfig, KaganConfig
    from kagan.core.services.queued_messages import QueuedMessage, QueuedMessageService
    from kagan.core.services.runtime import RuntimeService, RuntimeTaskView
    from kagan.core.services.sessions import SessionService
    from kagan.core.services.tasks import TaskService
    from kagan.core.services.types import TaskLike
    from kagan.core.services.workspaces import WorkspaceService

# ---------------------------------------------------------------------------
# State dataclasses
# ---------------------------------------------------------------------------


@dataclass(slots=True)
class RunningTaskState:
    """Lifecycle state for a currently running task loop."""

    task: asyncio.Task[None] | None = None
    session_id: str | None = None
    pending_respawn: bool = False


@dataclass(slots=True)
class BlockedSpawnState:
    """Scheduler metadata for conflict-blocked AUTO task starts."""

    task_id: str
    blocker_task_ids: tuple[str, ...]
    overlap_hints: tuple[str, ...]
    reason: str
    blocked_at: datetime


@dataclass(slots=True)
class AutomationEvent:
    """Queue item for automation status worker."""

    task_id: str
    old_status: TaskStatus | None = None
    new_status: TaskStatus | None = None


# ---------------------------------------------------------------------------
# Policy helpers
# ---------------------------------------------------------------------------

_PATH_HINT_RE = re.compile(r"[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)+")
_FILE_HINT_RE = re.compile(r"[A-Za-z0-9_.-]+\.[A-Za-z0-9]{1,8}")
_WORD_RE = re.compile(r"[A-Za-z0-9_]+")
_STREAM_LOG_FLUSH_INTERVAL_SECONDS = 0.25

_KEYWORD_HINTS: dict[str, str] = {
    "test": "tests/**",
    "tests": "tests/**",
    "pytest": "tests/**",
    "readme": "README.md",
    "docs": "docs/**",
    "config": "config/**",
    "pyproject": "pyproject.toml",
    "docker": "Dockerfile",
}


@dataclass(frozen=True, slots=True)
class ConflictAssessment:
    """Conflict decision for a candidate spawn against running tasks."""

    blocker_task_ids: tuple[str, ...]
    overlap_hints: tuple[str, ...]

    @property
    def is_blocked(self) -> bool:
        return bool(self.blocker_task_ids)


def is_auto_task(task_type: TaskType) -> bool:
    """Return whether a task type is eligible for automation."""
    return task_type is TaskType.AUTO


def should_stop_running_on_status_change(
    *,
    old_status: TaskStatus | None,
    new_status: TaskStatus | None,
) -> bool:
    """Return whether a running AUTO task should be stopped on status transition."""
    if old_status is TaskStatus.IN_PROGRESS:
        return new_status is not TaskStatus.REVIEW
    if old_status is TaskStatus.REVIEW:
        return new_status is not TaskStatus.REVIEW
    return False


def can_spawn_new_agent(*, running_count: int, max_agents: int) -> bool:
    """Return whether runner has capacity for another AUTO task."""
    return running_count < max_agents


def derive_conflict_hints(task: TaskLike) -> tuple[str, ...]:
    """Derive deterministic conflict hints from task text."""
    joined = "\n".join(
        part.strip()
        for part in (
            task.title,
            task.description or "",
            " ".join(task.acceptance_criteria or []),
        )
        if part and part.strip()
    )
    if not joined:
        return ()

    normalized = joined.replace("`", " ")
    hints: set[str] = set()

    for match in _PATH_HINT_RE.findall(normalized):
        hints.add(match.strip("./"))
    for match in _FILE_HINT_RE.findall(normalized):
        hints.add(match.strip("./"))

    words = {word.lower() for word in _WORD_RE.findall(normalized)}
    for word in words:
        keyword_hint = _KEYWORD_HINTS.get(word)
        if keyword_hint:
            hints.add(keyword_hint)

    return tuple(sorted(hints))


def assess_conflict(
    candidate: TaskLike,
    running_tasks: dict[str, TaskLike],
) -> ConflictAssessment:
    """Assess whether candidate should be blocked by running tasks."""
    if not running_tasks:
        return ConflictAssessment((), ())

    candidate_hints = set(derive_conflict_hints(candidate))
    if not candidate_hints:
        return ConflictAssessment((), ())

    blockers: list[str] = []
    overlaps: set[str] = set()
    for task_id, running in running_tasks.items():
        running_hints = set(derive_conflict_hints(running))
        overlap = candidate_hints & running_hints
        if not overlap:
            continue
        blockers.append(task_id)
        overlaps.update(overlap)

    return ConflictAssessment(tuple(blockers), tuple(sorted(overlaps)))


# ---------------------------------------------------------------------------
# AutomationReviewer
# ---------------------------------------------------------------------------


class AutomationReviewer:
    def __init__(
        self,
        *,
        task_service: TaskService,
        workspace_service: WorkspaceService,
        config: KaganConfig,
        execution_service: ExecutionRepository | None,
        notifier: Callable[[str, str, NotificationSeverity], None] | None,
        agent_factory: AgentFactory,
        git_adapter: GitOperationsProtocol | None,
        runtime_service: RuntimeService,
        get_agent_config: Callable[[TaskLike], AgentConfig],
        apply_model_override: Callable[[Agent, AgentConfig, str], None],
        set_review_agent: Callable[[str, Agent], Awaitable[None]],
        notify_task_changed: Callable[[], None],
    ) -> None:
        self._tasks = task_service
        self._workspaces = workspace_service
        self._config = config
        self._executions = execution_service
        self._notifier = notifier
        self._agent_factory = agent_factory
        self._git = git_adapter
        self._runtime_service = runtime_service

        self._get_agent_config = get_agent_config
        self._apply_model_override = apply_model_override
        self._set_review_agent = set_review_agent
        self._notify_task_changed = notify_task_changed

    def _notify_user(self, message: str, title: str, severity: NotificationSeverity) -> None:
        if self._notifier is not None:
            self._notifier(message, title, severity)

    async def run_review(
        self, task: TaskLike, wt_path: Path, execution_id: str
    ) -> tuple[bool, str]:
        """Run agent-based review and return (passed, summary)."""
        agent_config = self._get_agent_config(task)
        prompt = await self._build_review_prompt(task)

        agent = self._agent_factory(wt_path, agent_config, read_only=True)
        maybe_set_task_id = getattr(agent, "set_task_id", None)
        if callable(maybe_set_task_id):
            maybe_set_task_id(task.id)
        auto_approve = resolve_auto_approve(
            scope=AgentPermissionScope.AUTOMATION_REVIEWER,
            planner_auto_approve=self._config.general.auto_approve,
        )
        agent.set_auto_approve(auto_approve)

        self._apply_model_override(agent, agent_config, f"review of task {task.id}")

        agent.start()

        await self._set_review_agent(task.id, agent)

        try:
            await agent.wait_ready(timeout=AGENT_TIMEOUT_LONG)
            await agent.send_prompt(prompt)
            response = agent.get_response_text()

            serialized_output = serialize_agent_output(agent)
            if self._executions is not None:
                await self._executions.append_execution_log(execution_id, serialized_output)
                await self._executions.append_agent_turn(
                    execution_id,
                    prompt=prompt,
                    summary=response,
                )

            signal = parse_signal(response)
            if signal.signal == Signal.APPROVE:
                return True, signal.reason
            if signal.signal == Signal.REJECT:
                return False, signal.reason
            return False, "No review signal found in agent response"
        except TimeoutError:
            log.error(f"Review agent timeout for task {task.id}")
            return False, "Review agent timed out"
        except Exception as e:
            log.error(f"Review agent failed for {task.id}: {e}")
            return False, f"Review agent error: {e}"
        finally:
            self._runtime_service.clear_review_agent(task.id)
            await agent.stop()

    async def _handle_complete(self, task: TaskLike) -> None:
        """Handle completion: move to REVIEW then run review if enabled."""
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
        runtime_view = self._runtime_service.get(task.id)
        if runtime_view is not None:
            execution_id = runtime_view.execution_id

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
                "completed_at": utc_now().isoformat(),
            }
            await self._executions.update_execution(
                execution_id,
                metadata={"review_result": review_result},
            )

    async def _handle_blocked(self, task: TaskLike, reason: str) -> None:
        """Handle blocked task by moving it back to backlog with context."""
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


# ---------------------------------------------------------------------------
# AutomationEngine
# ---------------------------------------------------------------------------


class AutomationEngine:
    """Core automation engine that orchestrates AUTO task lifecycle."""

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
        self._queued = queued_message_service or QueuedMessageServiceImpl()
        self._running: dict[str, RunningTaskState] = {}
        self._on_task_changed = on_task_changed
        self._on_error = on_error
        self._notifier = notifier
        self._agent_factory = agent_factory
        self._event_bus = event_bus
        self._git = git_adapter
        self._runtime_service = runtime_service

        self._event_queue: asyncio.Queue[AutomationEvent] = (
            asyncio.Queue()  # quality-allow-unbounded-queue
        )
        self._pending_spawn_queue: deque[str] = deque()
        self._pending_spawn_set: set[str] = set()
        self._blocked_pending: dict[str, BlockedSpawnState] = {}
        self._pending_spawn_lock = asyncio.Lock()
        self._worker_task: asyncio.Task[None] | None = None
        self._event_task: asyncio.Task[None] | None = None
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

    # ------------------------------------------------------------------
    # Lifecycle: start / stop
    # ------------------------------------------------------------------

    async def start(self) -> None:
        """Start the automation event processing loop."""
        if self._started:
            return
        self._started = True
        self._worker_task = asyncio.create_task(self._worker_loop())
        if self._event_bus:
            self._event_task = asyncio.create_task(self._event_loop())
        log.info("Automation service started (reactive mode)")

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

    # ------------------------------------------------------------------
    # Event routing
    # ------------------------------------------------------------------

    async def handle_event(self, event: DomainEvent) -> None:
        """Process a domain event and trigger actions."""
        if isinstance(event, TaskStatusChanged):
            await self._event_queue.put(
                AutomationEvent(
                    task_id=event.task_id,
                    old_status=event.from_status,
                    new_status=event.to_status,
                )
            )

    async def handle_status_change(
        self, task_id: str, old_status: TaskStatus | None, new_status: TaskStatus | None
    ) -> None:
        """Handle a task status change event."""
        await self._event_queue.put(
            AutomationEvent(
                task_id=task_id,
                old_status=old_status,
                new_status=new_status,
            )
        )
        log.debug(f"Queued status change: {task_id} {old_status} -> {new_status}")

    async def _event_loop(self) -> None:
        """Subscribe to domain events and enqueue relevant automation work."""
        assert self._event_bus is not None
        async for event in self._event_bus.subscribe(TaskStatusChanged):
            await self.handle_event(event)

    async def _worker_loop(self) -> None:
        """Single worker that processes status events sequentially."""
        log.info("Automation worker loop started")
        while True:
            try:
                event = await self._event_queue.get()
                await self._process_status_event(event.task_id, event.old_status, event.new_status)
            except asyncio.CancelledError:
                log.info("Automation worker loop cancelled")
                break
            except Exception as e:  # quality-allow-broad-except
                log.error(f"Error in automation worker: {e}")

    async def _process_status_event(
        self, task_id: str, old_status: TaskStatus | None, new_status: TaskStatus | None
    ) -> None:
        if new_status is None:
            await self._stop_if_running(task_id)
            return

        task = await self._tasks.get_task(task_id)
        if task is None:
            await self._stop_if_running(task_id)
            return

        if not is_auto_task(task.task_type):
            self._blocked_pending.pop(task_id, None)
            self._clear_runtime_blocked(task_id)
            return

        if task_id in self._blocked_pending and task.status is not TaskStatus.BACKLOG:
            self._blocked_pending.pop(task_id, None)
            self._clear_runtime_blocked(task_id)

        if should_stop_running_on_status_change(old_status=old_status, new_status=new_status):
            await self._stop_if_running(task_id)
        await self._retry_blocked_pending_spawns()

    async def _process_spawn(self, task_id: str) -> None:
        """Handle explicit spawn requests from the UI."""
        if task_id in self._running:
            self._discard_pending_spawn(task_id)
            log.debug(f"Task {task_id} already running")
            return

        task = await self._tasks.get_task(task_id)
        if task is None:
            self._discard_pending_spawn(task_id)
            return
        if not is_auto_task(task.task_type):
            self._discard_pending_spawn(task_id)
            return

        self._blocked_pending.pop(task.id, None)
        self._clear_runtime_blocked(task.id)
        self._enqueue_pending_spawn(task.id)
        await self._admit_pending_spawns()

    # ------------------------------------------------------------------
    # Public query API
    # ------------------------------------------------------------------

    @property
    def running_tasks(self) -> set[str]:
        return self._runtime_service.running_tasks()

    def is_running(self, task_id: str) -> bool:
        view = self._runtime_view(task_id)
        if view is not None and view.is_running:
            return True
        return task_id in self._running

    def get_running_agent(self, task_id: str) -> Agent | None:
        """Get the running agent for a task (for watch functionality)."""
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
        """Wait for a task's running agent reference to become available."""
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
        view = self._runtime_view(task_id)
        return view.execution_id if view is not None else None

    def get_run_count(self, task_id: str) -> int:
        view = self._runtime_view(task_id)
        return view.run_count if view is not None else 0

    def is_reviewing(self, task_id: str) -> bool:
        view = self._runtime_view(task_id)
        return view.is_reviewing if view is not None else False

    def get_review_agent(self, task_id: str) -> Agent | None:
        view = self._runtime_view(task_id)
        return view.review_agent if view is not None else None

    # ------------------------------------------------------------------
    # Public action API
    # ------------------------------------------------------------------

    async def stop_task(self, task_id: str) -> bool:
        """Request to stop a task. Returns True if was running."""
        if task_id not in self._running:
            was_pending = task_id in self._pending_spawn_set or task_id in self._blocked_pending
            if not was_pending:
                return False

            self._discard_pending_spawn(task_id)
            self._blocked_pending.pop(task_id, None)
            self._clear_runtime_pending(task_id)
            self._clear_runtime_blocked(task_id)
            task = await self._tasks.get_task(task_id)
            if task is not None and task.status is not TaskStatus.BACKLOG:
                await self._update_task_status(task.id, TaskStatus.BACKLOG)
                self._notify_task_changed()
            return True

        task = await self._tasks.get_task(task_id)
        old_status = task.status if task is not None else TaskStatus.IN_PROGRESS
        if task is not None and task.status is not TaskStatus.BACKLOG:
            await self._update_task_status(task_id, TaskStatus.BACKLOG)
            self._notify_task_changed()
        if old_status is TaskStatus.BACKLOG:
            old_status = TaskStatus.IN_PROGRESS

        await self._event_queue.put(
            AutomationEvent(
                task_id=task_id,
                old_status=old_status,
                new_status=TaskStatus.BACKLOG,
            )
        )
        return True

    async def spawn_for_task(self, task: TaskLike) -> bool:
        """Manually request to spawn an agent for a task."""
        if task.id in self._running:
            return False
        if not is_auto_task(task.task_type):
            return False

        self._mark_runtime_pending(task.id, reason="Queued for scheduler admission.")
        await self._process_spawn(task.id)
        return True

    # ------------------------------------------------------------------
    # Review delegation
    # ------------------------------------------------------------------

    async def run_review(
        self, task: TaskLike, wt_path: Path, execution_id: str
    ) -> tuple[bool, str]:
        return await self._reviewer.run_review(task, wt_path, execution_id)

    async def _handle_complete(self, task: TaskLike) -> None:
        await self._reviewer._handle_complete(task)

    async def _handle_blocked(self, task: TaskLike, reason: str) -> None:
        self._blocked_pending.pop(task.id, None)
        self._mark_runtime_blocked(task.id, reason=reason)
        await self._reviewer._handle_blocked(task, reason)

    # ------------------------------------------------------------------
    # Preparation: pending-spawn queue management
    # ------------------------------------------------------------------

    def _enqueue_pending_spawn(self, task_id: str) -> None:
        """Add task to FIFO pending spawn queue (deduplicated)."""
        if task_id in self._running:
            self._discard_pending_spawn(task_id)
            return
        if task_id in self._pending_spawn_set:
            return
        self._pending_spawn_set.add(task_id)
        self._pending_spawn_queue.append(task_id)

    def _discard_pending_spawn(self, task_id: str) -> None:
        """Remove task from pending spawn queue if present."""
        if task_id not in self._pending_spawn_set:
            self._clear_runtime_pending(task_id)
            return
        self._pending_spawn_set.discard(task_id)
        with contextlib.suppress(ValueError):
            self._pending_spawn_queue.remove(task_id)
        self._clear_runtime_pending(task_id)

    async def _admit_pending_spawns(self) -> None:
        """Start pending AUTO tasks while capacity is available."""
        async with self._pending_spawn_lock:
            max_agents = self._config.general.max_concurrent_agents
            while self._pending_spawn_queue and can_spawn_new_agent(
                running_count=len(self._running),
                max_agents=max_agents,
            ):
                started = False
                for next_task_id in tuple(self._pending_spawn_queue):
                    task = await self._tasks.get_task(next_task_id)
                    if (
                        task is None
                        or not is_auto_task(task.task_type)
                        or next_task_id in self._running
                    ):
                        self._discard_pending_spawn(next_task_id)
                        self._blocked_pending.pop(next_task_id, None)
                        self._clear_runtime_blocked(next_task_id)
                        continue

                    running_tasks = await self._list_running_auto_tasks(
                        exclude_task_id=task.id,
                    )
                    conflict = assess_conflict(task, running_tasks)
                    if conflict.is_blocked:
                        self._discard_pending_spawn(task.id)
                        await self._mark_spawn_blocked(task, conflict)
                        continue

                    self._discard_pending_spawn(task.id)
                    self._blocked_pending.pop(task.id, None)
                    self._clear_runtime_blocked(task.id)
                    await self._spawn(task)
                    started = True
                    break
                if not started:
                    break

            if self._pending_spawn_queue:
                next_task_id = self._pending_spawn_queue[0]
                log.debug(
                    f"At capacity ({max_agents}), deferred spawn for task {next_task_id[:8]} "
                    f"(pending={len(self._pending_spawn_queue)})"
                )
                for queued_task_id in self._pending_spawn_queue:
                    self._mark_runtime_pending(
                        queued_task_id,
                        reason="Queued for capacity: waiting for an available agent slot.",
                    )

    async def _spawn(self, task: TaskLike) -> None:
        """Spawn an agent for a task."""
        title = task.title[:MODAL_TITLE_MAX_LENGTH]
        log.info(f"Spawning agent for AUTO task {task.id}: {title}")
        if task.status is not TaskStatus.IN_PROGRESS:
            await self._update_task_status(task.id, TaskStatus.IN_PROGRESS)
            self._notify_task_changed()

        state = RunningTaskState()
        self._running[task.id] = state
        self._runtime_service.mark_started(task.id)
        self._check_runtime_view_consistency(task.id, phase="mark_started")
        await self._publish_runtime_event(AutomationTaskStarted(task_id=task.id))

        runner_task = asyncio.create_task(self._run_task_loop(task))
        state.task = runner_task

        runner_task.add_done_callback(self._make_done_callback(task.id))

    # ------------------------------------------------------------------
    # Preparation: conflict detection and blocked-spawn management
    # ------------------------------------------------------------------

    async def _list_running_auto_tasks(self, *, exclude_task_id: str) -> dict[str, TaskLike]:
        running: dict[str, TaskLike] = {}
        for running_task_id in tuple(self._running.keys()):
            if running_task_id == exclude_task_id:
                continue
            task = await self._tasks.get_task(running_task_id)
            if task is None or not is_auto_task(task.task_type):
                continue
            running[running_task_id] = task
        return running

    async def _mark_spawn_blocked(self, task: TaskLike, conflict: ConflictAssessment) -> None:
        overlap_preview = ", ".join(conflict.overlap_hints[:3])
        blockers_preview = ", ".join(f"#{task_id[:8]}" for task_id in conflict.blocker_task_ids[:3])
        reason = f"Waiting on {blockers_preview} before starting"
        if overlap_preview:
            reason += f" (overlap: {overlap_preview})"

        self._blocked_pending[task.id] = BlockedSpawnState(
            task_id=task.id,
            blocker_task_ids=conflict.blocker_task_ids,
            overlap_hints=conflict.overlap_hints,
            reason=reason,
            blocked_at=utc_now(),
        )
        self._mark_runtime_blocked(
            task.id,
            reason=reason,
            blocked_by_task_ids=conflict.blocker_task_ids,
            overlap_hints=conflict.overlap_hints,
        )
        await self._record_blocked_history(
            task.id,
            reason=reason,
            blocker_task_ids=conflict.blocker_task_ids,
            overlap_hints=conflict.overlap_hints,
        )
        if task.status is not TaskStatus.BACKLOG:
            await self._update_task_status(task.id, TaskStatus.BACKLOG)
        self._notify_task_changed()

    async def _record_blocked_history(
        self,
        task_id: str,
        *,
        reason: str,
        blocker_task_ids: tuple[str, ...],
        overlap_hints: tuple[str, ...],
    ) -> None:
        """Persist a lightweight blocked event trail in the task scratchpad."""
        timestamp = utc_now().strftime("%Y-%m-%d %H:%M")
        blockers = ", ".join(f"#{blocker_id[:8]}" for blocker_id in blocker_task_ids) or "none"
        overlap = ", ".join(overlap_hints[:5]) or "n/a"
        entry = (
            f"\n\n---\n[Blocked auto-start {timestamp}]\n"
            f"- Reason: {reason}\n"
            f"- Blocked by: {blockers}\n"
            f"- Overlap hints: {overlap}"
        )
        try:
            scratchpad = await self._tasks.get_scratchpad(task_id)
            await self._tasks.update_scratchpad(task_id, (scratchpad or "") + entry)
        except Exception as exc:  # quality-allow-broad-except
            log.debug("Unable to persist blocked history for %s: %s", task_id, exc)

    async def _retry_blocked_pending_spawns(self) -> None:
        if not self._blocked_pending:
            return

        resumed_task_ids: list[str] = []
        for task_id, blocked in tuple(self._blocked_pending.items()):
            task = await self._tasks.get_task(task_id)
            if task is None or not is_auto_task(task.task_type):
                self._blocked_pending.pop(task_id, None)
                self._clear_runtime_blocked(task_id)
                continue

            waiting_on: list[str] = []
            for blocker_task_id in blocked.blocker_task_ids:
                if await self._blocker_is_active(blocker_task_id):
                    waiting_on.append(blocker_task_id)
            if waiting_on:
                continue

            self._blocked_pending.pop(task_id, None)
            self._clear_runtime_blocked(task_id)
            self._enqueue_pending_spawn(task.id)
            self._mark_runtime_pending(task.id, reason="Queued after blockers cleared.")
            resumed_task_ids.append(task.id)

        if resumed_task_ids:
            self._notify_task_changed()
            await self._admit_pending_spawns()

    async def _blocker_is_active(self, blocker_task_id: str) -> bool:
        """Return whether blocker task should keep dependent task in blocked state."""
        if blocker_task_id in self._running:
            return True

        blocker = await self._tasks.get_task(blocker_task_id)
        if blocker is None:
            return False

        runtime_view = self._runtime_view(blocker_task_id)
        if runtime_view is not None and (
            runtime_view.is_running or runtime_view.is_reviewing or runtime_view.is_pending
        ):
            return True

        return blocker.status in {TaskStatus.IN_PROGRESS, TaskStatus.REVIEW}

    # ------------------------------------------------------------------
    # Execution: agent invocation
    # ------------------------------------------------------------------

    async def _persist_incremental_agent_output(
        self,
        execution_id: str,
        agent: Agent,
        *,
        next_message_index: int,
    ) -> tuple[int, bool]:
        """Persist newly buffered agent messages for a running execution."""
        buffered_messages = agent.get_messages()
        if next_message_index >= len(buffered_messages):
            return len(buffered_messages), False

        new_messages = buffered_messages[next_message_index:]
        payload = serialize_agent_messages(new_messages)
        if payload is None or self._executions is None:
            return len(buffered_messages), False

        await self._executions.append_execution_log(execution_id, payload)
        return len(buffered_messages), True

    async def _send_prompt_with_incremental_persistence(
        self,
        *,
        task_id: str,
        execution_id: str,
        agent: Agent,
        prompt: str,
    ) -> bool:
        """Send a prompt and flush incremental output while the prompt is running."""
        if self._executions is None:
            await agent.send_prompt(prompt)
            return False

        prompt_task = asyncio.create_task(agent.send_prompt(prompt))
        next_message_index = 0
        persisted_any = False

        try:
            while True:
                done, _ = await asyncio.wait(
                    {prompt_task},
                    timeout=_STREAM_LOG_FLUSH_INTERVAL_SECONDS,
                )
                next_message_index, persisted = await self._persist_incremental_agent_output(
                    execution_id,
                    agent,
                    next_message_index=next_message_index,
                )
                persisted_any = persisted_any or persisted
                if done:
                    break

            await prompt_task
        except Exception:
            if not prompt_task.done():
                prompt_task.cancel()
                with contextlib.suppress(asyncio.CancelledError):
                    await prompt_task
            raise
        finally:
            try:
                next_message_index, persisted = await self._persist_incremental_agent_output(
                    execution_id,
                    agent,
                    next_message_index=next_message_index,
                )
                persisted_any = persisted_any or persisted
            except Exception as exc:  # quality-allow-broad-except
                log.debug(
                    "Unable to persist trailing incremental output for %s: %s",
                    task_id,
                    exc,
                )

        return persisted_any

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
        """Run a single execution for a task."""
        agent = self._agent_factory(wt_path, agent_config)
        maybe_set_task_id = getattr(agent, "set_task_id", None)
        if callable(maybe_set_task_id):
            maybe_set_task_id(task.id)
        auto_approve = resolve_auto_approve(
            scope=AgentPermissionScope.AUTOMATION_RUNNER,
            planner_auto_approve=self._config.general.auto_approve,
        )
        agent.set_auto_approve(auto_approve)

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
        persisted_incremental_output = False
        try:
            persisted_incremental_output = await self._send_prompt_with_incremental_persistence(
                task_id=task.id,
                execution_id=execution_id,
                agent=agent,
                prompt=prompt,
            )
        except Exception as e:  # quality-allow-broad-except
            log.error(f"Agent prompt failed for {task.id}: {e}")
            return (parse_signal(f'<blocked reason="Agent error: {e}"/>'), agent)
        finally:
            agent.clear_tool_calls()

        response = agent.get_response_text()
        signal_result = parse_signal(response)

        if self._executions is not None:
            if not persisted_incremental_output:
                serialized_output = serialize_agent_output(agent)
                await self._executions.append_execution_log(execution_id, serialized_output)
            await self._executions.append_agent_turn(
                execution_id,
                prompt=prompt,
                summary=response,
            )

        progress_note = f"\n\n--- Run {run_count} ---\n{response[-2000:]}"
        await self._tasks.update_scratchpad(task.id, scratchpad + progress_note)

        return (signal_result, agent)

    # ------------------------------------------------------------------
    # Completion: task lifecycle loop
    # ------------------------------------------------------------------

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
                        base_branch=(task.base_branch or self._config.general.default_base_branch),
                    )
                except ValueError as exc:
                    error_msg = str(exc)
                    log.error(f"Workspace creation failed for task {task.id}: {error_msg}")
                    self._notify_error(task.id, error_msg)
                    self._notify_user(
                        f"\u274c {error_msg}",
                        title="Cannot Start Agent",
                        severity=NotificationSeverity.ERROR,
                    )
                    await self._update_task_status(task.id, TaskStatus.BACKLOG)
                    return
                except Exception as exc:  # quality-allow-broad-except
                    error_str = str(exc).lower()
                    if "not a git repository" in error_str or "fatal:" in error_str:
                        error_msg = f"Repository is not a valid git repo: {exc}"
                    else:
                        error_msg = f"Failed to create workspace: {exc}"
                    log.error(f"Workspace creation failed for task {task.id}: {exc}")
                    self._notify_error(task.id, error_msg)
                    self._notify_user(
                        f"\u274c {error_msg}",
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
                        await self._process_spawn(task.id)
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
        except Exception as e:  # quality-allow-broad-except
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

    # ------------------------------------------------------------------
    # Completion: stop / cleanup helpers
    # ------------------------------------------------------------------

    async def _stop_if_running(self, task_id: str) -> None:
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
        del task
        self._remove_running_state_soon(task_id)

    def _make_done_callback(self, task_id: str) -> Callable[[asyncio.Task[None]], None]:
        """Use weak self-reference to avoid preventing garbage collection of the engine."""
        weak_self = weakref.ref(self)

        def on_done(task: asyncio.Task[None]) -> None:
            service = weak_self()
            if service is not None:
                service._handle_task_done(task_id, task)

        return on_done

    # ------------------------------------------------------------------
    # Completion: running-state removal and lifecycle events
    # ------------------------------------------------------------------

    async def _remove_running_state(self, task_id: str) -> None:
        """Remove running state and emit lifecycle-ended event once."""
        removed = self._running.pop(task_id, None)
        if removed is None:
            return
        self._runtime_service.mark_ended(task_id)
        self._check_runtime_view_consistency(task_id, phase="mark_ended")
        await self._publish_runtime_event(AutomationTaskEnded(task_id=task_id))
        if removed.pending_respawn:
            self._enqueue_pending_spawn(task_id)
        await self._retry_blocked_pending_spawns()
        await self._admit_pending_spawns()

    def _remove_running_state_soon(self, task_id: str) -> None:
        """Sync variant of running-state removal for task callbacks."""
        removed = self._running.pop(task_id, None)
        if removed is None:
            return
        self._runtime_service.mark_ended(task_id)
        self._check_runtime_view_consistency(task_id, phase="mark_ended_sync")
        self._publish_runtime_event_soon(AutomationTaskEnded(task_id=task_id))
        if removed.pending_respawn:
            self._enqueue_pending_spawn(task_id)
        with contextlib.suppress(RuntimeError):
            self._background_tasks.spawn(self._retry_blocked_pending_spawns())
        with contextlib.suppress(RuntimeError):
            self._background_tasks.spawn(self._admit_pending_spawns())

    # ------------------------------------------------------------------
    # Completion: agent attachment helpers
    # ------------------------------------------------------------------

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

    # ------------------------------------------------------------------
    # Completion: queued-message helpers
    # ------------------------------------------------------------------

    async def _take_implementation_queue(
        self,
        task_id: str,
        session_id: str | None,
    ) -> QueuedMessage | None:
        queued = await self._queued.take_queued(task_id, lane="implementation")
        if queued is not None:
            return queued
        if session_id is None:
            return None
        return await self._queued.take_queued(session_id, lane="implementation")

    async def _append_queued_message_to_scratchpad(self, task_id: str, content: str) -> None:
        scratchpad = await self._tasks.get_scratchpad(task_id)
        user_note = f"\n\n--- USER MESSAGE ---\n{truncate_queue_payload(content)}"
        await self._tasks.update_scratchpad(task_id, scratchpad + user_note)

    # ------------------------------------------------------------------
    # Preparation: runtime state markers
    # ------------------------------------------------------------------

    def _mark_runtime_blocked(
        self,
        task_id: str,
        *,
        reason: str,
        blocked_by_task_ids: tuple[str, ...] = (),
        overlap_hints: tuple[str, ...] = (),
    ) -> None:
        marker = getattr(self._runtime_service, "mark_blocked", None)
        if callable(marker):
            marker(
                task_id,
                reason=reason,
                blocked_by_task_ids=blocked_by_task_ids,
                overlap_hints=overlap_hints,
            )

    def _clear_runtime_blocked(self, task_id: str) -> None:
        clearer = getattr(self._runtime_service, "clear_blocked", None)
        if callable(clearer):
            clearer(task_id)

    def _mark_runtime_pending(self, task_id: str, *, reason: str) -> None:
        marker = getattr(self._runtime_service, "mark_pending", None)
        if callable(marker):
            marker(task_id, reason=reason)

    def _clear_runtime_pending(self, task_id: str) -> None:
        clearer = getattr(self._runtime_service, "clear_pending", None)
        if callable(clearer):
            clearer(task_id)

    # ------------------------------------------------------------------
    # Shared helpers
    # ------------------------------------------------------------------

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

    def _notify_task_changed(self) -> None:
        if self._on_task_changed:
            self._on_task_changed()

    def _notify_error(self, task_id: str, message: str) -> None:
        if self._on_error:
            self._on_error(task_id, message)

    def _notify_user(self, message: str, title: str, severity: NotificationSeverity) -> None:
        """Send a notification to the user via the app if available."""
        if self._notifier is not None:
            self._notifier(message, title, severity)

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

    def _get_agent_config(self, task: TaskLike) -> AgentConfig:
        return task.get_agent_config(self._config)

    def _apply_model_override(self, agent: Agent, agent_config: AgentConfig, context: str) -> None:
        """Apply model override to agent if configured."""
        model = None
        if "claude" in agent_config.identity.lower():
            model = self._config.general.default_model_claude
        elif "opencode" in agent_config.identity.lower():
            model = self._config.general.default_model_opencode

        if model:
            agent.set_model_override(model)
            log.info(f"Applied model override for {context}: {model}")

    async def _update_task_status(self, task_id: str, status: TaskStatus) -> None:
        await self._tasks.update_fields(task_id, status=status)
