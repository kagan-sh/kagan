from __future__ import annotations

import asyncio
import contextlib
import platform
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any

from kagan.core.git_utils import has_git_repo
from kagan.core.models.enums import CardIndicator, TaskStatus, TaskType
from kagan.core.services.jobs import JobRecord, JobStatus
from kagan.core.services.runtime import AutoOutputMode
from kagan.core.services.workspaces import RepoWorkspaceInput
from kagan.tui.ui.screen_result import await_screen_result

if TYPE_CHECKING:
    from kagan.core.acp import Agent
    from kagan.core.adapters.db.schema import Task
    from kagan.core.config import AgentConfig
    from kagan.core.services.runtime import AutoOutputReadiness
    from kagan.tui.ui.screens.kanban.screen import KanbanScreen


@dataclass(frozen=True, slots=True)
class WorkspaceProvisionResult:
    success: bool
    path: Path | None = None
    error: str | None = None


@dataclass(frozen=True, slots=True)
class PairBackendReadiness:
    success: bool
    backend: str | None = None
    error: str | None = None


class KanbanSessionController:
    START_JOB_PENDING_MESSAGE = "Agent start requested; waiting for scheduler."
    STOP_JOB_PENDING_MESSAGE = "Agent stop requested; waiting for scheduler."

    def __init__(self, screen: KanbanScreen) -> None:
        self.screen = screen

    async def wait_for_job_terminal(
        self,
        job_id: str,
        *,
        task_id: str,
        timeout_seconds: float = 0.6,
    ) -> JobRecord | None:
        """Wait briefly for a job to reach terminal state.

        If the UI worker is cancelled (for example, action interruption), cancel
        the in-flight job so we do not leave duplicate start/stop jobs running.
        """
        try:
            return await self.screen.ctx.api.wait_job(
                job_id,
                task_id=task_id,
                timeout_seconds=timeout_seconds,
            )
        except asyncio.CancelledError:
            with contextlib.suppress(Exception):  # quality-allow-broad-except
                await self.screen.ctx.api.cancel_job(job_id, task_id=task_id)
            raise

    @staticmethod
    def job_result_payload(record: JobRecord | None) -> dict[str, Any] | None:
        if record is None or not isinstance(record.result, dict):
            return None
        return record.result

    @classmethod
    def job_message(cls, record: JobRecord | None, default: str) -> str:
        payload = cls.job_result_payload(record)
        if payload is not None:
            payload_message = payload.get("message")
            if isinstance(payload_message, str) and payload_message.strip():
                return payload_message
        if record is not None and record.message:
            return record.message
        return default

    async def provision_workspace_for_active_repo(self, task: Task) -> WorkspaceProvisionResult:
        active_repo_id = self.screen.ctx.active_repo_id
        if active_repo_id is None:
            self.screen.notify("Select a repository to start a session", severity="warning")
            return WorkspaceProvisionResult(success=False, error="No active repository selected")

        repo_details = await self.screen.ctx.api.get_project_repo_details(task.project_id)
        repo = next((item for item in repo_details if item["id"] == active_repo_id), None)
        if repo is None:
            self.screen.notify("Active repository not part of this project", severity="error")
            return WorkspaceProvisionResult(success=False, error="Active repo not in project")

        repo_path = Path(repo["path"])
        if not await has_git_repo(repo_path):
            self.screen.notify(
                f"Not a git repository: {repo_path}. Run git init first.",
                severity="error",
            )
            return WorkspaceProvisionResult(success=False, error="Repository has no git metadata")

        self.screen.notify("Creating workspace...", severity="information")
        try:
            await self.screen.ctx.api.provision_workspace(
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
            return WorkspaceProvisionResult(success=False, error=str(exc))

        wt_path = await self.screen.ctx.api.get_workspace_path(task.id)
        if wt_path is None:
            self.screen.notify("Failed to provision workspace", severity="error")
            return WorkspaceProvisionResult(success=False, error="Workspace path missing")
        return WorkspaceProvisionResult(success=True, path=wt_path)

    async def handle_missing_agent(self, task: Task, agent_config: AgentConfig) -> str:
        from kagan.core.builtin_agents import get_builtin_agent, list_available_agents
        from kagan.tui.ui.modals.agent_choice import AgentChoiceModal, AgentChoiceResult

        builtin = get_builtin_agent(agent_config.short_name)
        available = list_available_agents()

        result = await await_screen_result(
            self.screen.app,
            AgentChoiceModal(
                missing_agent=builtin,
                available_agents=available,
                task_title=task.title,
            ),
        )
        return result or AgentChoiceResult.CANCELLED

    async def update_task_agent(self, task: Task, agent_short_name: str) -> None:
        current_agent = task.get_agent_config(self.screen.kagan_app.config).short_name
        self.screen.notify(
            f"Using {agent_short_name} instead of {current_agent}",
            title="Agent Changed",
            timeout=3,
        )

    async def ask_confirmation(self, title: str, message: str) -> bool:
        from kagan.tui.ui.modals import ConfirmModal

        result = await await_screen_result(
            self.screen.app, ConfirmModal(title=title, message=message)
        )
        return result is True

    async def confirm_start_auto_task(self, task: Task) -> bool:
        from kagan.core.constants import NOTIFICATION_TITLE_MAX_LENGTH

        title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
        return await self.ask_confirmation(
            "Start Agent?",
            f"Start agent for '{title}' and open output stream?",
        )

    async def confirm_attach_pair_session(self, task: Task) -> bool:
        from kagan.core.constants import NOTIFICATION_TITLE_MAX_LENGTH

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
        quiet_unavailable: bool = False,
    ) -> bool:
        """Open auto output for task."""
        api = self.screen.ctx.api
        attached_agent: Agent | None = None

        # Reconcile persisted runtime state so ENTER works for runs started outside TUI (e.g. MCP).
        await api.reconcile_running_tasks([task.id])

        if wait_for_running:
            attached_agent = await api.wait_for_running_agent(task.id, timeout=6.0)

        readiness: AutoOutputReadiness = await api.prepare_auto_output(task)
        if attached_agent is not None and readiness.running_agent is None:
            readiness.running_agent = attached_agent
            readiness.is_running = True
            readiness.can_open_output = True
            readiness.output_mode = AutoOutputMode.LIVE
        if readiness.output_mode is AutoOutputMode.WAITING and not readiness.is_running:
            recovery = await api.recover_stale_auto_output(task)
            if recovery.message:
                self.screen.notify(
                    recovery.message,
                    severity="information" if recovery.success else "warning",
                )
            if recovery.success:
                readiness = await api.prepare_auto_output(task)

        if readiness.message and not (quiet_unavailable and not readiness.can_open_output):
            severity = "warning" if not readiness.can_open_output else "information"
            self.screen.notify(readiness.message, severity=severity)
        if not readiness.can_open_output:
            return False

        await self.screen._review.open_task_output_for_task(
            task,
            read_only=True,
            initial_tab="review-agent-output",
            include_running_output=True,
            auto_output_readiness=readiness,
        )
        return True

    async def open_session_flow(self, task: Task) -> None:
        refreshed = await self.screen.ctx.api.get_task(task.id)
        if refreshed is not None:
            task = refreshed

        if task.status in (TaskStatus.REVIEW, TaskStatus.DONE):
            await self._open_read_only_output_for_terminal_status(task)
            return

        if task.task_type == TaskType.AUTO:
            await self._open_auto_session_flow(task)
            return

        await self._open_pair_session_flow(task)

    async def _open_read_only_output_for_terminal_status(self, task: Task) -> None:
        """Open review output for REVIEW and DONE tasks."""
        await self.screen._review.open_task_output_for_task(
            task,
            read_only=task.status == TaskStatus.DONE,
        )

    async def _open_auto_session_flow(self, task: Task) -> None:
        """Handle ENTER flow for AUTO tasks."""
        await self.screen.ctx.api.reconcile_running_tasks([task.id])
        if await self._open_blocked_auto_output_if_needed(task):
            return

        if await self.open_auto_output_for_task(task, quiet_unavailable=True):
            return

        if task.status == TaskStatus.BACKLOG:
            await self._start_and_open_auto_output(task)
            return
        if task.status == TaskStatus.IN_PROGRESS:
            await self._resume_or_start_in_progress_auto(task)
            return

    async def _open_blocked_auto_output_if_needed(self, task: Task) -> bool:
        """Open read-only running output for blocked AUTO tasks."""
        runtime_view = self.screen.ctx.api.get_runtime_view(task.id)
        if runtime_view is None or not runtime_view.is_blocked:
            return False

        await self.screen._review.open_task_output_for_task(
            task,
            read_only=True,
            initial_tab="review-agent-output",
            include_running_output=True,
        )
        return True

    async def _start_and_open_auto_output(self, task: Task) -> None:
        """Confirm start for AUTO task, then open running output."""
        if not await self.confirm_start_auto_task(task):
            return
        await self.start_agent_flow(task)
        await self._open_auto_output_after_start(task)

    async def _open_auto_output_after_start(self, task: Task) -> None:
        """Open AUTO output after a start request."""
        refreshed_after_start = await self.screen.ctx.api.get_task(task.id)
        await self.open_auto_output_for_task(
            refreshed_after_start or task,
            wait_for_running=True,
        )

    async def _resume_or_start_in_progress_auto(self, task: Task) -> None:
        """Open current AUTO output stream or start agent when idle."""
        runtime_view = self.screen.ctx.api.get_runtime_view(task.id)
        is_running = runtime_view.is_running if runtime_view is not None else False
        is_pending = runtime_view.is_pending if runtime_view is not None else False
        opened = await self.open_auto_output_for_task(
            task,
            wait_for_running=is_running,
        )
        if opened:
            return
        if is_running:
            self.screen.notify(
                "Agent is running; waiting for output stream to become available.",
                severity="information",
            )
            return
        if is_pending:
            self.screen.notify(
                "Agent start already queued; waiting for scheduler admission.",
                severity="information",
            )
            return
        self.screen.notify(
            "No active AUTO run detected. Press 'a' to start the agent.",
            severity="information",
        )

    async def _open_pair_session_flow(self, task: Task) -> None:
        """Handle ENTER flow for PAIR tasks."""
        resolved_task = await self._ensure_pair_agent_available(task)
        if resolved_task is None:
            return
        task = resolved_task

        wt_path = await self._ensure_workspace_path(task)
        if wt_path is None:
            return

        backend_readiness = await self.ensure_pair_terminal_backend_ready(task)
        if not backend_readiness.success or backend_readiness.backend is None:
            return

        terminal_backend = backend_readiness.backend

        await self._ensure_pair_session_exists(task, wt_path)

        if task.status == TaskStatus.IN_PROGRESS and not await self.confirm_attach_pair_session(
            task
        ):
            return

        if await self._show_pair_instructions_if_needed(task, wt_path, terminal_backend):
            return

        await self.do_open_pair_session(task, wt_path, terminal_backend)

    async def _ensure_pair_agent_available(self, task: Task) -> Task | None:
        """Ensure selected PAIR agent is available or task is updated to a fallback."""
        from kagan.core.agents.installer import check_agent_installed
        from kagan.tui.ui.modals.agent_choice import AgentChoiceResult

        agent_config = task.get_agent_config(self.screen.kagan_app.config)
        if check_agent_installed(agent_config.short_name):
            return task

        result = await self.handle_missing_agent(task, agent_config)
        if result == AgentChoiceResult.CANCELLED:
            return None
        if result == AgentChoiceResult.INSTALLED:
            return task

        fallback_agent = AgentChoiceResult.parse_fallback(result)
        if fallback_agent is None:
            return task

        await self.update_task_agent(task, fallback_agent)
        refreshed = await self.screen.ctx.api.get_task(task.id)
        resolved_task = refreshed or task
        resolved_config = resolved_task.get_agent_config(self.screen.kagan_app.config)
        if not check_agent_installed(resolved_config.short_name):
            return None
        return resolved_task

    async def _ensure_workspace_path(self, task: Task) -> Path | None:
        """Return existing workspace path or provision one from active repository."""
        wt_path = await self.screen.ctx.api.get_workspace_path(task.id)
        if wt_path is not None:
            return wt_path

        provision_result = await self.provision_workspace_for_active_repo(task)
        if not provision_result.success or provision_result.path is None:
            return None
        return provision_result.path

    async def _ensure_pair_session_exists(self, task: Task, workspace_path: Path) -> None:
        """Create PAIR session when missing."""
        # Session creation is routed through the core session service,
        # which handles MCP config generation with session-scoped capability profiles.
        # The session is not tied to the TUI process -- only attach requires a terminal.
        if await self.screen.ctx.api.session_exists(task.id):
            return
        self.screen.notify("Creating session...", severity="information")
        await self.screen.ctx.api.create_session(task.id, worktree_path=workspace_path)

    async def _show_pair_instructions_if_needed(
        self,
        task: Task,
        workspace_path: Path,
        terminal_backend: str,
    ) -> bool:
        """Show PAIR instructions modal when preference is enabled."""
        if not self.screen.kagan_app.config.ui.skip_pair_instructions:
            from kagan.tui.ui.modals.tmux_gateway import PairInstructionsModal

            result = await await_screen_result(
                self.screen.app,
                PairInstructionsModal(
                    task.id,
                    task.title,
                    terminal_backend,
                    self.startup_prompt_path_hint(workspace_path),
                ),
            )
            if result is None:
                return True
            if result == "skip_future":
                self.screen.kagan_app.config.ui.skip_pair_instructions = True
                self.screen.run_worker(
                    self.save_pair_instructions_preference(skip=True),
                    exclusive=True,
                    exit_on_error=False,
                )
            await self.do_open_pair_session(task, workspace_path, terminal_backend)
            return True
        return False

    def resolve_pair_terminal_backend(self, task: Task) -> str:
        from kagan.core.models.enums import resolve_pair_backend

        config_backend = getattr(
            self.screen.kagan_app.config.general,
            "default_pair_terminal_backend",
            "tmux",
        )
        return resolve_pair_backend(task.terminal_backend, config_backend)

    async def ensure_pair_terminal_backend_ready(self, task: Task) -> PairBackendReadiness:
        from kagan.tui.terminals.installer import (
            check_terminal_installed,
            first_available_pair_backend,
        )
        from kagan.tui.ui.modals.terminal_install import TerminalInstallModal

        backend = self.resolve_pair_terminal_backend(task)
        is_windows = platform.system() == "Windows"

        if backend in {"vscode", "cursor"}:
            if check_terminal_installed(backend):
                return PairBackendReadiness(success=True, backend=backend)
            self.screen.notify(
                f"{backend} is not installed. Install it and retry.",
                severity="warning",
            )
            return PairBackendReadiness(
                success=False,
                error=f"{backend} launcher is not installed",
            )

        if backend == "tmux":
            if check_terminal_installed("tmux"):
                return PairBackendReadiness(success=True, backend=backend)
            if is_windows:
                fallback = first_available_pair_backend(windows=True)
                if fallback is not None:
                    await self.screen.ctx.api.update_task(task.id, terminal_backend=fallback)
                    self.screen.notify(
                        f"tmux not found on Windows. Using {fallback} for this task.",
                        severity="information",
                    )
                    return PairBackendReadiness(success=True, backend=fallback)
                self.screen.notify(
                    "PAIR cancelled: install VS Code or Cursor to continue on Windows.",
                    severity="warning",
                )
                return PairBackendReadiness(
                    success=False,
                    error="No supported PAIR launcher available on Windows",
                )

            installed_tmux = await await_screen_result(
                self.screen.app,
                TerminalInstallModal("tmux"),
            )
            if installed_tmux and check_terminal_installed("tmux"):
                return PairBackendReadiness(success=True, backend=backend)

            fallback = first_available_pair_backend(windows=False)
            if fallback is not None:
                await self.screen.ctx.api.update_task(task.id, terminal_backend=fallback)
                self.screen.notify(
                    "tmux not installed. Using fallback launcher "
                    f"{fallback}. VS Code: https://code.visualstudio.com/download "
                    "Cursor: https://cursor.com/downloads",
                    severity="information",
                )
                return PairBackendReadiness(success=True, backend=fallback)
            self.screen.notify(
                "PAIR cancelled: install tmux (recommended), or install VS Code/Cursor "
                "for external development.",
                severity="warning",
            )
            return PairBackendReadiness(
                success=False,
                error="No supported PAIR launcher available",
            )

        self.screen.notify(f"Unsupported PAIR launcher: {backend}", severity="warning")
        return PairBackendReadiness(success=False, error=f"Unsupported PAIR launcher: {backend}")

    @staticmethod
    def startup_prompt_path_hint(workspace_path: Path) -> Path:
        return workspace_path / ".kagan" / "start_prompt.md"

    async def do_open_pair_session(
        self,
        task: Task,
        workspace_path: Path | None = None,
        terminal_backend: str | None = None,
    ) -> None:
        """Attach to an existing PAIR session.

        Session creation is handled separately by ``open_session_flow`` via
        the core session service (no TUI residency required).  Attach,
        however, requires terminal access and only works from the TUI process.
        """
        try:
            if task.status == TaskStatus.BACKLOG:
                await self.screen.ctx.api.update_task(task.id, status=TaskStatus.IN_PROGRESS)
                await self.screen._board.refresh_board()

            # Attach requires terminal access -- this is the only TUI-resident step.
            with self.screen.app.suspend():
                attached = await self.screen.ctx.api.attach_session(task.id)

            backend = terminal_backend or self.resolve_pair_terminal_backend(task)
            if not attached:
                self.screen.notify("Failed to open PAIR session", severity="warning")
                return

            if backend != "tmux":
                prompt_path = (
                    self.startup_prompt_path_hint(workspace_path)
                    if workspace_path is not None
                    else Path(".kagan/start_prompt.md")
                )
                self.screen.notify(
                    f"Workspace opened externally. Use startup prompt: {prompt_path}",
                    severity="information",
                )
                return

            session_still_exists = await self.screen.ctx.api.session_exists(task.id)
            if session_still_exists:
                return

            from kagan.tui.ui.modals.confirm import ConfirmModal

            confirmed = await await_screen_result(
                self.screen.app, ConfirmModal("Session Complete", "Move task to REVIEW?")
            )
            if confirmed:
                await self.screen.ctx.api.update_task(task.id, status=TaskStatus.REVIEW)
                await self.screen._board.refresh_board()

        except Exception as e:
            from kagan.core.tmux import TmuxError

            if isinstance(e, TmuxError):
                self.screen.notify(f"Tmux error: {e}", severity="error")

    async def start_agent_flow(self, task: Task) -> None:
        if task.task_type == TaskType.PAIR:
            return

        await self.screen.ctx.api.reconcile_running_tasks([task.id])
        runtime_view = self.screen.ctx.api.get_runtime_view(task.id)
        is_running = runtime_view.is_running if runtime_view is not None else False
        if is_running:
            self.screen.notify(
                "Agent already running for this task; opening Task Output.",
                severity="information",
            )
            await self.open_auto_output_for_task(
                task,
                wait_for_running=True,
                quiet_unavailable=True,
            )
            return

        wt_path = await self.screen.ctx.api.get_workspace_path(task.id)
        if wt_path is None:
            provision_result = await self.provision_workspace_for_active_repo(task)
            if not provision_result.success or provision_result.path is None:
                return
            wt_path = provision_result.path

        if task.status == TaskStatus.BACKLOG:
            await self.screen.ctx.api.move_task(task.id, TaskStatus.IN_PROGRESS)
            refreshed = await self.screen.ctx.api.get_task(task.id)
            if refreshed:
                task = refreshed
            await self.screen._board.refresh_board()

        self.screen.notify("Starting agent...", severity="information")

        submitted = await self.screen.ctx.api.submit_job(
            task.id,
            "start_agent",
        )
        terminal = await self.wait_for_job_terminal(submitted.job_id, task_id=task.id)
        payload = self.job_result_payload(terminal)
        if payload is not None and not bool(payload.get("success", False)):
            self.screen.notify(
                self.job_message(terminal, "Failed to start agent"),
                severity="warning",
            )
            return
        if terminal is not None and terminal.status in {JobStatus.FAILED, JobStatus.CANCELLED}:
            self.screen.notify(
                self.job_message(terminal, "Failed to start agent"),
                severity="warning",
            )
            return
        if payload is None:
            self.screen.notify(self.START_JOB_PENDING_MESSAGE, severity="information")
            return

        runtime = payload.get("runtime") if payload is not None else None
        runtime_running = isinstance(runtime, dict) and bool(runtime.get("is_running", False))
        if runtime_running or self.screen.ctx.api.is_automation_running(task.id):
            self.screen._board.set_card_indicator(task.id, CardIndicator.RUNNING, is_active=True)
            self.screen.notify(f"Agent started: {task.id[:8]}", severity="information")
            return

        self.screen.notify(
            self.job_message(terminal, "Agent start queued"),
            severity="information",
        )

    async def apply_global_agent_selection(self, selected: str) -> None:
        from kagan.core.builtin_agents import get_builtin_agent

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
