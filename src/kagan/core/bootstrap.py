"""Application bootstrap and dependency injection.

This module provides the AppContext which wires all services, adapters,
and event handlers together. It serves as the single point of configuration
for the application and enables clean dependency injection for testing.

Usage:
    async with bootstrap_app(config_path, db_path) as ctx:
        # ctx.task_service, ctx.event_bus, etc. are ready
        await ctx.automation_service.start()
"""

from __future__ import annotations

import asyncio
import contextlib
from contextlib import asynccontextmanager
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, TypeVar, cast

from kagan.core.config import KaganConfig
from kagan.core.events import (
    AutomationAgentAttached,
    AutomationReviewAgentAttached,
    AutomationTaskEnded,
    AutomationTaskStarted,
    DomainEvent,
    EventBus,
    EventHandler,
    MergeCompleted,
    MergeFailed,
    PRCreated,
    ProjectOpened,
    ScriptCompleted,
    TaskCreated,
    TaskDeleted,
    TaskStatusChanged,
    TaskUpdated,
)
from kagan.core.plugins.examples import register_example_plugins
from kagan.core.plugins.sdk import PluginRegistry

if TYPE_CHECKING:
    from collections.abc import AsyncIterator, Callable

    from textual.signal import Signal

    from kagan.core.adapters.db.repositories import (
        AuditRepository,
        ExecutionRepository,
        PlannerRepository,
        TaskRepository,
    )
    from kagan.core.agents.agent_factory import AgentFactory
    from kagan.core.api import KaganAPI
    from kagan.core.services.agent_health import AgentHealthService
    from kagan.core.services.automation import AutomationService
    from kagan.core.services.diffs import DiffService
    from kagan.core.services.jobs import JobService
    from kagan.core.services.merges import MergeService
    from kagan.core.services.projects import ProjectService
    from kagan.core.services.reviews import ReviewService
    from kagan.core.services.runtime import RuntimeService
    from kagan.core.services.sessions import SessionService
    from kagan.core.services.tasks import TaskService
    from kagan.core.services.workspaces import WorkspaceService


class InMemoryEventBus:
    """Simple async event bus with fan-out to handlers and async subscribers.

    This implementation is suitable for single-process use. Events are not
    persisted or replayed; new subscribers only receive future events.
    """

    def __init__(self) -> None:
        self._handlers: list[tuple[type[DomainEvent] | None, EventHandler]] = []
        self._queues: list[tuple[type[DomainEvent] | None, asyncio.Queue[DomainEvent]]] = []
        self._lock = asyncio.Lock()

    async def publish(self, event: DomainEvent) -> None:
        """Publish event to all matching handlers and subscribers."""
        for filter_type, handler in self._handlers:
            if filter_type is None or isinstance(event, filter_type):
                with contextlib.suppress(Exception):  # quality-allow-broad-except
                    handler(event)

        async with self._lock:
            for filter_type, queue in self._queues:
                if filter_type is None or isinstance(event, filter_type):
                    with contextlib.suppress(asyncio.QueueFull):
                        queue.put_nowait(event)

    def add_handler(
        self,
        handler: EventHandler,
        event_type: type[DomainEvent] | None = None,
    ) -> None:
        """Register a synchronous handler for events."""
        self._handlers.append((event_type, handler))

    def remove_handler(self, handler: EventHandler) -> None:
        """Remove a previously registered handler."""
        self._handlers = [(t, h) for t, h in self._handlers if h is not handler]

    async def subscribe(
        self, event_type: type[DomainEvent] | None = None
    ) -> AsyncIterator[DomainEvent]:
        """Subscribe to events, yielding them as they arrive."""
        queue: asyncio.Queue[DomainEvent] = asyncio.Queue(maxsize=100)
        async with self._lock:
            self._queues.append((event_type, queue))
        try:
            while True:
                event = await queue.get()
                yield event
        finally:
            async with self._lock:
                self._queues = [(t, q) for t, q in self._queues if q is not queue]


@dataclass
class SignalBinding:
    """Maps a domain event type to a Textual Signal."""

    event_type: type[DomainEvent]
    signal: Signal
    extractor: Callable[[DomainEvent], object] | None = None


