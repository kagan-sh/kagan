"""Main Kagan TUI application."""

from __future__ import annotations

import asyncio
import contextlib
import os
from pathlib import Path
from typing import TYPE_CHECKING, Literal

from textual.app import App, SystemCommand
from textual.signal import Signal

from kagan.core.agents.agent_factory import AgentFactory, create_agent
from kagan.core.bootstrap import (
    AppContext,
    create_app_context,
    create_signal_bridge,
    wire_default_signals,
)
from kagan.core.config import KaganConfig
from kagan.core.constants import (
    DEFAULT_CONFIG_PATH,
    DEFAULT_DB_PATH,
)
from kagan.core.debug_log import setup_debug_logging
from kagan.core.instance_lock import InstanceLock, LockInfo
from kagan.core.services.runtime import RuntimeContextState, RuntimeSessionEvent
from kagan.core.terminal import supports_truecolor
from kagan.tui.keybindings import APP_BINDINGS
from kagan.tui.theme import KAGAN_THEME, KAGAN_THEME_256
from kagan.tui.ui.screens.kanban import KanbanScreen
from kagan.tui.ui.screens.onboarding import OnboardingScreen

if TYPE_CHECKING:
    from collections.abc import Iterable

    from textual.screen import Screen
    from textual.worker import Worker

    from kagan.core.adapters.db.schema import Project, Repo
    from kagan.core.ipc.client import IPCClient
    from kagan.tui.ui.screens.planner.state import PersistentPlannerState


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
    ) -> None:
        """Initialize the TUI app and default runtime state."""
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
        self._core_client: IPCClient | None = None
        self.config: KaganConfig = KaganConfig()
        self.planner_state: PersistentPlannerState | None = None
        self._agent_factory = agent_factory
        self._instance_lock: InstanceLock | None = None
        self._startup_worker: Worker[None] | None = None
        self._core_status: str = "DISCONNECTED"

    @property
    def ctx(self) -> AppContext:
        """Get the application context for service access."""
        assert self._ctx is not None, "AppContext not initialized"
        return self._ctx

    async def on_mount(self) -> None:
        """Initialize app on mount."""
        setup_debug_logging()

        self._instance_lock = InstanceLock(self.project_root)
        if not self._instance_lock.acquire():
            lock_info = self._instance_lock.get_holder_info()
            if lock_info is not None and lock_info.pid == os.getpid():
                self.log("Reusing same-process instance lock for startup")
            else:
                from kagan.tui.ui.modals.instance_locked import InstanceLockedModal

                await self.push_screen(InstanceLockedModal(lock_info))
                return

        if not self._config_exists():
            await self.push_screen(OnboardingScreen())
            return

        self._run_startup_worker()

    def _config_exists(self) -> bool:
        """Check if the config file exists (determines first boot vs normal boot)."""
        return self.config_path.exists()

    def _run_startup_worker(self) -> None:
        """Start app initialization in an exclusive startup worker."""
        if self._startup_worker is not None and not self._startup_worker.is_finished:
            return
        self._startup_worker = self.run_worker(
            self._initialize_app(),
            group="startup",
            exclusive=True,
            exit_on_error=False,
        )

    async def _initialize_app(self) -> None:
        """Initialize all app components."""
        self.config = KaganConfig.load(self.config_path)
        self.log("Config loaded", path=str(self.config_path))

        from kagan.core.ipc.discovery import discover_core_endpoint

        endpoint = discover_core_endpoint()
        if endpoint is not None:
            from kagan.core.ipc.client import IPCClient

            try:
                client = IPCClient(endpoint)
                await client.connect()
                self._core_client = client
                self._core_status = "CONNECTED"
                self.log(
                    "Attached to running core",
                    transport=endpoint.transport,
                    address=endpoint.address,
                )
            except Exception as exc:
                self.log(
                    "Could not attach to core, using direct context",
                    error=str(exc),
                )

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

            await ctx.api.start_automation()
            self.log("Automation service initialized (reactive mode)")

        await self._startup_screen_decision()

    def _continue_after_welcome(self) -> None:
        """Called when welcome screen completes to continue app initialization."""
        self.call_later(self._run_init_after_welcome)

    def _run_init_after_welcome(self) -> None:
        """Run initialization after welcome screen."""
        self._run_startup_worker()

    def on_onboarding_screen_completed(self, message: OnboardingScreen.Completed) -> None:
        """Handle OnboardingScreen.Completed message."""
        self.config = message.config
        self.call_later(self._continue_after_onboarding)

    def _continue_after_onboarding(self) -> None:
        """Called after OnboardingScreen completes to continue startup flow.

        Pops the onboarding screen, reinitializes context (now that config exists),
        and continues to the normal startup screen decision flow.
        """

        self.pop_screen()
        self._run_startup_worker()

    async def _startup_screen_decision(self) -> None:
        """Decide the startup flow from runtime session controller state."""
        ctx = self.ctx
        cwd = self.project_root
        decision = await ctx.api.decide_startup(cwd)

        if decision.should_open_project and decision.project_id is not None:
            opened = await self.open_project_session(
                decision.project_id,
                preferred_repo_id=decision.preferred_repo_id,
                preferred_path=decision.preferred_path,
                allow_picker=False,
                screen_mode="push",
            )
            if opened:
                return

        from kagan.tui.ui.screens.welcome import WelcomeScreen

        await self.push_screen(
            WelcomeScreen(
                suggest_cwd=decision.suggest_cwd,
                cwd_path=decision.cwd_path,
                cwd_is_git_repo=decision.cwd_is_git_repo,
            )
        )
        self.log(
            "WelcomeScreen pushed",
            suggest_cwd=decision.suggest_cwd,
            cwd_path=decision.cwd_path,
            cwd_is_git_repo=decision.cwd_is_git_repo,
        )

    def _apply_runtime_session_state(self, state: RuntimeContextState) -> None:
        """Apply runtime session state to mutable app context."""
        self.ctx.active_project_id = state.project_id
        self.ctx.active_repo_id = state.repo_id

    async def _dispatch_runtime_session(
        self,
        event: RuntimeSessionEvent,
        *,
        project_id: str | None = None,
        repo_id: str | None = None,
    ) -> RuntimeContextState:
        """Dispatch a session event and sync AppContext from controller state."""
        try:
            state = await self.ctx.api.dispatch_runtime_session(
                event,
                project_id=project_id,
                repo_id=repo_id,
            )
        except Exception as exc:
            self.log("Runtime session persist failed", error=str(exc))
            state = self.ctx.api.runtime_state
        self._apply_runtime_session_state(state)
        return state

    async def open_project_session(
        self,
        project_id: str,
        *,
        preferred_repo_id: str | None = None,
        preferred_path: Path | None = None,
        allow_picker: bool = True,
        screen_mode: Literal["push", "switch"] = "switch",
    ) -> bool:
        """Open a project, resolve active repo, and navigate to the main screen."""
        project = await self.ctx.api.open_project(project_id)
        await self._dispatch_runtime_session(
            RuntimeSessionEvent.PROJECT_SELECTED,
            project_id=project.id,
        )
        if not await self._set_active_repo_for_project(
            project,
            preferred_repo_id=preferred_repo_id,
            preferred_path=preferred_path,
            allow_picker=allow_picker,
        ):
            return False
        await self._show_main_screen(mode=screen_mode)
        return True

    async def _set_active_repo_for_project(
        self,
        project: Project,
        *,
        preferred_repo_id: str | None = None,
        preferred_path: Path | None = None,
        allow_picker: bool = True,
    ) -> bool:
        """Resolve and apply the active repo for a project.

        Returns False if repo selection was cancelled or the repo is locked.
        """
        repo = await self._select_repo_for_project(
            project,
            preferred_repo_id=preferred_repo_id,
            preferred_path=preferred_path,
            allow_picker=allow_picker,
        )
        if repo is None:
            self._clear_active_repo()
            await self._dispatch_runtime_session(RuntimeSessionEvent.REPO_CLEARED)
            return False
        return await self._apply_active_repo(repo, project_id=project.id)

    async def _select_repo_for_project(
        self,
        project: Project,
        *,
        preferred_repo_id: str | None = None,
        preferred_path: Path | None = None,
        allow_picker: bool = True,
    ) -> Repo | None:
        """Return the repo to use for a project, optionally using a picker."""
        repos = await self.ctx.api.get_project_repos(project.id)
        if not repos:
            return None

        if preferred_repo_id is not None:
            preferred_repo = next((repo for repo in repos if repo.id == preferred_repo_id), None)
            if preferred_repo is not None:
                return preferred_repo

        if preferred_path is not None:
            matched = self._match_repo_for_path(repos, preferred_path)
            if matched:
                return matched

        if len(repos) == 1 or not allow_picker:
            return repos[0]

        current_repo_id = self.ctx.active_repo_id
        if current_repo_id is not None and all(repo.id != current_repo_id for repo in repos):
            current_repo_id = None

        from kagan.tui.ui.screens.repo_picker import RepoPickerScreen

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

        if not new_lock.acquire():
            holder = new_lock.get_holder_info()
            # Treat same-process lock ownership as current-instance ownership.
            # This prevents false-positive lock modals when repository selection
            # re-enters a path that this running app already holds.
            if holder is not None and holder.pid == os.getpid():
                return None
            return holder

        if self._instance_lock:
            self._instance_lock.release()

        self._instance_lock = new_lock
        return None

    async def _apply_active_repo(self, repo: Repo, *, project_id: str | None = None) -> bool:
        """Apply repo selection to app context and services.

        Returns True if successful, False if the repo is locked by another instance.
        """
        new_path = Path(repo.path)

        if self._instance_lock and new_path.resolve() != self.project_root.resolve():
            lock_holder = await self._try_switch_lock(new_path)
            if lock_holder is not None:
                from kagan.tui.ui.modals.instance_locked import InstanceLockedModal

                await self.push_screen(InstanceLockedModal(lock_holder, is_startup=False))
                return False

        self.project_root = new_path
        await self._dispatch_runtime_session(
            RuntimeSessionEvent.REPO_SELECTED,
            project_id=project_id or self.ctx.active_project_id,
            repo_id=repo.id,
        )

        self.ctx.api.bootstrap_session_service(self.project_root, self.config)
        return True

    def _clear_active_repo(self) -> None:
        """Clear active repo selection when a project has no repos."""
        self.ctx.active_repo_id = None

    async def _show_main_screen(self, *, mode: Literal["push", "switch"] = "push") -> None:
        """Navigate to the main screen (Planner if empty, Kanban otherwise)."""
        ctx = self.ctx
        tasks = await ctx.api.list_tasks(project_id=ctx.active_project_id)

        if not tasks:
            from kagan.tui.ui.screens.planner import PlannerScreen

            screen = PlannerScreen(agent_factory=self._agent_factory)
            if mode == "switch":
                await self.switch_screen(screen)
            else:
                await self.push_screen(screen)
            self.log("PlannerScreen shown (empty board)", mode=mode)
        else:
            screen = KanbanScreen()
            if mode == "switch":
                await self.switch_screen(screen)
            else:
                await self.push_screen(screen)
            self.log("KanbanScreen shown, app ready", mode=mode)

    async def on_unmount(self) -> None:
        """Clean up on unmount."""
        await self.cleanup()

    async def _reconcile_worktrees(self) -> None:
        """Remove orphaned worktrees from previous runs."""
        ctx = self.ctx
        tasks = await ctx.api.list_tasks(project_id=ctx.active_project_id)
        valid_ids = {t.id for t in tasks}
        cleaned = await ctx.api.cleanup_orphan_workspaces(valid_ids)
        if cleaned:
            self.log(f"Cleaned up {len(cleaned)} orphan worktree(s)")

    async def _reconcile_sessions(self) -> None:
        """Kill orphaned tmux sessions from previous runs."""
        from kagan.core.tmux import TmuxError, run_tmux

        try:
            output = await run_tmux("list-sessions", "-F", "#{session_name}")
            kagan_sessions = [s for s in output.split("\n") if s.startswith("kagan-")]

            tasks = await self.ctx.api.list_tasks(project_id=self.ctx.active_project_id)
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
        """Terminate all agents and close resources.

        Shutdown order:
        1. Stop planner agents.
        2. Mark the DB repository as closing (prevents new sessions).
        3. Unbind signals (prevents new workers being scheduled).
        4. Cancel all in-flight Textual workers and wait for them to finish.
        5. Close the AppContext (stops automation, disposes DB engine).
        6. Release the instance lock.
        """
        if self.planner_state and self.planner_state.agent:
            await self.planner_state.agent.stop()
        if self.planner_state and self.planner_state.refiner:
            await self.planner_state.refiner.stop()

        if self._ctx:
            # Mark repo closing early so any still-running worker gets a clean
            # RepositoryClosing error instead of an opaque OperationalError.
            if self._ctx._task_repo is not None:
                self._ctx._task_repo.mark_closing()
            # Unbind signals so no new workers are scheduled.
            if self._ctx.signal_bridge:
                self._ctx.signal_bridge.unbind_all()

        # Cancel in-flight workers and wait for them to settle before
        # disposing the DB engine.  During shutdown, workers that were
        # already mid-flight may fail with RepositoryClosing, ValueError
        # ("Connection closed"), or get cancelled outright — all expected.
        self.workers.cancel_all()
        with contextlib.suppress(Exception, asyncio.CancelledError):
            await self.workers.wait_for_complete()

        if self._ctx:
            await self._ctx.close()
            self._ctx = None

        if self._core_client is not None:
            with contextlib.suppress(Exception):
                await self._core_client.close()
            self._core_client = None
            self._core_status = "DISCONNECTED"

        if self._instance_lock:
            self._instance_lock.release()
            self._instance_lock = None

    async def action_open_project_selector(self) -> None:
        """Return to the project selector screen."""
        from kagan.tui.ui.screens.welcome import WelcomeScreen

        if isinstance(self.screen, WelcomeScreen):
            return

        await self.switch_screen(WelcomeScreen(highlight_recent=True))

    def action_open_repo_selector(self) -> None:
        """Open the repository selector for the active project."""
        self.run_worker(
            self._open_repo_selector(),
            group="app-open-repo-selector",
            exclusive=True,
            exit_on_error=False,
        )

    async def _open_repo_selector(self) -> None:
        """Worker entry for opening the repository selector."""
        invoking_screen = self.screen
        try:
            api = self.ctx.api
        except AssertionError:
            self.notify("App not initialized yet", severity="warning")
            return

        project = None
        if self.ctx.active_project_id is not None:
            project = await api.get_project(self.ctx.active_project_id)
        if project is None:
            project = await api.find_project_by_repo_path(str(self.project_root))
        if project is None:
            self.notify("No active project to select a repository", severity="warning")
            return

        repos = await api.get_project_repos(project.id)

        current_repo_id = self.ctx.active_repo_id
        from kagan.tui.ui.screens.repo_picker import RepoPickerScreen

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

        if not await self._apply_active_repo(selected_repo, project_id=project.id):
            return

        from kagan.tui.ui.screens.kanban import KanbanScreen
        from kagan.tui.ui.screens.planner import PlannerScreen

        reset_target = None
        if isinstance(invoking_screen, (PlannerScreen, KanbanScreen)):
            reset_target = invoking_screen
        elif isinstance(self.screen, (PlannerScreen, KanbanScreen)):
            reset_target = self.screen
        if reset_target is not None:
            await reset_target.reset_for_repo_change()
        await self._sync_active_screen_header()
        display_name = selected_repo.display_name or selected_repo.name
        self.notify(f"Active repository: {display_name}", severity="information")

    async def _sync_active_screen_header(self) -> None:
        """Sync header context and git branch for the active screen."""
        from textual.css.query import NoMatches

        from kagan.tui.ui.screens.base import KaganScreen
        from kagan.tui.ui.widgets.header import KaganHeader, _get_git_branch

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

    def get_system_commands(self, screen: Screen) -> Iterable[SystemCommand]:
        """Expose command palette actions for app-level navigation and utilities."""
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

    def action_show_help(self) -> None:
        """Open the help modal."""
        from kagan.tui.ui.modals import HelpModal

        self.push_screen(HelpModal())

    def action_toggle_debug_log(self) -> None:
        """Toggle the debug log viewer (F12). Disabled in production builds."""
        from kagan.core.limits import DEBUG_BUILD

        if not DEBUG_BUILD:
            self.notify("Debug log disabled in production builds", severity="warning")
            return

        from kagan.tui.ui.modals.debug_log import DebugLogModal

        self.push_screen(DebugLogModal())


def run() -> None:
    """Run the Kagan application."""
    app = KaganApp()
    app.run()


if __name__ == "__main__":
    run()
