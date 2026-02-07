"""Welcome screen with project picker for multi-repo support."""

from __future__ import annotations

from datetime import UTC, datetime
from pathlib import Path
from typing import TYPE_CHECKING, cast

from textual.containers import Container, Horizontal, Vertical
from textual.widgets import Button, Footer, Label, ListItem, ListView, Static

from kagan.constants import KAGAN_LOGO
from kagan.keybindings import WELCOME_BINDINGS
from kagan.ui.screens.base import KaganScreen

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.app import KaganApp


class ProjectListItem(ListItem):
    """A project item in the recent projects list."""

    def __init__(
        self,
        project_id: str,
        name: str,
        repo_paths: list[str],
        last_opened: datetime | None,
        task_summary: str,
    ) -> None:
        super().__init__()
        self.project_id = project_id
        self.project_name = name
        self.repo_paths = repo_paths
        self.last_opened = last_opened
        self.task_summary = task_summary

    def compose(self) -> ComposeResult:
        with Horizontal(classes="project-item"):
            with Vertical(classes="project-info"):
                yield Label(self.project_name, classes="project-name")
                yield Label(
                    self._format_repos(),
                    classes="project-repos",
                )
                yield Label(
                    self.task_summary,
                    classes="project-tasks",
                )
            yield Label(
                self._format_time(),
                classes="project-time",
            )

    def _format_repos(self) -> str:
        """Format repository paths for display."""
        if not self.repo_paths:
            return "No repositories"
        if len(self.repo_paths) == 1:
            return f"-> {self.repo_paths[0]}"
        return f"-> {', '.join(Path(p).name for p in self.repo_paths)}"

    def _format_time(self) -> str:
        """Format last opened time as relative time (e.g., '2h ago', '3d ago')."""
        if not self.last_opened:
            return "Never opened"

        # Handle timezone-naive datetimes
        now = datetime.now(UTC)
        last_opened = self.last_opened
        if last_opened.tzinfo is None:
            last_opened = last_opened.replace(tzinfo=UTC)

        delta = now - last_opened
        if delta.days > 7:
            return last_opened.strftime("%b %d")
        if delta.days > 0:
            return f"{delta.days}d ago"
        if delta.seconds > 3600:
            return f"{delta.seconds // 3600}h ago"
        if delta.seconds > 60:
            return f"{delta.seconds // 60}m ago"
        return "Just now"


