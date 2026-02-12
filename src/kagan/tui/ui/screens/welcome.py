"""Welcome screen with project picker for multi-repo support."""

from __future__ import annotations

import asyncio
from datetime import UTC, datetime
from pathlib import Path
from typing import TYPE_CHECKING

from sqlalchemy.exc import OperationalError
from textual.containers import Container, Horizontal
from textual.css.query import NoMatches
from textual.widgets import Button, Footer, Label, ListItem, ListView, Static, Switch

from kagan.core.adapters.db.repositories.base import RepositoryClosing
from kagan.core.constants import KAGAN_LOGO
from kagan.core.time import utc_now
from kagan.tui.keybindings import WELCOME_BINDINGS, get_key_for_action
from kagan.tui.ui.screens.base import KaganScreen
from kagan.tui.ui.utils.helpers import truncate_path
from kagan.tui.ui.widgets.keybinding_hint import KeybindingHint

if TYPE_CHECKING:
    from textual.app import ComposeResult

_PROJECT_LOAD_ERRORS = (RepositoryClosing, OperationalError, RuntimeError, ValueError)


class ProjectListItem(ListItem):
    """A project item in the recent projects list."""

    def __init__(
        self,
        project_id: str,
        name: str,
        repo_paths: list[str],
        last_opened: datetime | None,
        task_summary: str,
        index: int = 0,
    ) -> None:
        super().__init__()
        self.project_id = project_id
        self.project_name = name
        self.repo_paths = repo_paths
        self.last_opened = last_opened
        self.task_summary = task_summary
        self.index = index

    def compose(self) -> ComposeResult:
        with Horizontal(classes="project-item"):
            if self.index < 9:
                yield Label(f"[{self.index + 1}]", classes="project-shortcut")
            else:
                yield Label("   ", classes="project-shortcut")
            yield Label(self.project_name, classes="project-name")
            yield Label(f"— {self._format_repos()}", classes="project-repos")
            yield Label(f"— {self.task_summary}", classes="project-tasks")
            yield Label(self._format_time(), classes="project-time")

    def _format_repos(self) -> str:
        """Format repository paths for display with smart truncation."""
        if not self.repo_paths:
            return "📁 No repositories"
        if len(self.repo_paths) == 1:
            truncated = truncate_path(self.repo_paths[0], max_width=45)
            return f"📁 {truncated}"
        return f"📁 {', '.join(Path(p).name for p in self.repo_paths)}"

    def _format_time(self) -> str:
        """Format last opened time as relative time with arrow indicator (e.g., '2h ↵')."""
        if not self.last_opened:
            return "Never ↵"
        now = utc_now()
        last_opened = self.last_opened
        if last_opened.tzinfo is None:
            last_opened = last_opened.replace(tzinfo=UTC)

        delta = now - last_opened
        if delta.days > 7:
            return f"{last_opened.strftime('%b %d')} ↵"
        if delta.days > 0:
            return f"{delta.days}d ↵"
        if delta.seconds > 3600:
            return f"{delta.seconds // 3600}h ↵"
        if delta.seconds > 60:
            return f"{delta.seconds // 60}m ↵"
        return "Now ↵"


