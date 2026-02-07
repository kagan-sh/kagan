"""Main Kagan TUI application."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from textual.app import App
from textual.signal import Signal

from kagan.agents.agent_factory import AgentFactory, create_agent
from kagan.bootstrap import (
    AppContext,
    create_app_context,
    create_signal_bridge,
    wire_default_signals,
)
from kagan.config import KaganConfig
from kagan.constants import (
    DEFAULT_CONFIG_PATH,
    DEFAULT_DB_PATH,
    DEFAULT_LOCK_PATH,
)
from kagan.debug_log import setup_debug_logging
from kagan.git_utils import GitInitResult, has_git_repo, init_git_repo
from kagan.keybindings import APP_BINDINGS
from kagan.lock import InstanceLock, exit_if_already_running
from kagan.terminal import supports_truecolor
from kagan.theme import KAGAN_THEME, KAGAN_THEME_256
from kagan.ui.screens.kanban import KanbanScreen

if TYPE_CHECKING:
    from kagan.ui.screens.planner.state import PersistentPlannerState


class KaganApp(App):
    """Kagan TUI Application - AI-powered Kanban board."""

    TITLE = "ᘚᘛ KAGAN"
    CSS_PATH = "styles/kagan.tcss"

    BINDINGS = APP_BINDINGS

    def __init__(
        self,
        db_path: str = DEFAULT_DB_PATH,
        config_path: str = DEFAULT_CONFIG_PATH,
        lock_path: str | None = DEFAULT_LOCK_PATH,
        project_root: str | Path | None = None,
        agent_factory: AgentFactory = create_agent,
    ):
        super().__init__()
        # Register both themes and select based on terminal capabilities
        self.register_theme(KAGAN_THEME)
        self.register_theme(KAGAN_THEME_256)

        # Auto-select theme based on truecolor support
        if supports_truecolor():
            self.theme = "kagan"
        else:
            self.theme = "kagan-256"

        # Pub/sub signal for task changes - screens subscribe to this
        self.task_changed_signal: Signal[str] = Signal(self, "task_changed")
        self.iteration_changed_signal: Signal[tuple[str, int]] = Signal(self, "iteration_changed")

        self.db_path = Path(db_path)
        self.config_path = Path(config_path)
        self.lock_path = Path(lock_path) if lock_path else None
        self.project_root = Path(project_root) if project_root else Path.cwd()
        self._instance_lock: InstanceLock | None = None
        self._ctx: AppContext | None = None
        self.config: KaganConfig = KaganConfig()
        self.planner_state: PersistentPlannerState | None = None
        self._agent_factory = agent_factory

    @property
    def ctx(self) -> AppContext:
        """Get the application context for service access."""
        assert self._ctx is not None, "AppContext not initialized"
        return self._ctx

    async def on_mount(self) -> None:
        """Initialize app on mount."""
        # Set up debug logging capture for F12 viewer
        setup_debug_logging()

        # Check for first boot (no config.toml file)
        # The config directory may already exist (created by lock file),
        # so we check for config.toml specifically
        if not self.config_path.exists():
            from kagan.ui.screens.welcome import WelcomeScreen

            await self.push_screen(WelcomeScreen())
            return  # _continue_after_welcome will be called when welcome finishes

        await self._initialize_app()

    async def _initialize_app(self) -> None:
        """Initialize all app components."""
        self.config = KaganConfig.load(self.config_path)
        self.log("Config loaded", path=str(self.config_path))
        self.log.debug("Config settings", auto_start=self.config.general.auto_start)

        project_root = self.project_root
        if not await has_git_repo(project_root):
            base_branch = self.config.general.default_base_branch
            result: GitInitResult = await init_git_repo(project_root, base_branch)
            if result.success:
                self.log("Initialized git repository", base_branch=base_branch)
                if result.committed:
                    self.notify("Git repository initialized with .gitignore")
            else:
                # Show error to user
                error = result.error
                if error:
                    self.log.warning(
                        "Failed to initialize git repository",
                        path=str(project_root),
                        error_type=error.error_type,
                        message=error.message,
                    )
                    # Notify user with specific error message
                    if error.error_type == "version_low":
                        self.notify(
                            f"Git error: {error.message}. {error.details or ''}",
                            severity="error",
                        )
                    elif error.error_type == "user_not_configured":
                        self.notify(
                            f"Git error: {error.details or error.message}",
                            severity="error",
                        )
                    else:
                        self.notify(
                            f"Git initialization failed: {error.message}",
                            severity="error",
                        )
                else:
                    self.log.warning("Failed to initialize git repository", path=str(project_root))
                    self.notify("Git initialization failed", severity="error")

        if self._ctx is None:
            self._ctx = await create_app_context(
                self.config_path,
                self.db_path,
                config=self.config,
                project_root=self.project_root,
            )
            ctx = self._ctx
            # Wire signal bridge to map domain events to Textual signals
            bridge = create_signal_bridge(ctx.event_bus)
            wire_default_signals(bridge, self)
            ctx.signal_bridge = bridge
            self.log("AppContext initialized with SignalBridge")

            # Reconcile orphaned worktrees/sessions before starting automation
            await self._reconcile_worktrees()
            await self._reconcile_sessions()

            # Start automation service
            await ctx.automation_service.start()
            await ctx.automation_service.initialize_existing_tasks()
            self.log("Automation service initialized (reactive mode)")

        # Chat-first boot: show PlannerScreen if board is empty, else KanbanScreen
        ctx = self.ctx
        tasks = await ctx.task_service.list_tasks()
        if len(tasks) == 0:
            from kagan.ui.screens.planner import PlannerScreen

            await self.push_screen(PlannerScreen(agent_factory=self._agent_factory))
            self.log("PlannerScreen pushed (empty board)")
        else:
            await self.push_screen(KanbanScreen())
            self.log("KanbanScreen pushed, app ready")

    def _continue_after_welcome(self) -> None:
        """Called when welcome screen completes to continue app initialization."""
        self.call_later(self._run_init_after_welcome)

    async def _run_init_after_welcome(self) -> None:
        """Run initialization after welcome screen."""
        await self._initialize_app()

    async def on_unmount(self) -> None:
        """Clean up on unmount."""
        await self.cleanup()

    async def _reconcile_worktrees(self) -> None:
        """Remove orphaned worktrees from previous runs."""
        ctx = self.ctx
        tasks = await ctx.task_service.list_tasks()
        valid_ids = {t.id for t in tasks}
        cleaned = await ctx.workspace_service.cleanup_orphans(valid_ids)
        if cleaned:
            self.log(f"Cleaned up {len(cleaned)} orphan worktree(s)")

    async def _reconcile_sessions(self) -> None:
        """Kill orphaned tmux sessions from previous runs."""
        from kagan.sessions.tmux import TmuxError, run_tmux

        state = self.ctx.task_service
        try:
            output = await run_tmux("list-sessions", "-F", "#{session_name}")
            kagan_sessions = [s for s in output.split("\n") if s.startswith("kagan-")]

            tasks = await state.list_tasks()
            valid_task_ids = {t.id for t in tasks}

            for session_name in kagan_sessions:
                task_id = session_name.replace("kagan-", "")
                if task_id not in valid_task_ids:
                    # Orphaned session - task no longer exists
                    await run_tmux("kill-session", "-t", session_name)
                    self.log(f"Killed orphaned session: {session_name}")
                else:
                    # Session exists, ensure session_active flag is correct
                    await state.mark_session_active(task_id, True)
        except TmuxError:
            pass  # No tmux server running

    async def cleanup(self) -> None:
        """Terminate all agents and close resources."""
        # Stop planner agent if it's still alive
        if self.planner_state and self.planner_state.agent:
            await self.planner_state.agent.stop()
        if self.planner_state and self.planner_state.refiner:
            await self.planner_state.refiner.stop()

        # Close AppContext (handles automation + services)
        if self._ctx:
            await self._ctx.close()
            self._ctx = None

        if self._instance_lock:
            self._instance_lock.release()

    def action_show_help(self) -> None:
        """Open the help modal."""
        from kagan.ui.modals import HelpModal

        self.push_screen(HelpModal())

    def action_toggle_debug_log(self) -> None:
        """Toggle the debug log viewer (F12)."""
        from kagan.ui.modals.debug_log import DebugLogModal

        self.push_screen(DebugLogModal())


def run() -> None:
    """Run the Kagan application."""
    # Check for existing instance before starting
    instance_lock = exit_if_already_running()

    app = KaganApp()
    app._instance_lock = instance_lock
    try:
        app.run()
    finally:
        instance_lock.release()


if __name__ == "__main__":
    run()
