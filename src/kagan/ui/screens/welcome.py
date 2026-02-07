"""Welcome screen with project picker for multi-repo support."""

from __future__ import annotations

from datetime import UTC, datetime
from pathlib import Path
from typing import TYPE_CHECKING, cast

from textual.containers import Container, Horizontal
from textual.widgets import Button, Footer, Label, ListItem, ListView, Static

from kagan.constants import KAGAN_LOGO
from kagan.keybindings import WELCOME_BINDINGS, get_key_for_action
from kagan.ui.screens.base import KaganScreen
from kagan.ui.utils.path import truncate_path
from kagan.ui.widgets.keybinding_hint import KeybindingHint

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
        index: int = 0,
    ) -> None:
        super().__init__()
        self.project_id = project_id
        self.project_name = name
        self.repo_paths = repo_paths
        self.last_opened = last_opened
        self.task_summary = task_summary
        self.index = index  # 0-based index for number key shortcuts

    def compose(self) -> ComposeResult:
        with Horizontal(classes="project-item"):
            # Number shortcut indicator (1-9)
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
        # Multiple repos: show abbreviated names
        return f"📁 {', '.join(Path(p).name for p in self.repo_paths)}"

    def _format_time(self) -> str:
        """Format last opened time as relative time with arrow indicator (e.g., '2h ↵')."""
        if not self.last_opened:
            return "Never ↵"

        # Handle timezone-naive datetimes
        now = datetime.now(UTC)
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
    """Welcome screen shown on startup for project selection.

    Displays a list of recent projects and allows creating new projects
    or opening folders as projects.
    """

    BINDINGS = WELCOME_BINDINGS

    def __init__(
        self,
        suggest_cwd: bool = False,
        cwd_path: str | None = None,
        highlight_recent: bool = False,
    ) -> None:
        super().__init__()
        self._suggest_cwd = suggest_cwd
        self._cwd_path = cwd_path
        self._highlight_recent = highlight_recent
        self._project_items: list[ProjectListItem] = []  # Track items for number keys

    @property
    def kagan_app(self) -> KaganApp:
        """Get the typed KaganApp instance."""
        return cast("KaganApp", self.app)

    def compose(self) -> ComposeResult:
        with Container(id="welcome-container"):
            # Large ASCII art logo
            yield Static(KAGAN_LOGO, id="logo")
            yield Label("Your Development Cockpit", id="subtitle")

            # CWD suggestion banner (shown when suggest_cwd=True) - inside welcome container
            if self._suggest_cwd and self._cwd_path:
                with Container(id="cwd-suggestion-banner"):
                    yield Label("💡 Current directory is a git repository", id="cwd-title")
                    yield Label(self._cwd_path, id="cwd-path")
                    with Horizontal(id="cwd-actions"):
                        yield Button("Create Project", id="btn-cwd-create", variant="primary")
                        yield Button("Dismiss", id="btn-cwd-dismiss")

            # "Continue where you left off?" highlight box (shown when highlight_recent=True)
            with Container(id="continue-highlight"):
                yield Label("💡 Continue where you left off?", id="continue-title")
                yield Label("", id="continue-project-name")
                yield Label("", id="continue-project-info")
                yield Label("Press Enter or 1 to continue", id="continue-hint")

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
                yield Button("New Project", id="btn-new", variant="primary")
                yield Button("Open Folder", id="btn-open")
                yield Button("Settings", id="btn-settings")

        yield KeybindingHint(id="welcome-hint", classes="keybinding-hint")
        yield Footer(show_command_palette=False)

    async def on_mount(self) -> None:
        """Load recent projects on mount."""
        # Context is always available after onboarding
        self.run_worker(self._load_recent_projects(), exclusive=True)
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
        try:
            project_service = self.ctx.project_service
            projects = await project_service.list_recent_projects(limit=10)
        except (AttributeError, RuntimeError):
            # project_service not available - show empty state
            self._show_empty_state("No recent projects found.")
            self._hide_continue_highlight()
            return

        list_view = self.query_one("#project-list", ListView)
        empty_state = self.query_one("#empty-state", Label)

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
                index=idx,
            )
            self._project_items.append(item)
            await list_view.append(item)

        # Highlight the most recent project if enabled
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
        except Exception:
            pass

    def _hide_continue_highlight(self) -> None:
        """Hide the 'Continue where you left off' highlight box."""
        try:
            highlight = self.query_one("#continue-highlight", Container)
            highlight.display = False
        except Exception:
            pass

    async def _get_task_summary(self, project_id: str) -> str:
        """Get a task summary with status indicators.

        Status indicators:
        - ● = Active work (has in_progress tasks)
        - ◐ = Needs attention (has review tasks)
        - ○ = Idle (only backlog/done)
        """
        try:
            task_service = self.ctx.task_service
            tasks = await task_service.list_tasks(project_id=project_id)

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
        except (AttributeError, RuntimeError, TypeError):
            return "○ No tasks"

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

    def action_open_selected(self) -> None:
        """Open the currently selected project."""
        self.app.run_worker(self._open_selected_project(), exclusive=True)

    async def _open_selected_project(self) -> None:
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

    # Number key shortcuts for quick project selection
    def _open_project_by_index(self, index: int) -> None:
        """Open project by 0-based index."""
        if index < len(self._project_items):
            project_id = self._project_items[index].project_id
            self.app.run_worker(self._open_project(project_id))

    def action_open_project_1(self) -> None:
        self._open_project_by_index(0)

    def action_open_project_2(self) -> None:
        self._open_project_by_index(1)

    def action_open_project_3(self) -> None:
        self._open_project_by_index(2)

    def action_open_project_4(self) -> None:
        self._open_project_by_index(3)

    def action_open_project_5(self) -> None:
        self._open_project_by_index(4)

    def action_open_project_6(self) -> None:
        self._open_project_by_index(5)

    def action_open_project_7(self) -> None:
        self._open_project_by_index(6)

    def action_open_project_8(self) -> None:
        self._open_project_by_index(7)

    def action_open_project_9(self) -> None:
        self._open_project_by_index(8)

    async def _open_project(self, project_id: str) -> None:
        """Open a project and switch to appropriate screen.

        If the project has no tasks, opens PlannerScreen.
        Otherwise, opens KanbanScreen.
        """
        try:
            project_service = self.ctx.project_service
            project = await project_service.open_project(project_id)

            # Store active project in app context
            # Note: This requires active_project_id to be added to AppContext
            if hasattr(self.ctx, "active_project_id"):
                self.ctx.active_project_id = project_id

            repos = await project_service.get_project_repos(project_id)
            if repos:
                if not await self.kagan_app._set_active_repo_for_project(
                    project, allow_picker=True
                ):
                    return
            else:
                self.kagan_app._clear_active_repo()
                from kagan.ui.screens.repo_picker import RepoPickerScreen

                selected_repo_id = await self.app.push_screen_wait(
                    RepoPickerScreen(project=project, repositories=repos, current_repo_id=None)
                )
                if not selected_repo_id:
                    self.notify("Add a repository to continue", severity="warning")
                    return
                repos = await project_service.get_project_repos(project_id)
                selected_repo = next((repo for repo in repos if repo.id == selected_repo_id), None)
                if selected_repo is None:
                    self.notify("No repository selected", severity="warning")
                    return
                self.kagan_app._apply_active_repo(selected_repo)

            # Check if board is empty - if so, open Planner instead of Kanban
            tasks = await self.ctx.task_service.list_tasks(project_id=project_id)
            if len(tasks) == 0:
                from kagan.ui.screens.planner import PlannerScreen

                await self.app.switch_screen(PlannerScreen())
            else:
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
