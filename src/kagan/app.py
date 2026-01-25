"""Main Kagan TUI application."""

from __future__ import annotations

from pathlib import Path

from textual.app import App
from textual.binding import Binding

from kagan.agents import AgentManager, Scheduler, WorktreeManager
from kagan.config import KaganConfig
from kagan.constants import DEFAULT_CONFIG_PATH, DEFAULT_DB_PATH, DEFAULT_LOCK_PATH
from kagan.database import KnowledgeBase, StateManager
from kagan.lock import InstanceLock, exit_if_already_running
from kagan.ui.screens.kanban import KanbanScreen


class KaganApp(App):
    """Kagan TUI Application - AI-powered Kanban board."""

    TITLE = "KAGAN"
    CSS_PATH = "styles/kagan.tcss"

    BINDINGS = [
        Binding("q", "quit", "Quit", show=True),
        Binding("?", "help", "Help", show=True),
    ]

    def __init__(
        self,
        db_path: str = DEFAULT_DB_PATH,
        config_path: str = DEFAULT_CONFIG_PATH,
        lock_path: str | None = DEFAULT_LOCK_PATH,
    ):
        super().__init__()
        self.db_path = Path(db_path)
        self.config_path = Path(config_path)
        self.lock_path = Path(lock_path) if lock_path else None
        self._state_manager: StateManager | None = None
        self._knowledge_base: KnowledgeBase | None = None
        self._agent_manager: AgentManager | None = None
        self._worktree_manager: WorktreeManager | None = None
        self._scheduler: Scheduler | None = None
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
    def agent_manager(self) -> AgentManager:
        assert self._agent_manager is not None
        return self._agent_manager

    @property
    def worktree_manager(self) -> WorktreeManager:
        assert self._worktree_manager is not None
        return self._worktree_manager

    @property
    def scheduler(self) -> Scheduler:
        assert self._scheduler is not None
        return self._scheduler

    async def on_mount(self) -> None:
        """Initialize app on mount."""
        self.config = KaganConfig.load(self.config_path)

        self._state_manager = StateManager(self.db_path)
        await self._state_manager.initialize()

        self._knowledge_base = KnowledgeBase(self._state_manager.connection)

        self._agent_manager = AgentManager()
        self._worktree_manager = WorktreeManager()

        self._scheduler = Scheduler(
            state_manager=self._state_manager,
            agent_manager=self._agent_manager,
            worktree_manager=self._worktree_manager,
            config=self.config,
        )

        if self.config.general.auto_start:
            self.set_interval(2.0, self._scheduler_tick)

        await self.push_screen(KanbanScreen())

    async def _scheduler_tick(self) -> None:
        """Called periodically to run scheduler tick."""
        await self.scheduler.tick()

    async def on_unmount(self) -> None:
        """Clean up on unmount."""
        await self.cleanup()

    async def cleanup(self) -> None:
        """Terminate all agents and close resources."""
        if self._agent_manager:
            await self._agent_manager.terminate_all()
        if self._state_manager:
            await self._state_manager.close()
        if self._instance_lock:
            self._instance_lock.release()

    def action_help(self) -> None:
        """Show help screen."""
        self.notify(
            "h/l=columns, j/k=cards, n=new, e=edit, d=delete, m/M=move, s=start, x=stop",
            title="Help",
            timeout=5,
        )


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