TDomainEvent = TypeVar("TDomainEvent", bound=DomainEvent)


class SignalBridge:
    """Bridges domain events to Textual Signals for reactive UI updates.

    The SignalBridge subscribes to the event bus and publishes to Textual
    Signals when matching events arrive. This keeps the UI decoupled from
    services while enabling real-time updates.

    Usage:
        bridge = SignalBridge(event_bus)
        bridge.bind(TaskStatusChanged, app.task_changed_signal,
                    extractor=lambda e: e.task_id)
        bridge.bind(TaskCreated, app.task_changed_signal,
                    extractor=lambda e: e.task_id)
    """

    def __init__(self, event_bus: EventBus) -> None:
        self._event_bus = event_bus
        self._bindings: list[SignalBinding] = []
        self._handler_registered = False

    def bind(
        self,
        event_type: type[TDomainEvent],
        signal: Signal,
        *,
        extractor: Callable[[TDomainEvent], object] | None = None,
    ) -> None:
        """Bind an event type to a Textual Signal.

        Args:
            event_type: The domain event class to listen for.
            signal: The Textual Signal to publish to.
            extractor: Optional function to extract the signal payload from
                       the event. If None, the full event is published.
        """
        binding_extractor = cast(
            "Callable[[DomainEvent], object] | None",
            extractor,
        )  # cast-justified: generic callback erasure for SignalBinding storage
        self._bindings.append(SignalBinding(event_type, signal, binding_extractor))
        if not self._handler_registered:
            self._event_bus.add_handler(self._on_event)
            self._handler_registered = True

    def _on_event(self, event: DomainEvent) -> None:
        """Handle incoming events and publish to bound signals."""
        for binding in self._bindings:
            if isinstance(event, binding.event_type):
                payload = binding.extractor(event) if binding.extractor else event
                with contextlib.suppress(Exception):  # quality-allow-broad-except
                    binding.signal.publish(payload)

    def unbind_all(self) -> None:
        """Remove all bindings and unregister from event bus."""
        if self._handler_registered:
            self._event_bus.remove_handler(self._on_event)
            self._handler_registered = False
        self._bindings.clear()


@dataclass
class AppContext:
    """Central container for application dependencies.

    AppContext holds all services, adapters, and configuration needed by
    the application. It is created during bootstrap and passed to screens
    and components that need access to the domain layer.

    This design enables:
    - Clean dependency injection for testing
    - Single point of wiring for services
    - Explicit dependencies (no global state)
    - Easy mocking of individual services

    Attributes:
        config: Application configuration.
        event_bus: Domain event bus for pub/sub.
        signal_bridge: Bridges events to Textual Signals.
        task_service: Task operations.
        workspace_service: Workspace (worktree) operations.
        session_service: Tmux session operations.
        execution_service: Agent/script execution operations.
        review_service: Review status and log updates.
        merge_service: Git merge operations.
        automation_service: Reactive automation service for automated workflows.
    """

    config: KaganConfig
    config_path: Path
    db_path: Path

    event_bus: EventBus = field(default_factory=InMemoryEventBus)
    signal_bridge: SignalBridge | None = None

    task_service: TaskService = field(init=False)
    workspace_service: WorkspaceService = field(init=False)
    session_service: SessionService = field(init=False)
    execution_service: ExecutionRepository = field(init=False)
    job_service: JobService = field(init=False)
    review_service: ReviewService = field(init=False)
    runtime_service: RuntimeService = field(init=False)
    merge_service: MergeService = field(init=False)
    diff_service: DiffService = field(init=False)
    automation_service: AutomationService = field(init=False)
    project_service: ProjectService = field(init=False)
    agent_health: AgentHealthService = field(init=False)
    audit_repository: AuditRepository = field(init=False)
    planner_repository: PlannerRepository = field(init=False)
    api: KaganAPI = field(init=False)
    plugin_registry: PluginRegistry = field(init=False)

    active_project_id: str | None = None
    active_repo_id: str | None = None

    _task_repo: TaskRepository | None = field(default=None, repr=False)

    async def close(self) -> None:
        """Clean up all resources.

        Shutdown order matters:
        1. Mark the repository as closing so new ``_get_session()`` calls
           raise ``RepositoryClosing`` instead of hitting a disposed engine.
        2. Unbind signals to stop scheduling new UI workers.
        3. Stop the automation service (its own asyncio tasks).
        4. Dispose the engine (safe now — no new sessions can be created).
        """
        if self._task_repo is not None:
            self._task_repo.mark_closing()

        if self.signal_bridge:
            self.signal_bridge.unbind_all()

        if hasattr(self, "automation_service"):
            await self.automation_service.stop()
        if hasattr(self, "job_service"):
            await self.job_service.shutdown()

        if self._task_repo is not None:
            await self._task_repo.close()


