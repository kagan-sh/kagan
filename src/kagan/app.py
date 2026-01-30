"""Main Kagan TUI application."""

from __future__ import annotations

from pathlib import Path

from textual.app import App
from textual.signal import Signal

from kagan.agents.scheduler import Scheduler
from kagan.agents.worktree import WorktreeManager
from kagan.config import KaganConfig
from kagan.constants import (
    DEFAULT_CONFIG_PATH,
    DEFAULT_DB_PATH,
    DEFAULT_LOCK_PATH,
    TICK_INTERVAL,
)
from kagan.database import StateManager
from kagan.git_utils import has_git_repo, init_git_repo
from kagan.keybindings import APP_BINDINGS, to_textual_bindings
from kagan.lock import InstanceLock, exit_if_already_running
from kagan.sessions import SessionManager
from kagan.theme import KAGAN_THEME
from kagan.ui.screens.kanban import KanbanScreen


class KaganApp(App):
    """Kagan TUI Application - AI-powered Kanban board."""

    TITLE = "ᘚᘛ KAGAN"
    CSS_PATH = "styles/kagan.tcss"

    BINDINGS = to_textual_bindings(APP_BINDINGS)

    def __init__(
        self,
        db_path: str = DEFAULT_DB_PATH,
        config_path: str = DEFAULT_CONFIG_PATH,
        lock_path: str | None = DEFAULT_LOCK_PATH,
    ):
        super().__init__()
        # Register the Kagan theme before anything else
        self.register_theme(KAGAN_THEME)
        self.theme = "kagan"

        # Pub/sub signal for ticket changes - screens subscribe to this
        self.ticket_changed_signal: Signal[str] = Signal(self, "ticket_changed")
        self.iteration_changed_signal: Signal[tuple[str, int]] = Signal(self, "iteration_changed")

        self.db_path = Path(db_path)
        self.config_path = Path(config_path)
        self.lock_path = Path(lock_path) if lock_path else None
        self._state_manager: StateManager | None = None
        self._worktree_manager: WorktreeManager | None = None
        self._session_manager: SessionManager | None = None
        self._scheduler: Scheduler | None = None
        self._instance_lock: InstanceLock | None = None
        self.config: KaganConfig = KaganConfig()

    @property
    def state_manager(self) -> StateManager:
        assert self._state_manager is not None
        return self._state_manager

    @property
    def worktree_manager(self) -> WorktreeManager:
        assert self._worktree_manager is not None
        return self._worktree_manager

    @property
    def session_manager(self) -> SessionManager:
        assert self._session_manager is not None
        return self._session_manager

    @property
    def scheduler(self) -> Scheduler:
        assert self._scheduler is not None
        return self._scheduler

    async def on_mount(self) -> None:
        """Initialize app on mount."""
        # Check for first boot (no config.toml file)
        # Note: .kagan folder may already exist (created by lock file),
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

        project_root = self.config_path.parent.parent
        if not has_git_repo(project_root):
            base_branch = self.config.general.default_base_branch
            if init_git_repo(project_root, base_branch):
                self.log("Initialized git repository", base_branch=base_branch)
            else:
                self.log.warning("Failed to initialize git repository", path=str(project_root))

        # Only initialize managers if not already set (allows test mocking)
        if self._state_manager is None:
            self._state_manager = StateManager(
                self.db_path,
                on_change=lambda tid: self.ticket_changed_signal.publish(tid),
            )
            await self._state_manager.initialize()
            self.log("Database initialized", path=str(self.db_path))

        # Project root is the parent of .kagan directory (where config lives)
        if self._worktree_manager is None:
            self._worktree_manager = WorktreeManager(repo_root=project_root)
        if self._session_manager is None:
            self._session_manager = SessionManager(
                project_root=project_root, state=self._state_manager, config=self.config
            )
            # Reconcile orphaned sessions from previous runs
            await self._reconcile_sessions()

        if self._scheduler is None:
            self._scheduler = Scheduler(
                state_manager=self._state_manager,
                worktree_manager=self._worktree_manager,
                config=self.config,
                session_manager=self._session_manager,
                on_ticket_changed=lambda: self.ticket_changed_signal.publish(""),
                on_iteration_changed=lambda tid, it: self.iteration_changed_signal.publish(
                    (tid, it)
                ),
            )
            # Start scheduler tick loop
            self.set_interval(TICK_INTERVAL, self._scheduler_tick)
            self.log("Scheduler initialized", auto_start=self.config.general.auto_start)

        # Chat-first boot: show PlannerScreen if board is empty, else KanbanScreen
        tickets = await self._state_manager.get_all_tickets()
        if len(tickets) == 0:
            from kagan.ui.screens.planner import PlannerScreen

            await self.push_screen(PlannerScreen())
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

    async def _scheduler_tick(self) -> None:
        """Run one scheduler tick."""
        if self._scheduler:
            await self._scheduler.tick()

    async def _reconcile_sessions(self) -> None:
        """Kill orphaned tmux sessions from previous runs."""
        from kagan.sessions.tmux import TmuxError, run_tmux

        state = self.state_manager  # Uses property, ensures not None
        try:
            output = await run_tmux("list-sessions", "-F", "#{session_name}")
            kagan_sessions = [s for s in output.split("\n") if s.startswith("kagan-")]

            tickets = await state.get_all_tickets()
            valid_ticket_ids = {t.id for t in tickets}

            for session_name in kagan_sessions:
                ticket_id = session_name.replace("kagan-", "")
                if ticket_id not in valid_ticket_ids:
                    # Orphaned session - ticket no longer exists
                    await run_tmux("kill-session", "-t", session_name)
                    self.log(f"Killed orphaned session: {session_name}")
                else:
                    # Session exists, ensure session_active flag is correct
                    await state.mark_session_active(ticket_id, True)
        except TmuxError:
            pass  # No tmux server running

    async def cleanup(self) -> None:
        """Terminate all agents and close resources."""
        if self._scheduler:
            await self._scheduler.stop()
        if self._state_manager:
            await self._state_manager.close()
        if self._instance_lock:
            self._instance_lock.release()

    def action_show_help(self) -> None:
        """Open the help modal."""
        from kagan.ui.modals import HelpModal

        self.push_screen(HelpModal())


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