class WelcomeScreen(KaganScreen):
    """Welcome screen shown on startup for project selection.

    Displays a list of recent projects and allows creating new projects
    or opening folders as projects.
    """

    BINDINGS = WELCOME_BINDINGS

    def __init__(self, suggest_cwd: bool = False, cwd_path: str | None = None) -> None:
        super().__init__()
        self._suggest_cwd = suggest_cwd
        self._cwd_path = cwd_path

    @property
    def kagan_app(self) -> KaganApp:
        """Get the typed KaganApp instance."""
        return cast("KaganApp", self.app)

    def compose(self) -> ComposeResult:
        # CWD suggestion banner (shown when suggest_cwd=True)
        if self._suggest_cwd and self._cwd_path:
            with Container(id="cwd-suggestion-banner"):
                yield Label("Current directory is a git repository", id="cwd-title")
                yield Label(self._cwd_path, id="cwd-path")
                with Horizontal(id="cwd-actions"):
                    yield Button("Create Project from CWD", id="btn-cwd-create", variant="primary")
                    yield Button("Dismiss", id="btn-cwd-dismiss")

        with Container(id="welcome-container"):
            # Large ASCII art logo
            yield Static(KAGAN_LOGO, id="logo")
            yield Label("Your Development Cockpit", id="subtitle")

            # Recent projects header
            yield Label("RECENT PROJECTS", id="recent-header")

            # Project list
            yield ListView(id="project-list")

            # Empty state message
            yield Label(
                "No recent projects. Create a new project or open a folder.",
                id="empty-state",
            )

            # Action buttons
            with Horizontal(id="actions"):
                yield Button("[N] New Project", id="btn-new", variant="primary")
                yield Button("[O] Open Folder", id="btn-open")
                yield Button("[S] Settings", id="btn-settings")

        yield Footer()

    async def on_mount(self) -> None:
        """Load recent projects on mount."""
        # Context is always available after onboarding
        self.run_worker(self._load_recent_projects(), exclusive=True)

    async def _load_recent_projects(self) -> None:
        """Load and display recent projects from project service."""
        try:
            project_service = self.ctx.project_service
            projects = await project_service.list_recent_projects(limit=10)
        except (AttributeError, RuntimeError):
            # project_service not available - show empty state
            self._show_empty_state("No recent projects found.")
            return

        list_view = self.query_one("#project-list", ListView)
        empty_state = self.query_one("#empty-state", Label)

        if not projects:
            list_view.display = False
            empty_state.update("No recent projects. Create a new project or open a folder.")
            empty_state.display = True
            return

        empty_state.display = False
        list_view.display = True

        for project in projects:
            try:
                repos = await project_service.get_project_repos(project.id)
                repo_paths = [r.path for r in repos]
            except (AttributeError, RuntimeError):
                repo_paths = []

            # Get task summary
            task_summary = await self._get_task_summary(project.id)

            item = ProjectListItem(
                project_id=project.id,
                name=project.name,
                repo_paths=repo_paths,
                last_opened=project.last_opened_at,
                task_summary=task_summary,
            )
            await list_view.append(item)

    async def _get_task_summary(self, project_id: str) -> str:
        """Get a task summary for the project (e.g., '3 in progress, 2 in review')."""
        try:
            task_service = self.ctx.task_service
            tasks = await task_service.list_tasks(project_id=project_id)

            from kagan.core.models.enums import TaskStatus

            in_progress = sum(1 for t in tasks if t.status == TaskStatus.IN_PROGRESS)
            in_review = sum(1 for t in tasks if t.status == TaskStatus.REVIEW)

            if in_progress or in_review:
                return f"{in_progress} in progress, {in_review} in review"
            elif tasks:
                return f"{len(tasks)} tasks"
            else:
                return "No tasks"
        except (AttributeError, RuntimeError, TypeError):
            return "No tasks"

    def _show_empty_state(self, message: str) -> None:
        """Show the empty state with a custom message."""
        try:
            list_view = self.query_one("#project-list", ListView)
            empty_state = self.query_one("#empty-state", Label)
            list_view.display = False
            empty_state.update(message)
            empty_state.display = True
        except Exception:
            pass  # Widget not yet mounted

    def check_action(self, action: str, parameters: tuple[object, ...]) -> bool:
        """Enable actions only when valid."""
        if action == "open_selected":
            try:
                list_view = self.query_one("#project-list", ListView)
                return list_view.highlighted_child is not None
            except Exception:
                return False
        return True

    def action_new_project(self) -> None:
        """Create a new project via NewProjectModal."""
        self.app.run_worker(self._open_new_project_modal(), exclusive=True)

    async def _open_new_project_modal(self) -> None:
        from kagan.ui.modals.new_project import NewProjectModal

        result = await self.app.push_screen_wait(NewProjectModal())
        if result and "project_id" in result:
            await self._open_project(result["project_id"])

    def action_open_folder(self) -> None:
        """Open a folder as a new project or find existing project."""
        self.app.run_worker(self._open_folder_modal(), exclusive=True)

    async def _open_folder_modal(self) -> None:
        from kagan.ui.modals.folder_picker import FolderPickerModal

        folder_path = await self.app.push_screen_wait(FolderPickerModal())
        if not folder_path:
            return

        project_service = self.ctx.project_service

        # Check if this folder is already in a project
        existing = await project_service.find_project_by_repo_path(folder_path)
        if existing:
            await self._open_project(existing.id)
            return

        # Create a new project with this folder
        project_name = Path(folder_path).name
        project_id = await project_service.create_project(
            name=project_name,
            repo_paths=[folder_path],
        )
        await self._open_project(project_id)

    async def action_open_selected(self) -> None:
        """Open the currently selected project."""
        try:
            list_view = self.query_one("#project-list", ListView)
            if list_view.highlighted_child:
                item = list_view.highlighted_child
                if isinstance(item, ProjectListItem):
                    await self._open_project(item.project_id)
        except Exception:
            pass

    async def action_settings(self) -> None:
        """Open settings modal."""
        from kagan.ui.modals.settings import SettingsModal

        await self.app.push_screen(
            SettingsModal(
                config=self.ctx.config,
                config_path=self.ctx.config_path,
            )
        )

    async def action_quit(self) -> None:
        """Quit the application."""
        self.app.exit()

    async def _open_project(self, project_id: str) -> None:
        """Open a project and switch to Kanban screen."""
        try:
            project_service = self.ctx.project_service
            await project_service.open_project(project_id)

            # Store active project in app context
            # Note: This requires active_project_id to be added to AppContext
            if hasattr(self.ctx, "active_project_id"):
                self.ctx.active_project_id = project_id

            # Switch to Kanban screen
            from kagan.ui.screens.kanban import KanbanScreen

            await self.app.switch_screen(KanbanScreen())
        except Exception as e:
            self.app.notify(f"Failed to open project: {e}", severity="error")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "btn-new":
            self.action_new_project()
        elif event.button.id == "btn-open":
            self.action_open_folder()
        elif event.button.id == "btn-settings":
            self.app.run_worker(self.action_settings())
        elif event.button.id == "btn-cwd-create":
            self.app.run_worker(self._create_project_from_cwd())
        elif event.button.id == "btn-cwd-dismiss":
            self._dismiss_cwd_banner()

    async def _create_project_from_cwd(self) -> None:
        """Create a new project from the current working directory."""
        if not self._cwd_path:
            return

        project_service = self.ctx.project_service
        project_name = Path(self._cwd_path).name
        project_id = await project_service.create_project(
            name=project_name,
            repo_paths=[self._cwd_path],
        )
        await self._open_project(project_id)

    def _dismiss_cwd_banner(self) -> None:
        """Dismiss the CWD suggestion banner."""
        try:
            banner = self.query_one("#cwd-suggestion-banner", Container)
            banner.display = False
        except Exception:
            pass  # Banner not found

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        """Handle project selection from list."""
        if isinstance(event.item, ProjectListItem):
            # Use app-scoped worker to survive screen replacement
            self.app.run_worker(self._open_project(event.item.project_id))
