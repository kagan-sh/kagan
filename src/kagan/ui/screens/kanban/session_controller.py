from __future__ import annotations

import asyncio
import platform
from pathlib import Path
from typing import TYPE_CHECKING

from kagan.core.models.enums import CardIndicator, TaskStatus, TaskType
from kagan.git_utils import has_git_repo
from kagan.services.workspaces import RepoWorkspaceInput

if TYPE_CHECKING:
    from kagan.config import AgentConfig
    from kagan.core.models.entities import Task
    from kagan.mcp.global_config import GlobalMcpSpec
    from kagan.ui.screens.kanban.screen import KanbanScreen

VALID_PAIR_LAUNCHERS = {"tmux", "vscode", "cursor"}


class KanbanSessionController:
    def __init__(self, screen: KanbanScreen) -> None:
        self.screen = screen

    async def provision_workspace_for_active_repo(self, task: Task) -> Path | None:
        active_repo_id = self.screen.ctx.active_repo_id
        if active_repo_id is None:
            self.screen.notify("Select a repository to start a session", severity="warning")
            return None

        repo_details = await self.screen.ctx.project_service.get_project_repo_details(
            task.project_id
        )
        repo = next((item for item in repo_details if item["id"] == active_repo_id), None)
        if repo is None:
            self.screen.notify("Active repository not part of this project", severity="error")
            return None

        repo_path = Path(repo["path"])
        if not await has_git_repo(repo_path):
            self.screen.notify(
                f"Not a git repository: {repo_path}. Run git init first.",
                severity="error",
            )
            return None

        self.screen.notify("Creating workspace...", severity="information")
        try:
            await self.screen.ctx.workspace_service.provision(
                task_id=task.id,
                repos=[
                    RepoWorkspaceInput(
                        repo_id=repo["id"],
                        repo_path=repo["path"],
                        target_branch=repo["default_branch"],
                    )
                ],
            )
        except Exception as exc:
            self.screen.notify(f"Failed to create workspace: {exc}", severity="error")
            return None

        wt_path = await self.screen.ctx.workspace_service.get_path(task.id)
        if wt_path is None:
            self.screen.notify("Failed to provision workspace", severity="error")
            return None
        return wt_path

    async def ensure_mcp_installed(self, agent_config: AgentConfig, spec: GlobalMcpSpec) -> bool:
        from kagan.ui.modals.mcp_install import McpInstallModal

        future: asyncio.Future[bool] = asyncio.get_event_loop().create_future()

        def on_result(result: bool | None) -> None:
            if not future.done():
                future.set_result(result is True)

        self.screen.app.push_screen(
            McpInstallModal(agent_config=agent_config, spec=spec),
            callback=on_result,
        )
        return await future

    async def handle_missing_agent(self, task: Task, agent_config: AgentConfig) -> str:
        from kagan.builtin_agents import get_builtin_agent, list_available_agents
        from kagan.ui.modals.agent_choice import AgentChoiceModal, AgentChoiceResult

        builtin = get_builtin_agent(agent_config.short_name)
        available = list_available_agents()

        future: asyncio.Future[str] = asyncio.get_event_loop().create_future()

        def on_result(result: str | None) -> None:
            if not future.done():
                future.set_result(result or AgentChoiceResult.CANCELLED)

        self.screen.app.push_screen(
            AgentChoiceModal(
                missing_agent=builtin,
                available_agents=available,
                task_title=task.title,
            ),
            callback=on_result,
        )

        return await future

    async def update_task_agent(self, task: Task, agent_short_name: str) -> None:
        current_agent = task.get_agent_config(self.screen.kagan_app.config).short_name
        self.screen.notify(
            f"Using {agent_short_name} instead of {current_agent}",
            title="Agent Changed",
            timeout=3,
        )

    async def ask_confirmation(self, title: str, message: str) -> bool:
        from kagan.ui.modals import ConfirmModal

        future: asyncio.Future[bool] = asyncio.get_event_loop().create_future()

        def on_result(result: bool | None) -> None:
            if not future.done():
                future.set_result(result is True)

        self.screen.app.push_screen(ConfirmModal(title=title, message=message), callback=on_result)
        return await future

    async def confirm_start_auto_task(self, task: Task) -> bool:
        from kagan.constants import NOTIFICATION_TITLE_MAX_LENGTH

        title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
        return await self.ask_confirmation(
            "Start Agent?",
            f"Start agent for '{title}' and open output stream?",
        )

    async def confirm_attach_pair_session(self, task: Task) -> bool:
        from kagan.constants import NOTIFICATION_TITLE_MAX_LENGTH

        title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
        return await self.ask_confirmation(
            "Attach Session?",
            f"Attach to session for '{title}'?",
        )

    async def open_auto_output_for_task(
        self,
        task: Task,
        *,
        wait_for_running: bool = False,
    ) -> None:
        scheduler = self.screen.ctx.automation_service

        if wait_for_running:
            for _ in range(20):
                if scheduler.is_running(task.id):
                    break
                await asyncio.sleep(0.1)

        if not scheduler.is_running(task.id):
            latest = await self.screen.ctx.execution_service.get_latest_execution_for_task(task.id)
            if latest is None:
                self.screen.notify("No agent logs available for this task", severity="warning")
                return

        await self.screen._open_review_for_task(
            task,
            read_only=True,
            initial_tab="review-agent-output",
            include_running_output=True,
        )

    async def open_session_flow(self, task: Task) -> None:
        refreshed = await self.screen.ctx.task_service.get_task(task.id)
        if refreshed is not None:
            task = refreshed

        if task.status in (TaskStatus.REVIEW, TaskStatus.DONE):
            await self.screen._open_review_for_task(task, read_only=task.status == TaskStatus.DONE)
            return

        if task.task_type == TaskType.AUTO:
            if task.status == TaskStatus.BACKLOG:
                if not await self.confirm_start_auto_task(task):
                    return
                await self.start_agent_flow(task)
                refreshed_after_start = await self.screen.ctx.task_service.get_task(task.id)
                await self.open_auto_output_for_task(
                    refreshed_after_start or task,
                    wait_for_running=True,
                )
            elif task.status == TaskStatus.IN_PROGRESS:
                await self.open_auto_output_for_task(task)
            return

        agent_config = task.get_agent_config(self.screen.kagan_app.config)
        from kagan.agents.installer import check_agent_installed

        if not check_agent_installed(agent_config.short_name):
            from kagan.ui.modals.agent_choice import AgentChoiceResult

            result = await self.handle_missing_agent(task, agent_config)
            if result == AgentChoiceResult.CANCELLED:
                return
            if result == AgentChoiceResult.INSTALLED:
                pass
            elif fallback_agent := AgentChoiceResult.parse_fallback(result):
                await self.update_task_agent(task, fallback_agent)
                refreshed = await self.screen.ctx.task_service.get_task(task.id)
                if refreshed:
                    task = refreshed
                    agent_config = task.get_agent_config(self.screen.kagan_app.config)

        from kagan.mcp.global_config import get_global_mcp_spec, is_global_mcp_configured

        if not is_global_mcp_configured(agent_config.short_name):
            spec = get_global_mcp_spec(agent_config.short_name)
            if spec:
                installed = await self.ensure_mcp_installed(agent_config, spec)
                if not installed:
                    return

        wt_path = await self.screen.ctx.workspace_service.get_path(task.id)
        if wt_path is None:
            wt_path = await self.provision_workspace_for_active_repo(task)
            if wt_path is None:
                return

        if not await self.screen._ensure_pair_terminal_backend_ready(task):
            return

        terminal_backend = self.screen._resolve_pair_terminal_backend(task)

        if not await self.screen.ctx.session_service.session_exists(task.id):
            self.screen.notify("Creating session...", severity="information")
            await self.screen.ctx.session_service.create_session(task, wt_path)

        if task.status == TaskStatus.IN_PROGRESS and not await self.confirm_attach_pair_session(
            task
        ):
            return

        if not self.screen.kagan_app.config.ui.skip_pair_instructions:
            from kagan.ui.modals.tmux_gateway import PairInstructionsModal

            def on_gateway_result(result: str | None) -> None:
                if result is None:
                    return
                if result == "skip_future":
                    self.screen.kagan_app.config.ui.skip_pair_instructions = True
                    cb_result = self.save_pair_instructions_preference(skip=True)
                    if asyncio.iscoroutine(cb_result):
                        asyncio.create_task(cb_result)

                self.screen.app.call_later(
                    self.screen._do_open_pair_session, task, wt_path, terminal_backend
                )

            self.screen.app.push_screen(
                PairInstructionsModal(
                    task.id,
                    task.title,
                    terminal_backend,
                    self.screen._startup_prompt_path_hint(wt_path),
                ),
                on_gateway_result,
            )
            return

        await self.screen._do_open_pair_session(task, wt_path, terminal_backend)

    def resolve_pair_terminal_backend(self, task: Task) -> str:
        task_backend = getattr(task, "terminal_backend", None)
        if isinstance(task_backend, str):
            normalized = task_backend.strip().lower()
            if normalized in VALID_PAIR_LAUNCHERS:
                return normalized

        configured = getattr(
            self.screen.kagan_app.config.general,
            "default_pair_terminal_backend",
            "tmux",
        )
        if isinstance(configured, str):
            normalized = configured.strip().lower()
            if normalized in VALID_PAIR_LAUNCHERS:
                return normalized

        return "tmux"

    async def ensure_pair_terminal_backend_ready(self, task: Task) -> bool:
        from kagan.terminals.installer import check_terminal_installed, first_available_pair_backend
        from kagan.ui.modals.terminal_install import TerminalInstallModal

        backend = self.screen._resolve_pair_terminal_backend(task)
        is_windows = platform.system() == "Windows"

        if backend in {"vscode", "cursor"}:
            if check_terminal_installed(backend):
                return True
            self.screen.notify(
                f"{backend} is not installed. Install it and retry.",
                severity="warning",
            )
            return False

        if backend == "tmux":
            if check_terminal_installed("tmux"):
                return True
            if is_windows:
                fallback = first_available_pair_backend(windows=True)
                if fallback is not None:
                    await self.screen.ctx.task_service.update_fields(
                        task.id, terminal_backend=fallback
                    )
                    self.screen.notify(
                        f"tmux not found on Windows. Using {fallback} for this task.",
                        severity="information",
                    )
                    return True
                self.screen.notify(
                    "PAIR cancelled: install VS Code or Cursor to continue on Windows.",
                    severity="warning",
                )
                return False

            installed_tmux = await self.screen.app.push_screen(TerminalInstallModal("tmux"))
            if installed_tmux and check_terminal_installed("tmux"):
                return True

            fallback = first_available_pair_backend(windows=False)
            if fallback is not None:
                await self.screen.ctx.task_service.update_fields(task.id, terminal_backend=fallback)
                self.screen.notify(
                    "tmux not installed. Using fallback launcher "
                    f"{fallback}. VS Code: https://code.visualstudio.com/download "
                    "Cursor: https://cursor.com/downloads",
                    severity="information",
                )
                return True
            self.screen.notify(
                "PAIR cancelled: install tmux (recommended), or install VS Code/Cursor "
                "for external development.",
                severity="warning",
            )
            return False

        self.screen.notify(f"Unsupported PAIR launcher: {backend}", severity="warning")
        return False

    @staticmethod
    def startup_prompt_path_hint(workspace_path: Path) -> Path:
        return workspace_path / ".kagan" / "start_prompt.md"

    async def do_open_pair_session(
        self,
        task: Task,
        workspace_path: Path | None = None,
        terminal_backend: str | None = None,
    ) -> None:
        try:
            if task.status == TaskStatus.BACKLOG:
                await self.screen.ctx.task_service.update_fields(
                    task.id, status=TaskStatus.IN_PROGRESS
                )
                await self.screen._refresh_board()

            with self.screen.app.suspend():
                attached = await self.screen.ctx.session_service.attach_session(task.id)

            backend = terminal_backend or self.screen._resolve_pair_terminal_backend(task)
            if not attached:
                self.screen.notify("Failed to open PAIR session", severity="warning")
                return

            if backend != "tmux":
                prompt_path = (
                    self.screen._startup_prompt_path_hint(workspace_path)
                    if workspace_path is not None
                    else Path(".kagan/start_prompt.md")
                )
                self.screen.notify(
                    f"Workspace opened externally. Use startup prompt: {prompt_path}",
                    severity="information",
                )
                return

            session_still_exists = await self.screen.ctx.session_service.session_exists(task.id)
            if session_still_exists:
                return

            from kagan.ui.modals.confirm import ConfirmModal

            def on_confirm(result: bool | None) -> None:
                if result:

                    async def move_to_review() -> None:
                        await self.screen.ctx.task_service.update_fields(
                            task.id, status=TaskStatus.REVIEW
                        )
                        await self.screen._refresh_board()

                    self.screen.app.call_later(move_to_review)

            self.screen.app.push_screen(
                ConfirmModal("Session Complete", "Move task to REVIEW?"),
                on_confirm,
            )

        except Exception as e:
            from kagan.tmux import TmuxError

            if isinstance(e, TmuxError):
                self.screen.notify(f"Tmux error: {e}", severity="error")

    async def start_agent_flow(self, task: Task) -> None:
        if task.task_type == TaskType.PAIR:
            return

        if self.screen.ctx.automation_service.is_running(task.id):
            self.screen.notify("Agent already running for this task (press Enter to open output)")
            return

        wt_path = await self.screen.ctx.workspace_service.get_path(task.id)
        if wt_path is None:
            wt_path = await self.provision_workspace_for_active_repo(task)
            if wt_path is None:
                return

        if task.status == TaskStatus.BACKLOG:
            await self.screen.ctx.task_service.move(task.id, TaskStatus.IN_PROGRESS)
            refreshed = await self.screen.ctx.task_service.get_task(task.id)
            if refreshed:
                task = refreshed
            await self.screen._refresh_board()

        self.screen.notify("Starting agent...", severity="information")

        result = self.screen.ctx.automation_service.spawn_for_task(task)

        if hasattr(result, "__await__"):
            spawned = await result
        else:
            spawned = result

        if spawned:
            self.screen._set_card_indicator(task.id, CardIndicator.RUNNING, is_active=True)
            self.screen.notify(f"Agent started: {task.id[:8]}", severity="information")
        else:
            self.screen.notify("Failed to start agent (at capacity?)", severity="warning")

    async def apply_global_agent_selection(self, selected: str) -> None:
        from kagan.builtin_agents import get_builtin_agent

        config = self.screen.kagan_app.config
        current_agent = config.general.default_worker_agent
        if not selected or selected == current_agent:
            return

        config.general.default_worker_agent = selected
        if config.get_agent(selected) is None:
            if builtin := get_builtin_agent(selected):
                config.agents[selected] = builtin.config.model_copy(deep=True)

        await config.save(self.screen.kagan_app.config_path)
        self.screen.header.update_agent_from_config(config)
        self.screen.notify(f"Global agent set to: {selected}", severity="information")

    async def save_pair_instructions_preference(self, skip: bool = True) -> None:
        try:
            await self.screen.kagan_app.config.update_ui_preferences(
                self.screen.kagan_app.config_path,
                skip_pair_instructions=skip,
            )
        except Exception as e:
            self.screen.notify(f"Failed to save preference: {e}", severity="error")
