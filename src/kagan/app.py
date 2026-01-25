"""Main Kagan TUI application."""

from pathlib import Path

from textual.app import App
from textual.binding import Binding

from kagan.config import KaganConfig, ensure_config_exists
from kagan.constants import DEFAULT_CONFIG_PATH, DEFAULT_DB_PATH
from kagan.database import StateManager
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
    ):
        super().__init__()
        self.db_path = Path(db_path)
        self.config_path = Path(config_path)
        self._state_manager: StateManager | None = None
        self.config: KaganConfig | None = None

    @property
    def state_manager(self) -> StateManager:
        assert self._state_manager is not None
        return self._state_manager

    async def on_mount(self) -> None:
        """Initialize app on mount."""
        # Load configuration
        self.config = ensure_config_exists()

        # Initialize database
        self._state_manager = StateManager(self.db_path)
        await self._state_manager.initialize()

        # Push the main Kanban screen
        await self.push_screen(KanbanScreen())

    async def on_unmount(self) -> None:
        """Clean up on unmount."""
        if self._state_manager:
            await self._state_manager.close()

    def action_help(self) -> None:
        """Show help screen."""
        self.notify(
            "Keybindings: h/l=columns, j/k=cards, n=new, e=edit, d=delete, m/M=move",
            title="Help",
            timeout=5,
        )


def run() -> None:
    """Run the Kagan application."""
    app = KaganApp()
    app.run()


if __name__ == "__main__":
    run()