class WelcomeScreen(KaganScreen):
    """Welcome screen shown on startup for project selection."""

    BINDINGS = WELCOME_BINDINGS

    def __init__(
        self,
        suggest_cwd: bool = False,
        cwd_path: str | None = None,
        cwd_is_git_repo: bool = False,
        highlight_recent: bool = False,
    ) -> None:
        super().__init__()
        self._suggest_cwd = suggest_cwd
        self._cwd_path = cwd_path
        self._cwd_is_git_repo = cwd_is_git_repo
        self._highlight_recent = highlight_recent
        self._project_items: list[ProjectListItem] = []

    def compose(self) -> ComposeResult:
        with Container(id="welcome-container"):
            yield Static(KAGAN_LOGO, id="logo")
            yield Label("Your Development Cockpit", id="subtitle")
            if self._suggest_cwd and self._cwd_path:
                with Container(id="cwd-suggestion-banner"):
                    if self._cwd_is_git_repo:
                        yield Label("Current directory is a git repository", id="cwd-title")
                    else:
                        yield Label(
                            "Current directory is a folder (will initialize git repo)",
                            id="cwd-title",
                        )
                    yield Label(self._cwd_path, id="cwd-path")
                    with Horizontal(id="cwd-actions"):
                        yield Button("Create Project", id="btn-cwd-create", variant="primary")
                        yield Button("Dismiss", id="btn-cwd-dismiss")

            with Container(id="continue-highlight"):
                yield Label("💡 Continue where you left off?", id="continue-title")
                yield Label("", id="continue-project-name")
                yield Label("", id="continue-project-info")
                yield Label("Press Enter or 1 to continue", id="continue-hint")

            yield Label("RECENT PROJECTS", id="recent-header")

            yield ListView(id="project-list")

            yield Label(
                "No recent projects. Create a new project or open a folder.",
                id="empty-state",
            )

            with Horizontal(classes="toggle-row"):
                yield Switch(
                    id="auto-review-switch",
                    value=self.ctx.config.general.auto_review,
                )
                yield Label("Enable auto review", classes="toggle-text")

            with Horizontal(id="actions"):
                yield Button("New Project", id="btn-new", variant="primary")
                yield Button("Open Folder", id="btn-open")
                yield Button("Settings", id="btn-settings")

            yield Label(
                "Control Kagan from your editor via Admin MCP"
                " — docs.kagan.sh/how-to/admin-mcp-editors",
                id="admin-mcp-hint",
            )

        yield KeybindingHint(id="welcome-hint", classes="keybinding-hint")
        yield Footer(show_command_palette=False)

    async def on_mount(self) -> None:
        """Load recent projects on mount."""
        self.run_worker(
            self._load_recent_projects(),
            group="welcome-load-projects",
            exclusive=True,
            exit_on_error=False,
        )
        self._update_keybinding_hints()

    def _update_keybinding_hints(self) -> None:
        hint_widget = self.query_one("#welcome-hint", KeybindingHint)
        hint_widget.show_hints(
            [
                (get_key_for_action(WELCOME_BINDINGS, "open_selected"), "open"),
                (get_key_for_action(WELCOME_BINDINGS, "new_project"), "new"),
                (get_key_for_action(WELCOME_BINDINGS, "open_folder"), "open folder"),
                (get_key_for_action(WELCOME_BINDINGS, "settings"), "settings"),
            ]
        )

    async def _load_recent_projects(self) -> None:
        """Load and display recent projects from project service."""
        api = self.ctx.api
        projects = None
        for attempt in range(3):
            try:
                projects = await api.list_projects(limit=10)
                break
            except _PROJECT_LOAD_ERRORS as exc:
                self.log(
                    "Recent projects load failed",
                    attempt=attempt + 1,
                    error=str(exc),
                )
                if attempt == 2:
                    self._show_empty_state(
                        f"Unable to load projects from {self.ctx.db_path}. Restart and try again."
                    )
                    self._hide_continue_highlight()
                    return
                await asyncio.sleep(0.35 * (attempt + 1))

        if not self.is_mounted:
            return

        if projects is None:
            self._show_empty_state("No recent projects found.")
            self._hide_continue_highlight()
            return

        list_view = self.query_one("#project-list", ListView)
        empty_state = self.query_one("#empty-state", Label)
        list_view.clear()

        if not projects:
            list_view.display = False
            empty_state.update("No recent projects. Create a new project or open a folder.")
            empty_state.display = True
            self._hide_continue_highlight()
            return

        empty_state.display = False
        list_view.display = True
        self._project_items.clear()

        for idx, project in enumerate(projects):
            try:
                repos = await api.get_project_repos(project.id)
                repo_paths = [r.path for r in repos]
            except _PROJECT_LOAD_ERRORS as exc:
                self.log(
                    "Project repo lookup failed",
                    project_id=project.id,
                    error=str(exc),
                )
                repo_paths = []

            task_summary = await self._get_task_summary(project.id)

            item = ProjectListItem(
                project_id=project.id,
                name=project.name,
                repo_paths=repo_paths,
                last_opened=project.last_opened_at,
                task_summary=task_summary,
                index=idx,
            )
            self._project_items.append(item)
            await list_view.append(item)

        if self._highlight_recent and projects:
            self._show_continue_highlight(projects[0].name, self._project_items[0].task_summary)
        else:
            self._hide_continue_highlight()

    def _show_continue_highlight(self, project_name: str, task_summary: str) -> None:
        """Show the 'Continue where you left off' highlight box."""
        try:
            highlight = self.query_one("#continue-highlight", Container)
            highlight.display = True
            self.query_one("#continue-project-name", Label).update(f"▸ {project_name}")
            self.query_one("#continue-project-info", Label).update(f"  ({task_summary})")
        except NoMatches:
            return

    def _hide_continue_highlight(self) -> None:
        """Hide the 'Continue where you left off' highlight box."""
        try:
            highlight = self.query_one("#continue-highlight", Container)
            highlight.display = False
        except NoMatches:
            return

    async def _get_task_summary(self, project_id: str) -> str:
        """Get a task summary with status indicators."""
        try:
            tasks = await self.ctx.api.list_tasks(project_id=project_id)

            from kagan.core.models.enums import TaskStatus

            in_progress = sum(1 for t in tasks if t.status == TaskStatus.IN_PROGRESS)
            in_review = sum(1 for t in tasks if t.status == TaskStatus.REVIEW)

            parts: list[str] = []

            if in_progress:
                parts.append(f"● {in_progress} in progress")
            if in_review:
                parts.append(f"◐ {in_review} in review")

            if parts:
                return "  ".join(parts)
            elif tasks:
                return f"○ {len(tasks)} tasks"
            else:
                return "○ No tasks"
        except _PROJECT_LOAD_ERRORS as exc:
            self.log("Task summary lookup failed", project_id=project_id, error=str(exc))
            return "○ No tasks"

    def _show_empty_state(self, message: str) -> None:
        """Show the empty state with a custom message."""
        try:
            list_view = self.query_one("#project-list", ListView)
            empty_state = self.query_one("#empty-state", Label)
            list_view.display = False
            empty_state.update(message)
            empty_state.display = True
        except NoMatches:
            return

    def check_action(self, action: str, parameters: tuple[object, ...]) -> bool:
        """Enable actions only when valid."""
        if action == "open_selected":
            try:
                list_view = self.query_one("#project-list", ListView)
                return list_view.highlighted_child is not None
            except NoMatches:
                return False
        return True

    def action_new_project(self) -> None:
        """Create a new project via NewProjectModal."""
        self.app.run_worker(
            self._open_new_project_modal(),
            group="welcome-new-project",
            exclusive=True,
            exit_on_error=False,
        )

    async def _open_new_project_modal(self) -> None:
        from kagan.tui.ui.modals.new_project import NewProjectModal

        result = await self.app.push_screen_wait(NewProjectModal())
        if result and "project_id" in result:
            await self._open_project(result["project_id"])

    def action_open_folder(self) -> None:
        """Open a folder as a new project or find existing project."""
        self.app.run_worker(
            self._open_folder_modal(),
            group="welcome-open-folder",
            exclusive=True,
            exit_on_error=False,
        )

    async def _open_folder_modal(self) -> None:
        from kagan.tui.ui.modals.folder_picker import FolderPickerModal

        folder_path = await self.app.push_screen_wait(FolderPickerModal())
        if not folder_path:
            return

        api = self.ctx.api

        existing = await api.find_project_by_repo_path(folder_path)
        if existing:
            await self._open_project(existing.id)
            return

        project_name = Path(folder_path).name
        project_id = await api.create_project(
            name=project_name,
            repo_paths=[folder_path],
        )
        await self._open_project(project_id)

    def action_open_selected(self) -> None:
        """Open the currently selected project."""
        self.app.run_worker(
            self._open_selected_project(),
            group="welcome-open-selected",
            exclusive=True,
            exit_on_error=False,
        )

    async def _open_selected_project(self) -> None:
        try:
            list_view = self.query_one("#project-list", ListView)
        except NoMatches:
            return
        if list_view.highlighted_child:
            item = list_view.highlighted_child
            if isinstance(item, ProjectListItem):
                await self._open_project(item.project_id)

    async def action_settings(self) -> None:
        from kagan.tui.ui.modals.settings import SettingsModal

        await self.app.push_screen(
            SettingsModal(
                config=self.ctx.config,
                config_path=self.ctx.config_path,
            )
        )

    async def action_quit(self) -> None:
        """Quit the application."""
        self.app.exit()

    def _open_project_by_index(self, index: int) -> None:
        """Open project by 0-based index."""
        if index < len(self._project_items):
            project_id = self._project_items[index].project_id
            self.app.run_worker(
                self._open_project(project_id),
                group="welcome-open-project",
                exclusive=True,
                exit_on_error=False,
            )

    def action_open_project(self, index: str) -> None:
        """Open project by parameterized 0-based index."""
        self._open_project_by_index(int(index))

    async def _open_project(self, project_id: str) -> None:
        """Open a project and switch to the project board."""
        try:
            opened = await self.kagan_app.open_project_session(
                project_id,
                allow_picker=True,
                screen_mode="switch",
            )
            if not opened:
                self.notify("Unable to open project", severity="warning")
        except ValueError as exc:
            self.app.notify(f"Failed to open project: {exc}", severity="error")
        except (RepositoryClosing, OperationalError):
            self.app.notify("Project data is unavailable during shutdown", severity="warning")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "btn-new":
            self.action_new_project()
        elif event.button.id == "btn-open":
            self.action_open_folder()
        elif event.button.id == "btn-settings":
            self.app.run_worker(
                self.action_settings(),
                group="welcome-open-settings",
                exclusive=True,
                exit_on_error=False,
            )
        elif event.button.id == "btn-cwd-create":
            self.app.run_worker(
                self._create_project_from_cwd(),
                group="welcome-create-from-cwd",
                exclusive=True,
                exit_on_error=False,
            )
        elif event.button.id == "btn-cwd-dismiss":
            self._dismiss_cwd_banner()

    def on_switch_changed(self, event: Switch.Changed) -> None:
        """Persist auto review preference from the welcome screen."""
        if event.switch.id != "auto-review-switch":
            return
        self.ctx.config.general.auto_review = event.value
        self.app.run_worker(
            self.ctx.config.save(self.ctx.config_path),
            group="welcome-save-auto-review",
            exclusive=True,
            exit_on_error=False,
        )

    async def _create_project_from_cwd(self) -> None:
        """Create a new project from the current working directory.

        When the CWD is not a git repo, initializes one with an initial commit
        before creating the project.
        """
        if not self._cwd_path:
            return

        cwd = Path(self._cwd_path)

        if not self._cwd_is_git_repo:
            from kagan.core.git_utils import init_git_repo

            self.notify("Initializing git repository...", severity="information")
            result = await init_git_repo(cwd, base_branch="main")
            if not result.success:
                msg = result.error.message if result.error else "Unknown error"
                details = result.error.details if result.error else ""
                self.notify(
                    f"Failed to initialize git repository: {msg}\n{details}",
                    severity="error",
                )
                return
            self.notify("Git repository initialized", severity="information")

        api = self.ctx.api
        project_name = cwd.name
        project_id = await api.create_project(
            name=project_name,
            repo_paths=[self._cwd_path],
        )
        await self._open_project(project_id)

    def _dismiss_cwd_banner(self) -> None:
        """Dismiss the CWD suggestion banner."""
        try:
            banner = self.query_one("#cwd-suggestion-banner", Container)
            banner.display = False
        except NoMatches:
            return

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        """Handle project selection from list."""
        if isinstance(event.item, ProjectListItem):
            self.app.run_worker(
                self._open_project(event.item.project_id),
                group="welcome-open-project",
                exclusive=True,
                exit_on_error=False,
            )
