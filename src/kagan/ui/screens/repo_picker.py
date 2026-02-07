"""Repository picker screen for multi-repo projects."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

from textual.binding import Binding
from textual.containers import Container, Horizontal
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Label, ListItem, ListView, Static

from kagan.constants import KAGAN_LOGO
from kagan.ui.utils.path import truncate_path
from kagan.ui.widgets.keybinding_hint import KeybindingHint

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.adapters.db.schema import Project, Repo
    from kagan.app import KaganApp
    from kagan.bootstrap import AppContext


class RepoListItem(ListItem):
    """A repository item in the repo picker list."""

    def __init__(
        self,
        repo: Repo,
        is_current: bool = False,
        task_count: int = 0,
    ) -> None:
        super().__init__()
        self.repo = repo
        self.is_current = is_current
        self.task_count = task_count

    def compose(self) -> ComposeResult:
        """Compose the repository list item."""
        indicator = "●" if self.is_current else "○"
        task_label = "task" if self.task_count == 1 else "tasks"
        display_name = self.repo.display_name or self.repo.name
        path = truncate_path(self.repo.path, max_width=30)

        with Horizontal(classes="repo-item"):
            yield Label(indicator, classes="repo-indicator")
            yield Label(display_name, classes="repo-name")
            yield Label(f"— {path}", classes="repo-path")
            yield Label(f"{self.task_count} {task_label}", classes="repo-tasks")


class RepoPickerScreen(ModalScreen[str | None]):
    """Screen for selecting a repository in multi-repo projects."""

    BINDINGS = [
        Binding("escape", "cancel", "Cancel", show=False),
        Binding("enter", "select", "Select", show=False),
        Binding("k,up", "cursor_up", "Previous", show=False),
        Binding("j,down", "cursor_down", "Next", show=False),
        Binding("n", "add_repo", "Add Repo", show=False),
    ]

    def __init__(
        self,
        project: Project,
        repositories: list[Repo],
        current_repo_id: str | None = None,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self._project = project
        self._repositories = repositories
        self._current_repo_id = current_repo_id
        self._repo_items: list[RepoListItem] = []

    @property
    def kagan_app(self) -> KaganApp:
        """Get the typed KaganApp instance."""
        return cast("KaganApp", self.app)

    @property
    def ctx(self) -> AppContext:
        """Get the application context for service access."""
        app = self.kagan_app
        if not hasattr(app, "_ctx") or app._ctx is None:
            msg = "AppContext not initialized. Ensure bootstrap has completed."
            raise RuntimeError(msg)
        return app._ctx

    def compose(self) -> ComposeResult:
        """Compose the repository picker layout."""
        with Container(id="repo-picker-container"):
            yield Static(KAGAN_LOGO, id="repo-picker-logo")
            yield Label("Select Repository", id="repo-picker-subtitle")
            yield Label(f"PROJECT: {self._project.name}", id="repo-picker-project-name")
            yield Label("No repositories yet. Add one to continue.", id="repo-picker-empty")

            yield ListView(id="repo-list")

            with Horizontal(id="repo-picker-actions"):
                yield Button("Add Repo", id="btn-add-repo", variant="primary")
                yield Button("Close", id="btn-close")

        yield KeybindingHint(id="repo-picker-hint", classes="keybinding-hint")
        yield Footer(show_command_palette=False)

    async def on_mount(self) -> None:
        """Populate the repository list on mount."""
        self._update_keybinding_hints()
        await self._refresh_repos(highlight_repo_id=self._current_repo_id)

    def _update_keybinding_hints(self) -> None:
        hint_widget = self.query_one("#repo-picker-hint", KeybindingHint)
        hint_widget.show_hints(
            [
                ("↑/↓", "navigate"),
                ("Enter", "select"),
                ("n", "add repo"),
                ("Esc", "cancel"),
            ]
        )

    async def _refresh_repos(self, *, highlight_repo_id: str | None = None) -> None:
        list_view = self.query_one("#repo-list", ListView)
        empty_state = self.query_one("#repo-picker-empty", Label)
        await list_view.clear()
        self._repo_items.clear()

        self._repositories = await self.ctx.project_service.get_project_repos(self._project.id)
        if not self._repositories:
            empty_state.display = True
            list_view.display = False
            return
        empty_state.display = False
        list_view.display = True
        for repo in self._repositories:
            is_current = repo.id == self._current_repo_id
            task_count = await self._get_repo_task_count(repo.id)
            item = RepoListItem(repo=repo, is_current=is_current, task_count=task_count)
            self._repo_items.append(item)
            await list_view.append(item)

        target_repo_id = highlight_repo_id or self._current_repo_id
        if target_repo_id:
            for idx, item in enumerate(self._repo_items):
                if item.repo.id == target_repo_id:
                    list_view.index = idx
                    break

    async def action_add_repo(self) -> None:
        from pathlib import Path

        from kagan.ui.modals.folder_picker import FolderPickerModal

        folder_path = await self.app.push_screen_wait(FolderPickerModal())
        if not folder_path:
            return

        resolved = str(Path(folder_path).expanduser().resolve())
        existing = [Path(repo.path).resolve() for repo in self._repositories]
        if Path(resolved).resolve() in existing:
            self.notify("Repository already added to this project", severity="warning")
            return

        resolved_path = Path(resolved)
        if not (resolved_path / ".git").exists():
            from kagan.git_utils import init_git_repo

            self.notify("Initializing git repository...", severity="information")
            result = await init_git_repo(resolved_path, base_branch="main")
            if not result.success:
                msg = result.error.message if result.error else "Unknown error"
                details = result.error.details if result.error else ""
                self.notify(
                    f"Failed to initialize git repository: {msg}\n{details}",
                    severity="error",
                )
                return
            self.notify("Git repository initialized", severity="information")

        repo_id = await self.ctx.project_service.add_repo_to_project(
            project_id=self._project.id,
            repo_path=resolved,
            is_primary=False,
        )
        await self._refresh_repos(highlight_repo_id=repo_id)
        if repo_id:
            self.dismiss(repo_id)
        self.notify("Repository added", severity="information")

    def action_close(self) -> None:
        self.action_cancel()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-add-repo":
            self.run_worker(self.action_add_repo())
        elif event.button.id == "btn-close":
            self.action_cancel()

    async def _get_repo_task_count(self, repo_id: str) -> int:
        """Get the number of tasks associated with a repository."""
        try:
            task_service = self.ctx.task_service
            tasks = await task_service.list_tasks(project_id=self._project.id)

            return len(tasks)
        except (AttributeError, RuntimeError):
            return 0

    def action_cancel(self) -> None:
        """Cancel and dismiss without selection."""
        self.dismiss(None)

    def action_select(self) -> None:
        """Select the highlighted repository and dismiss."""
        list_view = self.query_one("#repo-list", ListView)
        if list_view.highlighted_child and isinstance(list_view.highlighted_child, RepoListItem):
            self.dismiss(list_view.highlighted_child.repo.id)
        else:
            self.dismiss(None)

    def action_cursor_up(self) -> None:
        """Move cursor up in the list."""
        list_view = self.query_one("#repo-list", ListView)
        list_view.action_cursor_up()

    def action_cursor_down(self) -> None:
        """Move cursor down in the list."""
        list_view = self.query_one("#repo-list", ListView)
        list_view.action_cursor_down()

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        """Handle double-click or Enter on list item."""
        if isinstance(event.item, RepoListItem):
            self.dismiss(event.item.repo.id)