def create_signal_bridge(event_bus: EventBus) -> SignalBridge:
    """Create a SignalBridge for the given event bus."""
    return SignalBridge(event_bus)


def wire_default_signals(bridge: SignalBridge, app: object) -> None:
    """Wire default event-to-signal bindings for the Kagan app.

    This function sets up the standard bindings between domain events
    and Textual Signals used by the Kagan UI.

    Args:
        bridge: The SignalBridge to configure.
        app: The KaganApp instance with signal attributes.
    """

    if hasattr(app, "task_changed_signal"):
        bridge.bind(
            TaskCreated,
            app.task_changed_signal,
            extractor=lambda e: e.task_id,
        )
        bridge.bind(
            TaskUpdated,
            app.task_changed_signal,
            extractor=lambda e: e.task_id,
        )
        bridge.bind(
            TaskDeleted,
            app.task_changed_signal,
            extractor=lambda e: e.task_id,
        )
        bridge.bind(
            TaskStatusChanged,
            app.task_changed_signal,
            extractor=lambda e: e.task_id,
        )
        bridge.bind(
            AutomationTaskStarted,
            app.task_changed_signal,
            extractor=lambda e: e.task_id,
        )
        bridge.bind(
            AutomationAgentAttached,
            app.task_changed_signal,
            extractor=lambda e: e.task_id,
        )
        bridge.bind(
            AutomationReviewAgentAttached,
            app.task_changed_signal,
            extractor=lambda e: e.task_id,
        )
        bridge.bind(
            AutomationTaskEnded,
            app.task_changed_signal,
            extractor=lambda e: e.task_id,
        )


@asynccontextmanager
async def bootstrap_app(
    config_path: Path,
    db_path: Path,
    *,
    config: KaganConfig | None = None,
    project_root: Path | None = None,
) -> AsyncIterator[AppContext]:
    """Bootstrap the application context with all services wired.

    This is the main entry point for creating an application context.
    It handles initialization and cleanup of all services.

    Args:
        config_path: Path to the config.toml file.
        db_path: Path to the SQLite database.
        config: Optional pre-loaded config (for testing).
        project_root: Optional project root override (defaults to cwd).

    Yields:
        Fully initialized AppContext.

    Example:
        async with bootstrap_app(config_path, db_path) as ctx:
            await ctx.automation_service.start()
            # ... run application ...
    """
    try:
        ctx = await create_app_context(
            config_path, db_path, config=config, project_root=project_root
        )
        yield ctx
    finally:
        await ctx.close()


