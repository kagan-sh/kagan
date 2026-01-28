"""Main Kagan TUI application."""

from __future__ import annotations

import shlex
from pathlib import Path
from shutil import which

from textual.app import App
from textual.binding import Binding
from textual.signal import Signal

from kagan.agents.worktree import WorktreeManager
from kagan.config import KaganConfig, get_os_value
from kagan.constants import DEFAULT_CONFIG_PATH, DEFAULT_DB_PATH, DEFAULT_LOCK_PATH
from kagan.data.builtin_agents import get_builtin_agent
from kagan.database import KnowledgeBase, StateManager
from kagan.git_utils import has_git_repo, init_git_repo
from kagan.lock import InstanceLock, exit_if_already_running
from kagan.sessions import SessionManager
from kagan.theme import KAGAN_THEME
from kagan.ui.screens.agent_missing import AgentMissingScreen, MissingAgentInfo
from kagan.ui.screens.kanban import KanbanScreen


class KaganApp(App):
    """Kagan TUI Application - AI-powered Kanban board."""

    TITLE = "ᘚᘛ KAGAN"
    CSS_PATH = "styles/kagan.tcss"

    BINDINGS = [
        Binding("q", "quit", "Quit", show=True),
        Binding("?", "command_palette", "Help", show=True),
        Binding("ctrl+p", "command_palette", show=False),  # Hide from footer, still works
    ]

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

        self.db_path = Path(db_path)
        self.config_path = Path(config_path)
        self.lock_path = Path(lock_path) if lock_path else None
        self._state_manager: StateManager | None = None
        self._knowledge_base: KnowledgeBase | None = None
        self._worktree_manager: WorktreeManager | None = None
        self._session_manager: SessionManager | None = None
        self._instance_lock: InstanceLock | None = None
        self.config: KaganConfig = KaganConfig()

    @property
    def state_manager(self) -> StateManager:
        assert self._state_manager is not None
        return self._state_manager

    @property
    def knowledge_base(self) -> KnowledgeBase:
        assert self._knowledge_base is not None
        return self._knowledge_base

    @property
    def worktree_manager(self) -> WorktreeManager:
        assert self._worktree_manager is not None
        return self._worktree_manager

    @property
    def session_manager(self) -> SessionManager:
        assert self._session_manager is not None
        return self._session_manager

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

        missing_agents = self._get_missing_agents()
        if missing_agents:
            await self.push_screen(AgentMissingScreen(missing_agents))
            return

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

        if self._knowledge_base is None:
            self._knowledge_base = KnowledgeBase(self._state_manager.connection)

        # Project root is the parent of .kagan directory (where config lives)
        if self._worktree_manager is None:
            self._worktree_manager = WorktreeManager(repo_root=project_root)
        if self._session_manager is None:
            self._session_manager = SessionManager(
                project_root=project_root, state=self._state_manager
            )

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

    async def cleanup(self) -> None:
        """Terminate all agents and close resources."""
        if self._state_manager:
            await self._state_manager.close()
        if self._instance_lock:
            self._instance_lock.release()

    def _get_missing_agents(self) -> list[MissingAgentInfo]:
        selected = [self.config.general.default_worker_agent]

        missing: list[MissingAgentInfo] = []
        seen: set[str] = set()

        for agent_name in selected:
            if agent_name in seen:
                continue
            seen.add(agent_name)

            agent_config = self.config.get_agent(agent_name)
            if agent_config is None:
                continue

            run_command = get_os_value(agent_config.run_command)
            if not run_command:
                missing.append(
                    MissingAgentInfo(
                        name=agent_config.name,
                        short_name=agent_config.short_name,
                        run_command="",
                        install_command=None,
                    )
                )
                continue

            command_parts = shlex.split(run_command)
            if not command_parts or which(command_parts[0]) is None:
                builtin = get_builtin_agent(agent_name)
                missing.append(
                    MissingAgentInfo(
                        name=agent_config.name,
                        short_name=agent_config.short_name,
                        run_command=run_command,
                        install_command=builtin.install_command if builtin else None,
                    )
                )

        return missing


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
