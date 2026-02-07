"""Main Kagan TUI application."""

from __future__ import annotations

import asyncio
from pathlib import Path
from typing import TYPE_CHECKING

from textual.app import App, SystemCommand
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
)
from kagan.debug_log import setup_debug_logging
from kagan.git_utils import has_git_repo
from kagan.instance_lock import InstanceLock, LockInfo
from kagan.keybindings import APP_BINDINGS
from kagan.terminal import supports_truecolor
from kagan.theme import KAGAN_THEME, KAGAN_THEME_256
from kagan.ui.screens.kanban import KanbanScreen
from kagan.ui.screens.onboarding import OnboardingScreen

if TYPE_CHECKING:
    from collections.abc import Iterable

    from textual.screen import Screen

    from kagan.adapters.db.schema import Project, Repo
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
        project_root: str | Path | None = None,
        agent_factory: AgentFactory = create_agent,
    ):
        super().__init__()

        self.register_theme(KAGAN_THEME)
        self.register_theme(KAGAN_THEME_256)

        if supports_truecolor():
            self.theme = "kagan"
        else:
            self.theme = "kagan-256"

        self.task_changed_signal: Signal[str] = Signal(self, "task_changed")

        self.db_path = Path(db_path)
        self.config_path = Path(config_path)
        self.project_root = Path(project_root) if project_root else Path.cwd()
        self._ctx: AppContext | None = None
        self.config: KaganConfig = KaganConfig()
        self.planner_state: PersistentPlannerState | None = None
        self._agent_factory = agent_factory
        self._instance_lock: InstanceLock | None = None

    @property
    def ctx(self) -> AppContext:
        """Get the application context for service access."""
        assert self._ctx is not None, "AppContext not initialized"
        return self._ctx

    async def on_mount(self) -> None:
        """Initialize app on mount."""
        setup_debug_logging()

        # Acquire per-repo instance lock before any initialization
        self._instance_lock = InstanceLock(self.project_root)
        if not self._instance_lock.acquire():
            from kagan.ui.modals.instance_locked import InstanceLockedModal

            lock_info = self._instance_lock.get_holder_info()
            await self.push_screen(InstanceLockedModal(lock_info))
            return

        if not self._config_exists():
            await self.push_screen(OnboardingScreen())
            return

        await self._initialize_app()

    def _config_exists(self) -> bool:
        """Check if the config file exists (determines first boot vs normal boot)."""
        return self.config_path.exists()

    async def _initialize_app(self) -> None:
        """Initialize all app components."""
        self.config = KaganConfig.load(self.config_path)
        self.log("Config loaded", path=str(self.config_path))

        if self._ctx is None:
            self._ctx = await create_app_context(
                self.config_path,
                self.db_path,
                config=self.config,
                project_root=self.project_root,
                agent_factory=self._agent_factory,
            )
            ctx = self._ctx

            bridge = create_signal_bridge(ctx.event_bus)
            wire_default_signals(bridge, self)
            ctx.signal_bridge = bridge
            self.log("AppContext initialized with SignalBridge")

            await self._reconcile_worktrees()
            await self._reconcile_sessions()

            await ctx.automation_service.start()
            self.log("Automation service initialized (reactive mode)")

        await self._startup_screen_decision()

    def _continue_after_welcome(self) -> None:
        """Called when welcome screen completes to continue app initialization."""
        self.call_later(self._run_init_after_welcome)

    async def _run_init_after_welcome(self) -> None:
        """Run initialization after welcome screen."""
        await self._initialize_app()

    def on_onboarding_screen_completed(self, message: OnboardingScreen.Completed) -> None:
        """Handle OnboardingScreen.Completed message."""
        self.config = message.config
        self.call_later(self._continue_after_onboarding)

    async def _continue_after_onboarding(self) -> None:
        """Called after OnboardingScreen completes to continue startup flow.

        Pops the onboarding screen, reinitializes context (now that config exists),
        and continues to the normal startup screen decision flow.
        """

        self.pop_screen()

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

        project = await ctx.project_service.find_project_by_repo_path(str(cwd))
        if project:
            self.log("Detected project from CWD", project_id=project.id)
            ctx.active_project_id = project.id
            project = await ctx.project_service.open_project(project.id)
            await self._set_active_repo_for_project(project, preferred_path=cwd, allow_picker=False)
            await self._push_main_screen()
            return

        suggest_cwd = await has_git_repo(cwd)
        cwd_path = str(cwd) if suggest_cwd else None

        from kagan.ui.screens.welcome import WelcomeScreen

        await self.push_screen(WelcomeScreen(suggest_cwd=suggest_cwd, cwd_path=cwd_path))
        self.log(
            "WelcomeScreen pushed",
            suggest_cwd=suggest_cwd,
            cwd_path=cwd_path,
        )

    async def _set_active_repo_for_project(
        self,
        project: Project,
        *,
        preferred_path: Path | None = None,
        allow_picker: bool = True,
    ) -> bool:
        """Resolve and apply the active repo for a project.

        Returns False if repo selection was cancelled or the repo is locked.
        """
        repo = await self._select_repo_for_project(
            project,
            preferred_path=preferred_path,
            allow_picker=allow_picker,
        )
        if repo is None:
            self._clear_active_repo()
            return False
        return await self._apply_active_repo(repo)

    async def _select_repo_for_project(
        self,
        project: Project,
        *,
        preferred_path: Path | None = None,
        allow_picker: bool = True,
    ) -> Repo | None:
        """Return the repo to use for a project, optionally using a picker."""
        repos = await self.ctx.project_service.get_project_repos(project.id)
        if not repos:
            return None

        if preferred_path is not None:
            matched = self._match_repo_for_path(repos, preferred_path)
            if matched:
                return matched

        if len(repos) == 1 or not allow_picker:
            return repos[0]

        current_repo_id = self.ctx.active_repo_id
        if current_repo_id is not None and all(repo.id != current_repo_id for repo in repos):
            current_repo_id = None

        from kagan.ui.screens.repo_picker import RepoPickerScreen

        selected_repo_id = await self.push_screen_wait(
            RepoPickerScreen(
                project,
                repos,
                current_repo_id=current_repo_id,
            )
        )
        if not selected_repo_id:
            return None
        return next((repo for repo in repos if repo.id == selected_repo_id), None)

    def _match_repo_for_path(self, repos: list[Repo], path: Path) -> Repo | None:
        """Return the repo whose path contains the given path."""
        resolved_path = path.resolve()
        for repo in repos:
            repo_path = Path(repo.path).resolve()
            if resolved_path == repo_path or resolved_path.is_relative_to(repo_path):
                return repo
        return None

    async def _try_switch_lock(self, new_repo_path: Path) -> LockInfo | None:
        """Attempt to switch lock to a new repo path.

        Returns None on success, or LockInfo of the holder if the repo is locked.
        This implements Option D: lock follows the active repo.
        """
        new_lock = InstanceLock(new_repo_path)

        # Try to acquire the new lock first (before releasing old)
        if not new_lock.acquire():
            # New repo is locked by another instance
            return new_lock.get_holder_info()

        # Success - release old lock and switch
        if self._instance_lock:
            self._instance_lock.release()

        self._instance_lock = new_lock
        return None

    async def _apply_active_repo(self, repo: Repo) -> bool:
        """Apply repo selection to app context and services.

        Returns True if successful, False if the repo is locked by another instance.
        """
        new_path = Path(repo.path)

        # Check if we're switching to a different repo (not just re-applying same one)
        if self._instance_lock and new_path.resolve() != self.project_root.resolve():
            lock_holder = await self._try_switch_lock(new_path)
            if lock_holder is not None:
                # Repo is locked by another instance - show modal but don't quit
                from kagan.ui.modals.instance_locked import InstanceLockedModal

                await self.push_screen(InstanceLockedModal(lock_holder, is_startup=False))
                return False

        self.project_root = new_path
        self.ctx.active_repo_id = repo.id

        from kagan.services.sessions import SessionService

        self.ctx.session_service = SessionService(
            self.project_root,
            self.ctx.task_service,
            self.ctx.workspace_service,
            self.config,
        )
        return True

    def _clear_active_repo(self) -> None:
        """Clear active repo selection when a project has no repos."""
        self.ctx.active_repo_id = None

    async def _push_main_screen(self) -> None:
        """Push the main screen (Planner if empty, Kanban otherwise)."""
        ctx = self.ctx
        tasks = await ctx.task_service.list_tasks(project_id=ctx.active_project_id)

        if not tasks:
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
        tasks = await ctx.task_service.list_tasks(project_id=ctx.active_project_id)
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

            tasks = await state.list_tasks(project_id=self.ctx.active_project_id)
            valid_task_ids = {t.id for t in tasks}

            for session_name in kagan_sessions:
                task_id = session_name.replace("kagan-", "")
                if task_id not in valid_task_ids:
                    await run_tmux("kill-session", "-t", session_name)
                    self.log(f"Killed orphaned session: {session_name}")
                else:
                    continue
        except TmuxError:
            pass

    async def cleanup(self) -> None:
        """Terminate all agents and close resources."""
        if self.planner_state and self.planner_state.agent:
            await self.planner_state.agent.stop()
        if self.planner_state and self.planner_state.refiner:
            await self.planner_state.refiner.stop()

        if self._ctx:
            await self._ctx.close()
            self._ctx = None

        # Release instance lock
        if self._instance_lock:
            self._instance_lock.release()
            self._instance_lock = None

    async def action_open_project_selector(self) -> None:
        """Return to the project selector screen."""
        from kagan.ui.screens.welcome import WelcomeScreen

        if isinstance(self.screen, WelcomeScreen):
            return

        await self.switch_screen(WelcomeScreen(highlight_recent=True))

    def action_open_repo_selector(self) -> None:
        """Open the repository selector for the active project."""
        self.run_worker(self._open_repo_selector(), exclusive=True, exit_on_error=False)

    async def _open_repo_selector(self) -> None:
        """Worker entry for opening the repository selector."""
        try:
            project_service = self.ctx.project_service
        except AssertionError:
            self.notify("App not initialized yet", severity="warning")
            return

        project = None
        if self.ctx.active_project_id is not None:
            project = await project_service.get_project(self.ctx.active_project_id)
        if project is None:
            project = await project_service.find_project_by_repo_path(str(self.project_root))
        if project is None:
            self.notify("No active project to select a repository", severity="warning")
            return

        repos = await project_service.get_project_repos(project.id)

        current_repo_id = self.ctx.active_repo_id
        from kagan.ui.screens.repo_picker import RepoPickerScreen

        selected_repo_id = await self.push_screen_wait(
            RepoPickerScreen(
                project,
                repos,
                current_repo_id=current_repo_id,
            )
        )
        if not selected_repo_id or selected_repo_id == current_repo_id:
            return

        selected_repo = next((repo for repo in repos if repo.id == selected_repo_id), None)
        if selected_repo is None:
            return

        if not await self._apply_active_repo(selected_repo):
            # Repo is locked by another instance - modal was shown
            return

        from kagan.ui.screens.kanban import KanbanScreen
        from kagan.ui.screens.planner import PlannerScreen

        if isinstance(self.screen, (PlannerScreen, KanbanScreen)):
            await self.screen.reset_for_repo_change()
        await self._sync_active_screen_header()
        display_name = selected_repo.display_name or selected_repo.name
        self.notify(f"Active repository: {display_name}", severity="information")

    async def _sync_active_screen_header(self) -> None:
        """Sync header context and git branch for the active screen."""
        from textual.css.query import NoMatches

        from kagan.ui.screens.base import KaganScreen
        from kagan.ui.widgets.header import KaganHeader, _get_git_branch

        screen = self.screen
        if not isinstance(screen, KaganScreen):
            return
        try:
            header = screen.query_one(KaganHeader)
        except NoMatches:
            return

        await screen.sync_header_context(header)
        if self.ctx.active_repo_id is None:
            header.update_branch("")
            return
        branch = await _get_git_branch(self.project_root)
        header.update_branch(branch)

    def _run_action(self, action: str, *, target: str = "screen") -> None:
        if target == "app":
            action_method = getattr(self, f"action_{action}", None)
            if action_method is None:
                return
            result = action_method()
            if asyncio.iscoroutine(result):
                self.run_worker(result)
            return

        screen = self.screen
        if hasattr(screen, "run_action"):
            result = screen.run_action(action)
            if asyncio.iscoroutine(result):
                self.run_worker(result)

    def _screen_allows_action(self, screen: Screen, action: str) -> bool:
        if not hasattr(screen, f"action_{action}"):
            return False
        check_action = getattr(screen, "check_action", None)
        if check_action is None:
            return True
        try:
            return check_action(action, ()) is True
        except Exception:
            return False

    def get_system_commands(self, screen: Screen) -> Iterable[SystemCommand]:
        yield from super().get_system_commands(screen)
        yield SystemCommand("Help", "Open help", self.action_show_help)
        yield SystemCommand(
            "Projects",
            "Open project selector",
            lambda: self._run_action("open_project_selector", target="app"),
        )
        yield SystemCommand(
            "Repositories",
            "Open repo selector",
            lambda: self._run_action("open_repo_selector", target="app"),
        )
        yield SystemCommand(
            "Debug Log",
            "Open debug log viewer",
            lambda: self._run_action("toggle_debug_log", target="app"),
        )
        yield SystemCommand("Quit", "Exit Kagan", self.exit)

        def _make_screen_callback(action_name: str):
            def _callback() -> None:
                self._run_action(action_name, target="screen")

            return _callback

        from kagan.ui.screens.kanban import KanbanScreen
        from kagan.ui.screens.planner import PlannerScreen

        if isinstance(screen, KanbanScreen):
            kanban_actions = [
                ("Task: New", "Create a new task", "new_task"),
                ("Task: New AUTO", "Create a new AUTO task", "new_auto_task"),
                ("Task: Open", "Open session or start task", "open_session"),
                ("Task: Edit", "Edit selected task", "edit_task"),
                ("Task: View Details", "View task details", "view_details"),
                ("Task: Delete", "Delete selected task", "delete_task_direct"),
                ("Task: Duplicate", "Duplicate selected task", "duplicate_task"),
                ("Task: Peek", "Toggle peek overlay", "toggle_peek"),
                ("Task: Move Left", "Move task to previous column", "move_backward"),
                ("Task: Move Right", "Move task to next column", "move_forward"),
                ("Task: Start Agent", "Start AUTO agent", "start_agent"),
                ("Task: Stop Agent", "Stop AUTO agent", "stop_agent"),
                ("Task: View Diff", "View diff (REVIEW tasks)", "view_diff"),
                ("Task: Review", "Open review modal", "open_review"),
                ("Task: Merge", "Merge task", "merge_direct"),
                ("Board: Search", "Toggle search bar", "toggle_search"),
                ("Board: Plan Mode", "Open planner", "open_planner"),
                ("Board: Settings", "Open settings", "open_settings"),
            ]
            for title, help_text, action in kanban_actions:
                if self._screen_allows_action(screen, action):
                    yield SystemCommand(title, help_text, _make_screen_callback(action))

        if isinstance(screen, PlannerScreen):
            planner_actions = [
                ("Planner: Enhance", "Refine prompt", "refine"),
                ("Planner: Back to Board", "Return to board", "to_board"),
                ("Planner: Stop", "Stop planner", "cancel"),
            ]
            for title, help_text, action in planner_actions:
                if self._screen_allows_action(screen, action):
                    yield SystemCommand(title, help_text, _make_screen_callback(action))

    def action_show_help(self) -> None:
        """Open the help modal."""
        from kagan.ui.modals import HelpModal

        self.push_screen(HelpModal())

    def action_toggle_debug_log(self) -> None:
        """Toggle the debug log viewer (F12). Disabled in production builds."""
        from kagan.limits import DEBUG_BUILD

        if not DEBUG_BUILD:
            self.notify("Debug log disabled in production builds", severity="warning")
            return

        from kagan.ui.modals.debug_log import DebugLogModal

        self.push_screen(DebugLogModal())


def run() -> None:
    """Run the Kagan application."""
    app = KaganApp()
    app.run()


if __name__ == "__main__":
    run()