async def create_app_context(
    config_path: Path,
    db_path: Path,
    *,
    config: KaganConfig | None = None,
    project_root: Path | None = None,
    agent_factory: AgentFactory | None = None,
) -> AppContext:
    """Create a fully initialized AppContext (non-context-manager)."""
    if config is None:
        config = KaganConfig.load(config_path)

    event_bus = InMemoryEventBus()
    ctx = AppContext(
        config=config,
        config_path=config_path,
        db_path=db_path,
        event_bus=event_bus,
    )
    ctx.plugin_registry = PluginRegistry()
    register_example_plugins(ctx.plugin_registry)

    from kagan.core.adapters.db.repositories import (
        AuditRepository,
        ExecutionRepository,
        JobRepository,
        PlannerRepository,
        RepoRepository,
        ScratchRepository,
        SessionRecordRepository,
        TaskRepository,
    )
    from kagan.core.adapters.git.operations import GitOperationsAdapter
    from kagan.core.adapters.git.worktrees import GitWorktreeAdapter
    from kagan.core.agents.agent_factory import create_agent
    from kagan.core.services import (
        AutomationServiceImpl,
        DiffServiceImpl,
        JobServiceImpl,
        MergeServiceImpl,
        ProjectServiceImpl,
        ReviewServiceImpl,
        RuntimeServiceImpl,
        SessionServiceImpl,
        TaskServiceImpl,
        WorkspaceServiceImpl,
    )

    project_root = project_root or Path.cwd()

    task_repo = TaskRepository(
        db_path,
        project_root=project_root,
        default_branch=config.general.default_base_branch,
    )
    await task_repo.initialize()

    def _set_default_project(event: DomainEvent) -> None:
        if isinstance(event, ProjectOpened):
            task_repo.set_default_project_id(event.project_id)

    event_bus.add_handler(_set_default_project, ProjectOpened)

    session_factory = task_repo.session_factory
    repo_repository = RepoRepository(session_factory)
    execution_repository = ExecutionRepository(session_factory)
    session_record_repository = SessionRecordRepository(session_factory)
    scratch_repository = ScratchRepository(session_factory)
    audit_repository = AuditRepository(session_factory)
    planner_repository = PlannerRepository(session_factory)
    job_repository = JobRepository(session_factory)

    ctx._task_repo = task_repo
    ctx.audit_repository = audit_repository
    ctx.planner_repository = planner_repository
    ctx.task_service = TaskServiceImpl(
        task_repo,
        event_bus,
        session_repo=session_record_repository,
        scratch_repo=scratch_repository,
    )
    ctx.project_service = ProjectServiceImpl(
        session_factory,
        event_bus,
        repo_repository,
    )
    git_adapter = GitWorktreeAdapter()
    git_ops_adapter = GitOperationsAdapter()
    ctx.workspace_service = WorkspaceServiceImpl(
        session_factory,
        git_adapter,
        ctx.task_service,
        ctx.project_service,
    )
    ctx.session_service = SessionServiceImpl(
        project_root,
        ctx.task_service,
        ctx.workspace_service,
        config,
    )
    ctx.execution_service = execution_repository
    ctx.review_service = ReviewServiceImpl(ctx.task_service, ctx.execution_service)
    ctx.runtime_service = RuntimeServiceImpl(
        ctx.project_service,
        session_factory,
        execution_service=ctx.execution_service,
        automation_resolver=lambda: ctx.automation_service,
    )

    factory = agent_factory if agent_factory is not None else create_agent

    ctx.automation_service = AutomationServiceImpl(
        ctx.task_service,
        ctx.workspace_service,
        config,
        session_service=ctx.session_service,
        execution_service=ctx.execution_service,
        event_bus=event_bus,
        agent_factory=factory,
        git_adapter=git_ops_adapter,
        runtime_service=ctx.runtime_service,
    )

    async def _job_executor(action: str, params: dict[str, object]) -> dict[str, object]:
        from kagan.core.commands.job_action_executor import execute_job_action

        return await execute_job_action(ctx, action=action, params=params)

    ctx.job_service = JobServiceImpl(_job_executor, repository=job_repository)
    ctx.merge_service = MergeServiceImpl(
        ctx.task_service,
        ctx.workspace_service,
        ctx.session_service,
        ctx.automation_service,
        config,
        session_factory,
        event_bus,
        git_ops_adapter,
    )
    ctx.diff_service = DiffServiceImpl(session_factory, git_ops_adapter, ctx.workspace_service)

    from kagan.core.services.agent_health import AgentHealthServiceImpl

    ctx.agent_health = AgentHealthServiceImpl(config)

    from kagan.core.api import KaganAPI

    ctx.api = KaganAPI(ctx)

    return ctx


__all__ = [
    "AppContext",
    "DomainEvent",
    "EventBus",
    "InMemoryEventBus",
    "MergeCompleted",
    "MergeFailed",
    "PRCreated",
    "ScriptCompleted",
    "SignalBridge",
    "TaskCreated",
    "TaskDeleted",
    "TaskStatusChanged",
    "TaskUpdated",
    "bootstrap_app",
    "create_app_context",
    "create_signal_bridge",
    "wire_default_signals",
]
