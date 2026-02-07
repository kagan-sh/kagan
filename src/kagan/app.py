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
from kagan.git_utils import has_git_repo
from kagan.keybindings import APP_BINDINGS
from kagan.lock import InstanceLock, exit_if_already_running
from kagan.terminal import supports_truecolor
from kagan.theme import KAGAN_THEME, KAGAN_THEME_256
from kagan.ui.screens.kanban import KanbanScreen
from kagan.ui.screens.onboarding import OnboardingScreen

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
        if not self._config_exists():
            # First boot - show onboarding screen to collect initial settings
            await self.push_screen(OnboardingScreen())
            return  # _continue_after_onboarding will be called when onboarding finishes

        await self._initialize_app()

    def _config_exists(self) -> bool:
        """Check if the config file exists (determines first boot vs normal boot)."""
        return self.config_path.exists()

    async def _initialize_app(self) -> None:
        """Initialize all app components."""
        self.config = KaganConfig.load(self.config_path)
        self.log("Config loaded", path=str(self.config_path))
        self.log.debug("Config settings", auto_start=self.config.general.auto_start)

        if self._ctx is None:
            self._ctx = await create_app_context(
                self.config_path,
                self.db_path,
                config=self.config,
                project_root=self.project_root,
                agent_factory=self._agent_factory,
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

        # Determine startup screen based on project context
        await self._startup_screen_decision()

    def _continue_after_welcome(self) -> None:
        """Called when welcome screen completes to continue app initialization."""
        self.call_later(self._run_init_after_welcome)

    async def _run_init_after_welcome(self) -> None:
        """Run initialization after welcome screen."""
        await self._initialize_app()

    def on_onboarding_screen_completed(self, message: OnboardingScreen.Completed) -> None:
        """Handle OnboardingScreen.Completed message."""
        # Store the config from onboarding
        self.config = message.config
        self.call_later(self._continue_after_onboarding)

    async def _continue_after_onboarding(self) -> None:
        """Called after OnboardingScreen completes to continue startup flow.

        Pops the onboarding screen, reinitializes context (now that config exists),
        and continues to the normal startup screen decision flow.
        """
        # Pop the onboarding screen
        self.pop_screen()

        # Initialize the app now that config exists
        await self._initialize_app()

    async def _startup_screen_decision(self) -> None:
        """Decide which screen to show on startup based on project context.

        Flow:
        1. If CWD is in an existing project → open that project
        2. If CWD is a git repo not in any project → show WelcomeScreen with CWD suggestion
        3. Otherwise → show welcome screen for project selection
        """
        ctx = self.ctx
        cwd = self.project_root

        # Try to find existing project containing this path
        project = await ctx.project_service.find_project_by_repo_path(str(cwd))
        if project:
            # CWD is in an existing project - open it
            self.log("Detected project from CWD", project_id=project.id)
            ctx.active_project_id = project.id
            await ctx.project_service.open_project(project.id)
            await self._push_main_screen()
            return

        # Check if CWD is a git repo not in any project
        suggest_cwd = await has_git_repo(cwd)
        cwd_path = str(cwd) if suggest_cwd else None

        # Show welcome screen (with CWD suggestion if applicable)
        from kagan.ui.screens.welcome import WelcomeScreen

        await self.push_screen(WelcomeScreen(suggest_cwd=suggest_cwd, cwd_path=cwd_path))
        self.log(
            "WelcomeScreen pushed",
            suggest_cwd=suggest_cwd,
            cwd_path=cwd_path,
        )

    async def _push_main_screen(self) -> None:
        """Push the main screen (Planner if empty, Kanban otherwise)."""
        ctx = self.ctx
        tasks = await ctx.task_service.list_tasks()

        if len(tasks) == 0:
            from kagan.ui.screens.planner import PlannerScreen

            await self.push_screen(PlannerScreen(agent_factory=self._agent_factory))
            self.log("PlannerScreen pushed (empty board)")
        else:
            await self.push_screen(KanbanScreen())
            self.log("KanbanScreen pushed, app ready")

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
        from kagan.tmux import TmuxError, run_tmux

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
