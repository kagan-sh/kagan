"""ReviewModal — task review with diff inspection, AI review, and agent output."""

from __future__ import annotations

import asyncio
import contextlib
import re
from typing import TYPE_CHECKING, Any, Final, Literal

from acp import RequestError
from sqlalchemy.exc import OperationalError
from textual import on
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.css.query import NoMatches
from textual.widgets import (
    Button,
    DataTable,
    Footer,
    Label,
    LoadingIndicator,
    RichLog,
    Rule,
    Static,
    TabbedContent,
    TabPane,
)

from kagan.core.acp import messages
from kagan.core.adapters.db.repositories.base import RepositoryClosing
from kagan.core.agents.agent_factory import AgentFactory, create_agent
from kagan.core.constants import MODAL_TITLE_MAX_LENGTH
from kagan.core.limits import AGENT_TIMEOUT
from kagan.core.models.enums import StreamPhase, TaskStatus, TaskType
from kagan.core.services.jobs import JobRecord, JobStatus
from kagan.tui.keybindings import REVIEW_BINDINGS
from kagan.tui.ui.modals.base import KaganModalScreen
from kagan.tui.ui.modals.review_prompt import (
    build_review_prompt,
    extract_review_decision,
    truncate_queue_payload,
)
from kagan.tui.ui.user_messages import task_deleted_close_message, task_moved_close_message
from kagan.tui.ui.utils.agent_stream_router import AgentStreamRouter
from kagan.tui.ui.utils.helpers import (
    WAVE_FRAMES,
    WAVE_INTERVAL_MS,
    colorize_diff_line,
    copy_with_notification,
    parse_agent_exit_code,
)
from kagan.tui.ui.widgets import ChatPanel, StreamingOutput

_SHUTDOWN_ERRORS = (RepositoryClosing, OperationalError)

if TYPE_CHECKING:
    from textual.app import ComposeResult
    from textual.timer import Timer
    from textual.worker import Worker

    from kagan.core.acp import Agent
    from kagan.core.adapters.db.schema import Task
    from kagan.core.config import AgentConfig
    from kagan.core.services.diffs import FileDiff, RepoDiff

DiffModalResult = Literal["approve", "reject"]
DIFF_MODAL_APPROVE_RESULT: Final = "approve"
DIFF_MODAL_REJECT_RESULT: Final = "reject"


class ReviewModal(KaganModalScreen[str | None]):
    """Modal for reviewing task changes."""

    BINDINGS = REVIEW_BINDINGS
    START_JOB_PENDING_MESSAGE = "Agent start requested; waiting for scheduler."
    STOP_JOB_PENDING_MESSAGE = "Agent stop requested; waiting for scheduler."

    _LIVE_ATTACH_TIMEOUT_SECONDS = 1.5

    _agent: Agent | None
    _live_output_agent: object | None
    _live_review_agent: object | None
    _live_output_attached: bool
    _live_output_wait_noted: bool
    _live_review_attached: bool
    _loaded_agent_output_entry_ids: set[str]
    _runtime_poll_timer: Timer | None

    def __init__(
        self,
        task: Task,
        agent_config: AgentConfig,
        base_branch: str = "main",
        agent_factory: AgentFactory = create_agent,
        execution_id: str | None = None,
        run_count: int = 0,
        running_agent: object | None = None,
        review_agent: object | None = None,
        is_reviewing: bool = False,
        is_running: bool = False,
        is_blocked: bool = False,
        blocked_reason: str | None = None,
        blocked_by_task_ids: tuple[str, ...] = (),
        overlap_hints: tuple[str, ...] = (),
        is_pending: bool = False,
        pending_reason: str | None = None,
        read_only: bool = False,
        initial_tab: str = "review-summary",
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self._task_model = task
        self._agent_config = agent_config
        self._base_branch = base_branch
        self._agent_factory = agent_factory
        self._agent: Agent | None = None
        self._execution_id = execution_id
        self._run_count = run_count
        self._live_output_agent = self._stream_resolve_live_handle(running_agent)
        self._live_review_agent = self._stream_resolve_live_handle(review_agent)
        self._is_reviewing = is_reviewing
        self._is_running = is_running
        self._is_blocked = is_blocked
        self._blocked_reason = blocked_reason
        self._blocked_by_task_ids = blocked_by_task_ids
        self._overlap_hints = overlap_hints
        self._is_pending = is_pending
        self._pending_reason = pending_reason
        self._read_only = read_only
        self._initial_tab = initial_tab
        self._live_output_attached = False
        self._live_output_wait_noted = False
        self._live_review_attached = False
        self._loaded_agent_output_entry_ids: set[str] = set()
        self._review_log_loaded = False
        self._phase: StreamPhase = StreamPhase.IDLE
        self._diff_stats: str = ""
        self._diff_text: str = ""
        self._file_diffs: dict[str, FileDiff] = {}
        self._no_changes: bool = False
        self._anim_timer: Timer | None = None
        self._anim_frame: int = 0
        self._prompt_worker: Worker[None] | None = None
        self._review_queue_pending = False
        self._implementation_queue_pending = False
        self._hydrated = False
        self._runtime_poll_timer = None
        self._agent_stream = AgentStreamRouter(
            get_output=self._stream_target_output,
            on_update=self._stream_on_update,
            on_complete=self._stream_on_complete,
            on_fail=self._stream_on_fail,
        )

    @staticmethod
    def _state_attr(state: object | None, name: str, default: Any = None) -> Any:
        if state is None:
            return default
        if isinstance(state, dict):
            return state.get(name, default)
        return getattr(state, name, default)

    @classmethod
    def _state_bool(cls, state: object | None, name: str) -> bool:
        return bool(cls._state_attr(state, name, False))

    @classmethod
    def _state_tuple(cls, state: object | None, name: str) -> tuple[str, ...]:
        value = cls._state_attr(state, name, ())
        if isinstance(value, (list, tuple)):
            return tuple(str(item) for item in value if str(item).strip())
        return ()

    @classmethod
    def _execution_metadata(cls, execution: object | None) -> dict[str, Any]:
        metadata = cls._state_attr(execution, "metadata_", None)
        if isinstance(metadata, dict):
            return metadata
        metadata = cls._state_attr(execution, "metadata", None)
        if isinstance(metadata, dict):
            return metadata
        return {}

    @classmethod
    def _stream_resolve_live_handle(
        cls,
        source: object | None,
        attr: str | None = None,
    ) -> object | None:
        candidate = source if attr is None else cls._state_attr(source, attr)
        if callable(getattr(candidate, "set_message_target", None)):
            return candidate
        return None

    @staticmethod
    def _stream_set_target(handle: object | None, target: object | None) -> bool:
        setter = getattr(handle, "set_message_target", None)
        if not callable(setter):
            return False
        setter(target)
        return True

    # ------------------------------------------------------------------
    # Compose & lifecycle
    # ------------------------------------------------------------------

    def compose(self) -> ComposeResult:
        with Vertical(id="review-modal-container"):
            yield Label(
                f"Task Output: {self._task_model.title[:MODAL_TITLE_MAX_LENGTH]}",
                id="review-title",
                classes="modal-title",
            )
            yield Label(
                f"Branch: task-{self._task_model.short_id} → {self._base_branch}",
                id="branch-info",
                classes="branch-info",
            )
            yield Label("", id="task-output-status", classes="task-output-status")
            yield Rule()

            yield LoadingIndicator(id="review-loading", classes="review-loading")

            with TabbedContent(id="review-tabs", classes="hidden"):
                with TabPane("Summary", id="review-summary"):
                    with VerticalScroll(id="review-summary-scroll"):
                        yield Label("Overview", classes="section-title")
                        with Horizontal(id="review-stats", classes="hidden"):
                            yield Static(id="stat-additions", classes="stat-card")
                            yield Static(id="stat-deletions", classes="stat-card")
                            yield Static(id="stat-files", classes="stat-card")

                        yield Label("Commits", classes="section-title")
                        yield DataTable(id="commits-table")

                        yield Label("Changes", classes="section-title")
                        yield Static(id="diff-stats")

                        if self._task_model.description.strip():
                            yield Label("Description", classes="section-title")
                            yield Static(
                                self._task_model.description.strip(),
                                id="review-description",
                            )

                with TabPane("Diff", id="review-diff"):
                    with Horizontal(id="diff-pane"):
                        yield DataTable(id="diff-files", classes="hidden")
                        yield RichLog(id="diff-log", wrap=False, markup=True, auto_scroll=False)

                with TabPane("Review Output", id="review-ai"):
                    with Vertical(id="ai-review-section"):
                        with Horizontal(id="ai-review-header"):
                            yield Label("Review Output", classes="section-title")
                            yield Static("", classes="spacer")
                            yield Static(
                                "Decision: Pending",
                                id="decision-badge",
                                classes="decision-badge decision-pending",
                            )
                            yield Static(
                                "○ Ready",
                                id="phase-badge",
                                classes="phase-badge phase-idle",
                            )
                        yield Static("", id="review-state-note", classes="task-output-state-note")
                        yield ChatPanel(
                            None,
                            allow_input=True,
                            input_placeholder="Send a follow-up message...",
                            output_id="ai-review-output",
                            id="ai-review-chat",
                        )
                with TabPane("Agent Output", id="review-agent-output"):
                    yield Static(
                        "",
                        id="agent-output-state-note",
                        classes="task-output-state-note",
                    )
                    yield ChatPanel(
                        self._execution_id,
                        allow_input=True,
                        input_placeholder="Queue implementation follow-up...",
                        output_id="review-agent-output",
                        id="review-agent-output-chat",
                    )

            yield Rule()

            with Horizontal(classes="button-row hidden"):
                with Horizontal(classes="button-group button-group-start"):
                    if self._task_model.task_type == TaskType.PAIR:
                        yield Button("Attach", variant="default", id="attach-btn")
                    yield Button("Rebase (R)", variant="default", id="rebase-btn")
                    yield Button("Review (g)", id="generate-btn", variant="primary")
                    yield Button("Cancel", id="cancel-btn", variant="warning", classes="hidden")
                yield Static("", classes="spacer")
                with Horizontal(classes="button-group button-group-end"):
                    yield Button("Approve (Enter)", variant="success", id="approve-btn")
                    yield Button("Reject (r)", variant="error", id="reject-btn")

        yield Footer(show_command_palette=False)

    async def on_mount(self) -> None:
        """Render shell immediately, then hydrate heavy content in background."""
        self._show_shell()
        self._bind_task_updates()
        self._runtime_poll_timer = self.set_interval(1.0, self._schedule_runtime_refresh)
        with contextlib.suppress(Exception):
            await self._stream_attach_live_output_if_available(wait_for_agent=False)
        with contextlib.suppress(Exception):
            await self._stream_attach_live_review_if_available()
        self.run_worker(self._hydrate_content, exclusive=True, exit_on_error=False)

    def _show_shell(self) -> None:
        """Make modal interactive before loading expensive data."""
        with contextlib.suppress(NoMatches):
            self.query_one("#review-loading", LoadingIndicator).remove()
        self.query_one("#review-tabs").remove_class("hidden")
        if not self._read_only:
            self.query_one(".button-row").remove_class("hidden")
        self._actions_set_active_tab(self._initial_tab)
        self._queue_sync_agent_output_visibility()
        self._queue_sync_review_visibility()
        self._state_refresh_task_output_labels()

    def _bind_task_updates(self) -> None:
        self.kagan_app.task_changed_signal.subscribe(self, self._on_task_changed)

    async def _hydrate_content(self) -> None:
        """Load commits, diffs and history without blocking initial paint."""
        from kagan.core.debug_log import log

        workspaces = []
        try:
            workspaces = await self.ctx.api.list_workspaces(task_id=self._task_model.id)
            if workspaces:
                actual_branch = workspaces[0].branch_name
                branch_info = self.query_one("#branch-info", Label)
                branch_info.update(f"Branch: {actual_branch} → {self._base_branch}")

            commits_task = self.ctx.api.get_workspace_commit_log(
                self._task_model.id,
                base_branch=self._base_branch,
            )
            diff_stats_task = self.ctx.api.get_workspace_diff_stats(
                self._task_model.id,
                base_branch=self._base_branch,
            )
            diff_task = self.ctx.api.get_workspace_diff(
                self._task_model.id,
                base_branch=self._base_branch,
            )
            commits, diff_stats, self._diff_text = await asyncio.gather(
                commits_task,
                diff_stats_task,
                diff_task,
            )

            self._diff_populate_commits(commits)
            self.query_one("#diff-stats", Static).update(diff_stats or "[dim](No changes)[/dim]")
            self._diff_stats = diff_stats or ""
            self._no_changes = not commits and not self._diff_stats
            await self._diff_populate_pane(workspaces)
        except Exception as exc:
            log.error(
                "[ReviewModal] Failed to hydrate diff/commit data",
                task_id=self._task_model.id[:8],
                error=str(exc),
            )
            self._diff_text = ""
            self._diff_stats = ""
            self._no_changes = False
            with contextlib.suppress(NoMatches):
                self.query_one("#diff-stats", Static).update(
                    "[dim](Unable to load diff data)[/dim]"
                )

        if self._no_changes:
            self.query_one("#approve-btn", Button).label = "Close task"

        history_results = await asyncio.gather(
            self._stream_load_agent_output_history(),
            self._queue_configure_review_chat(),
            self._queue_configure_agent_output_chat(),
            return_exceptions=True,
        )
        for result in history_results:
            if isinstance(result, Exception):
                log.error(
                    "[ReviewModal] Failed to hydrate output/chat state",
                    task_id=self._task_model.id[:8],
                    error=str(result),
                )

        queue_results = await asyncio.gather(
            self._queue_refresh_review_state(),
            self._queue_refresh_implementation_state(),
            return_exceptions=True,
        )
        for result in queue_results:
            if isinstance(result, Exception):
                log.error(
                    "[ReviewModal] Failed to refresh queue state",
                    task_id=self._task_model.id[:8],
                    error=str(result),
                )

        if self._read_only:
            self.query_one("#generate-btn", Button).add_class("hidden")
            self.query_one("#cancel-btn", Button).add_class("hidden")

        with contextlib.suppress(Exception):
            await self._stream_load_prior_review()
        with contextlib.suppress(Exception):
            await self._stream_attach_live_output_if_available()
        with contextlib.suppress(Exception):
            await self._stream_attach_live_review_if_available()
        with contextlib.suppress(Exception):
            await self._stream_maybe_auto_start_pair_review()
        self._hydrated = True
        self._state_refresh_task_output_labels()

    async def _on_task_changed(self, task_id: str) -> None:
        if not self.is_mounted or task_id != self._task_model.id:
            return
        self._schedule_runtime_refresh()

    def _schedule_runtime_refresh(self) -> None:
        if not self.is_mounted:
            return
        self.run_worker(
            self._refresh_runtime_state_non_blocking,
            group="review-runtime-refresh",
            exclusive=True,
            exit_on_error=False,
        )

    async def _refresh_runtime_state_non_blocking(self) -> None:
        await self._refresh_runtime_state(wait_for_live_agent=False)

    # ------------------------------------------------------------------
    # Event handlers
    # ------------------------------------------------------------------

    @staticmethod
    def _resolve_diff_row_key(row_key: object) -> str:
        value = getattr(row_key, "value", row_key)
        return str(value)

    @on(DataTable.RowHighlighted, "#diff-files")
    def on_diff_file_highlighted(self, event: DataTable.RowHighlighted) -> None:
        """Preview the highlighted file diff."""
        self._actions_set_active_tab("review-diff")
        self._diff_show_file(self._resolve_diff_row_key(event.row_key))

    @on(DataTable.RowSelected, "#diff-files")
    def on_diff_file_selected(self, event: DataTable.RowSelected) -> None:
        """Open the selected file diff in the diff tab."""
        self._actions_set_active_tab("review-diff")
        self._diff_show_file(self._resolve_diff_row_key(event.row_key))

    @on(DataTable.CellHighlighted, "#diff-files")
    def on_diff_file_cell_highlighted(self, event: DataTable.CellHighlighted) -> None:
        """Preview the highlighted file diff cell."""
        self._actions_set_active_tab("review-diff")
        self._diff_show_file(self._resolve_diff_row_key(event.cell_key.row_key))

    @on(DataTable.CellSelected, "#diff-files")
    def on_diff_file_cell_selected(self, event: DataTable.CellSelected) -> None:
        """Open the selected file diff cell in the diff tab."""
        self._actions_set_active_tab("review-diff")
        self._diff_show_file(self._resolve_diff_row_key(event.cell_key.row_key))

    @on(Button.Pressed, "#generate-btn")
    async def on_generate_btn(self) -> None:
        """Start or regenerate the review stream."""
        await self.action_generate_review()

    @on(Button.Pressed, "#cancel-btn")
    async def on_cancel_btn(self) -> None:
        """Cancel the active review stream."""
        await self.action_cancel_review()

    @on(Button.Pressed, "#rebase-btn")
    async def on_rebase_btn(self) -> None:
        """Rebase the task branch onto the base branch."""
        await self.action_rebase()

    @on(Button.Pressed, "#attach-btn")
    async def on_attach_btn(self) -> None:
        """Attach to the active PAIR session."""
        await self.action_attach_session()

    @on(Button.Pressed, "#approve-btn")
    def on_approve_btn(self) -> None:
        """Complete the review with approval."""
        self.action_approve()

    @on(Button.Pressed, "#reject-btn")
    def on_reject_btn(self) -> None:
        """Complete the review with rejection."""
        self.action_reject()

    @on(messages.AgentMessage)
    async def on_agent_message(self, message: messages.AgentMessage) -> None:
        """Route agent stream events to the active output pane."""
        await self._agent_stream.dispatch(message)

    async def _refresh_runtime_state(self, *, wait_for_live_agent: bool = True) -> None:
        if not self.is_mounted:
            return
        with contextlib.suppress(*_SHUTDOWN_ERRORS):
            await self.ctx.api.reconcile_running_tasks([self._task_model.id])
        try:
            latest = await self.ctx.api.get_task(self._task_model.id)
        except _SHUTDOWN_ERRORS:
            return
        if latest is None:
            self.notify(task_deleted_close_message("review"), severity="warning")
            self.dismiss(None)
            return

        previous_status = self._task_model.status
        self._task_model = latest
        if previous_status is TaskStatus.REVIEW and latest.status is not TaskStatus.REVIEW:
            self.notify(
                task_moved_close_message(latest.status.value),
                severity="information",
            )
            self.dismiss(None)
            return
        if previous_status == TaskStatus.IN_PROGRESS and latest.status == TaskStatus.REVIEW:
            self._read_only = False
            self.query_one(".button-row").remove_class("hidden")
            self.query_one("#generate-btn", Button).remove_class("hidden")
            self.query_one("#cancel-btn", Button).add_class("hidden")
            self._state_set_phase(StreamPhase.IDLE)

        runtime_view = self.ctx.api.get_runtime_view(self._task_model.id)
        previous_execution_id = self._execution_id
        self._is_running = self._state_bool(runtime_view, "is_running")
        self._is_reviewing = self._state_bool(runtime_view, "is_reviewing")
        self._is_blocked = self._state_bool(runtime_view, "is_blocked")
        self._blocked_reason = (
            self._state_attr(runtime_view, "blocked_reason") if self._is_blocked else None
        )
        self._blocked_by_task_ids = (
            self._state_tuple(runtime_view, "blocked_by_task_ids") if self._is_blocked else ()
        )
        self._overlap_hints = (
            self._state_tuple(runtime_view, "overlap_hints") if self._is_blocked else ()
        )
        self._is_pending = self._state_bool(runtime_view, "is_pending")
        self._pending_reason = (
            self._state_attr(runtime_view, "pending_reason") if self._is_pending else None
        )
        runtime_execution_id = self._state_attr(runtime_view, "execution_id")
        if runtime_execution_id is not None:
            self._execution_id = runtime_execution_id
        if self._execution_id != previous_execution_id:
            self._loaded_agent_output_entry_ids.clear()
        self._live_output_agent = self._stream_resolve_live_handle(runtime_view, "running_agent")
        self._live_review_agent = self._stream_resolve_live_handle(runtime_view, "review_agent")
        if self._live_output_agent is None:
            self._live_output_attached = False
        if self._live_review_agent is None:
            self._live_review_attached = False

        await self._stream_attach_live_output_if_available(wait_for_agent=wait_for_live_agent)
        await self._stream_attach_live_review_if_available()
        try:
            should_poll = (self._is_running and not self._live_output_attached) or (
                self._is_reviewing and not self._live_review_attached
            )
            if should_poll:
                await self._stream_load_agent_output_history()
            await asyncio.gather(
                self._queue_refresh_review_state(),
                self._queue_refresh_implementation_state(),
            )
            if self._hydrated:
                await self._stream_load_prior_review()
        except _SHUTDOWN_ERRORS:
            return
        self._state_refresh_task_output_labels()

    async def on_unmount(self) -> None:
        """Release stream resources before closing the modal."""
        with contextlib.suppress(Exception):
            self.kagan_app.task_changed_signal.unsubscribe(self)
        if self._runtime_poll_timer is not None:
            self._runtime_poll_timer.stop()
            self._runtime_poll_timer = None
        self._state_stop_animation()
        if self._prompt_worker is not None and not self._prompt_worker.is_finished:
            self._prompt_worker.cancel()
        if self._agent:
            await self._agent.stop()
        if self._live_output_agent is not None:
            self._stream_set_target(self._live_output_agent, None)
        if self._live_review_agent is not None:
            self._stream_set_target(self._live_review_agent, None)

    # ------------------------------------------------------------------
    # State management
    # ------------------------------------------------------------------

    def _state_set_phase(self, phase: StreamPhase) -> None:
        """Update phase and UI state."""
        self._phase = phase
        badge = self.query_one("#phase-badge", Static)
        badge.update(f"{phase.icon} {phase.label}")
        badge.set_classes(f"phase-badge phase-{phase.value}")

        gen_btn = self.query_one("#generate-btn", Button)
        cancel_btn = self.query_one("#cancel-btn", Button)
        if self._read_only:
            gen_btn.add_class("hidden")
            cancel_btn.add_class("hidden")
            self._state_sync_review_actions()
            return

        if phase == StreamPhase.IDLE:
            self._state_stop_animation()
            gen_btn.label = "Review (g)"
            gen_btn.variant = "primary"
            gen_btn.remove_class("hidden")
            gen_btn.disabled = False
            cancel_btn.add_class("hidden")
        elif phase in (StreamPhase.THINKING, StreamPhase.STREAMING):
            self._state_start_animation()
            gen_btn.add_class("hidden")
            cancel_btn.remove_class("hidden")
        else:
            self._state_stop_animation()
            gen_btn.label = "Regenerate (g)"
            gen_btn.variant = "default"
            gen_btn.remove_class("hidden")
            gen_btn.disabled = False
            cancel_btn.add_class("hidden")
        self._state_sync_review_actions()

    def _state_set_decision(self, decision: str | None) -> None:
        badge = self.query_one("#decision-badge", Static)
        if decision == "approved":
            badge.update("Decision: Approve")
            badge.set_classes("decision-badge decision-approved")
            return
        if decision == "rejected":
            badge.update("Decision: Reject")
            badge.set_classes("decision-badge decision-rejected")
            return
        badge.update("Decision: Pending")
        badge.set_classes("decision-badge decision-pending")

    def _state_sync_decision_from_output(self) -> None:
        output = self._stream_review_output_panel().output
        decision = extract_review_decision(output.get_text_content())
        self._state_set_decision(decision)

    def _state_sync_review_actions(self) -> None:
        if self._read_only:
            return
        try:
            approve_btn = self.query_one("#approve-btn", Button)
        except NoMatches:
            return
        queue_pending = self._review_queue_pending
        review_running = self._phase in (
            StreamPhase.THINKING,
            StreamPhase.STREAMING,
        )
        approve_btn.disabled = queue_pending or review_running
        if queue_pending:
            approve_btn.tooltip = "Process queued review messages before approval."
        elif review_running:
            approve_btn.tooltip = "Wait for review to complete before approval."
        else:
            approve_btn.tooltip = ""

    def _state_start_animation(self) -> None:
        """Start wave animation for thinking/streaming state."""
        if self._anim_timer is None:
            self._anim_frame = 0
            self._anim_timer = self.set_interval(WAVE_INTERVAL_MS / 1000, self._state_next_frame)

    def _state_stop_animation(self) -> None:
        """Stop wave animation."""
        if self._anim_timer is not None:
            self._anim_timer.stop()
            self._anim_timer = None

    def _state_next_frame(self) -> None:
        """Advance the wave animation frame."""
        self._anim_frame = (self._anim_frame + 1) % len(WAVE_FRAMES)
        badge = self.query_one("#phase-badge", Static)
        badge.update(f"{WAVE_FRAMES[self._anim_frame]} {self._phase.label}")

    def _state_refresh_task_output_labels(self) -> None:
        status_text = self._task_model.status.value.upper()
        if self._is_reviewing:
            runtime_text = "reviewing"
        elif self._is_running:
            runtime_text = "running"
        elif self._is_blocked:
            runtime_text = "blocked"
        else:
            runtime_text = "idle"
        self.query_one("#task-output-status", Label).update(
            f"Task: {status_text} | Runtime: {runtime_text}"
        )
        self.query_one("#review-state-note", Static).update(self._state_review_note())
        self.query_one("#agent-output-state-note", Static).update(self._state_agent_output_note())

    def _state_review_note(self) -> str:
        if self._is_reviewing or self._phase in (
            StreamPhase.THINKING,
            StreamPhase.STREAMING,
        ):
            return "Reviewer is running."
        if self._review_log_loaded:
            if self._task_model.status is TaskStatus.REVIEW:
                return "Showing latest reviewer output."
            return (
                "Showing latest reviewer output from a previous review cycle. "
                f"Task is in {self._task_model.status.value.upper()}."
            )
        return f"Reviewer has not run yet. Task is in {self._task_model.status.value.upper()}."

    def _state_agent_output_note(self) -> str:
        if self._is_running:
            if self._live_output_attached:
                return "Implementation stream is live."
            return "Implementation run is active. Waiting for stream attachment."
        if self._is_blocked:
            reason = self._blocked_reason or "Task is blocked by overlapping changes."
            blocked_by = ", ".join(
                self._state_format_task_ref(task_id) for task_id in self._blocked_by_task_ids
            )
            overlap = ", ".join(self._overlap_hints[:2])
            details: list[str] = [f"Blocked: {reason}"]
            if blocked_by:
                details.append(f"Waiting on {blocked_by}.")
            if overlap:
                details.append(f"Overlap hint: {overlap}.")
            return " ".join(details)
        if self._is_pending:
            return self._pending_reason or "Queued for scheduler admission."
        if self._execution_id is not None:
            return "Showing latest saved implementation output."
        return "Implementation agent is idle."

    @staticmethod
    def _state_format_task_ref(task_id: str) -> str:
        normalized = task_id.strip()
        if not normalized:
            return ""
        if normalized.startswith("#"):
            return normalized
        return f"#{normalized[:8]}"

    # ------------------------------------------------------------------
    # Stream management
    # ------------------------------------------------------------------

    async def _stream_resolve_execution_id(self) -> str | None:
        if self._execution_id is not None:
            return self._execution_id
        execution = await self.ctx.api.get_latest_execution_for_task(self._task_model.id)
        if execution is None:
            return None
        if self._execution_id != execution.id:
            self._loaded_agent_output_entry_ids.clear()
        self._execution_id = execution.id
        return self._execution_id

    def _stream_agent_output_panel(self) -> ChatPanel:
        return self.query_one("#review-agent-output-chat", ChatPanel)

    def _stream_review_output_panel(self) -> ChatPanel:
        return self.query_one("#ai-review-chat", ChatPanel)

    def _stream_target_output(self) -> StreamingOutput:
        if self._live_review_attached or self._agent is not None:
            return self._stream_review_output_panel().output
        if self._live_output_attached or self._initial_tab == "review-agent-output":
            return self._stream_agent_output_panel().output
        return self._stream_review_output_panel().output

    async def _stream_load_agent_output_history(self) -> None:
        execution_id = await self._stream_resolve_execution_id()
        panel = self._stream_agent_output_panel()
        if execution_id is None:
            if not self._is_running and not self._loaded_agent_output_entry_ids:
                await panel.output.post_note("No execution logs available", classes="warning")
            return

        entries = await self.ctx.api.get_execution_log_entries(execution_id)
        if not entries:
            if not self._is_running and not self._loaded_agent_output_entry_ids:
                await panel.output.post_note("No execution logs available", classes="warning")
            return

        indexed_entries = [
            (str(getattr(entry, "id", f"idx-{index}")), index, entry)
            for index, entry in enumerate(entries)
        ]
        new_entries = [
            (entry_id, index, entry)
            for entry_id, index, entry in indexed_entries
            if entry_id not in self._loaded_agent_output_entry_ids
        ]
        if not new_entries:
            return

        execution = await self.ctx.api.get_execution(execution_id)
        metadata = self._execution_metadata(execution)
        has_review_result = "review_result" in metadata
        review_log_start_index = metadata.get("review_log_start_index")

        # Determine which entries are review vs implementation
        review_indices: set[int] = set()
        if review_log_start_index is not None:
            # Use the boundary marker stored before review began
            review_indices = {i for i in range(review_log_start_index, len(indexed_entries))}
        elif has_review_result and len(indexed_entries) > 1:
            # Backward compat: fall back to "last entry = review" heuristic
            review_indices = {len(indexed_entries) - 1}

        rendered_impl_output = False
        has_impl_entries = False
        review_new_entries = []
        for entry_id, index, entry in new_entries:
            self._loaded_agent_output_entry_ids.add(entry_id)
            if index in review_indices:
                review_new_entries.append(entry)
                continue
            has_impl_entries = True
            if not entry.logs:
                continue
            panel.set_execution_id(None)
            for line in entry.logs.splitlines():
                rendered_impl_output = await panel._render_log_line(line) or rendered_impl_output

        if has_impl_entries and not rendered_impl_output:
            await panel.output.post_note(
                "Execution history is present but contains no displayable output yet.",
                classes="warning",
            )

        if review_new_entries:
            chat_panel = self._stream_review_output_panel()
            chat_panel.remove_class("hidden")
            for entry in review_new_entries:
                if not entry.logs:
                    continue
                for line in entry.logs.splitlines():
                    await chat_panel._render_log_line(line)
            self._review_log_loaded = True
            if has_review_result:
                self._state_sync_decision_from_output()
                self._state_set_phase(StreamPhase.COMPLETE)

    async def _stream_attach_live_review_if_available(self) -> None:
        if self._live_review_attached:
            return
        if not self._is_reviewing:
            return
        if self._live_review_agent is None:
            runtime_view = self.ctx.api.get_runtime_view(self._task_model.id)
            self._live_review_agent = self._stream_resolve_live_handle(runtime_view, "review_agent")
        if self._live_review_agent is None:
            return
        chat_panel = self._stream_review_output_panel()
        chat_panel.remove_class("hidden")
        self._live_review_attached = True
        self._stream_set_target(self._live_review_agent, self)
        await chat_panel.output.post_note("Connected to live review stream", classes="info")
        self._state_set_phase(StreamPhase.STREAMING)

    async def _stream_attach_live_output_if_available(
        self,
        *,
        wait_for_agent: bool = True,
    ) -> None:
        if self._live_output_attached:
            return
        if self._is_reviewing or not self._is_running:
            return
        if self._live_output_agent is None:
            if not wait_for_agent:
                return
            loop = asyncio.get_running_loop()
            deadline = loop.time() + self._LIVE_ATTACH_TIMEOUT_SECONDS
            while loop.time() < deadline and self._live_output_agent is None:
                with contextlib.suppress(*_SHUTDOWN_ERRORS):
                    await self.ctx.api.reconcile_running_tasks([self._task_model.id])
                runtime_view = self.ctx.api.get_runtime_view(self._task_model.id)
                self._live_output_agent = self._stream_resolve_live_handle(
                    runtime_view, "running_agent"
                )
                if self._live_output_agent is None:
                    await asyncio.sleep(0.1)
            if self._live_output_agent is None:
                if not self._live_output_wait_noted:
                    await self._stream_agent_output_panel().output.post_note(
                        "Waiting for live agent stream...",
                        classes="warning",
                    )
                    self._live_output_wait_noted = True
                return
        panel = self._stream_agent_output_panel()
        self._live_output_attached = True
        self._stream_set_target(self._live_output_agent, self)
        self._live_output_wait_noted = False
        await panel.output.post_note("Connected to live agent stream", classes="info")

    async def _stream_maybe_auto_start_pair_review(self) -> bool:
        if self._read_only or self._live_review_attached or self._review_log_loaded:
            return False
        if self._task_model.task_type != TaskType.PAIR:
            return False
        if self._task_model.status != TaskStatus.REVIEW:
            return False
        if self._phase != StreamPhase.IDLE:
            return False
        if not self.ctx.config.general.auto_review:
            return False
        await self.action_generate_review()
        return True

    async def _stream_load_prior_review(self) -> None:
        """Load auto-review results from execution metadata if available."""
        if self._review_log_loaded:
            return
        execution_id = await self._stream_resolve_execution_id()
        if execution_id is None:
            return
        execution = await self.ctx.api.get_execution(execution_id)
        metadata = self._execution_metadata(execution)
        if not metadata:
            return
        review_result = metadata.get("review_result")
        if review_result is None:
            return

        status = review_result.get("status", "")
        summary = review_result.get("summary", "")

        chat_panel = self._stream_review_output_panel()
        chat_panel.remove_class("hidden")
        output = chat_panel.output

        if status == "approved":
            await output.post_note("Auto-review passed", classes="success")
        else:
            await output.post_note("Auto-review flagged issues", classes="warning")

        if summary:
            await output.post_response(summary)

        if status == "approved":
            self._state_set_decision("approved")
        elif status == "rejected":
            self._state_set_decision("rejected")

        self._review_log_loaded = True
        self._state_set_phase(StreamPhase.COMPLETE)

    def _stream_reset_agents(self) -> None:
        if self._agent is not None:
            self._agent = None
        if self._live_output_attached and self._agent is None:
            self._live_output_attached = False
        if self._live_review_attached and self._agent is None:
            self._live_review_attached = False

    async def _prompt_generate_review(self, output: StreamingOutput) -> None:
        """Spawn agent to generate code review."""
        from kagan.core.debug_log import log

        wt_path = await self.ctx.api.get_workspace_path(self._task_model.id)
        if not wt_path:
            await output.post_note("Error: Worktree not found", classes="error")
            self._state_set_phase(StreamPhase.IDLE)
            return

        diff = self._diff_text or await self.ctx.api.get_workspace_diff(
            self._task_model.id,
            base_branch=self._base_branch,
        )
        if not diff:
            await output.post_note("No diff to review", classes="info")
            self._state_set_phase(StreamPhase.IDLE)
            return

        queued_follow_up = await self._queue_take_review_follow_up()

        self._agent = self._agent_factory(wt_path, self._agent_config, read_only=True)
        self._agent.start(self)

        await output.post_note("Analyzing changes...", classes="info")
        log.info("[ReviewModal] Agent started, waiting for response")

        try:
            await self._agent.wait_ready(timeout=AGENT_TIMEOUT)
        except TimeoutError:
            await output.post_note("Review failed: agent startup timed out", classes="error")
            self._state_set_phase(StreamPhase.IDLE)
            return

        review_prompt = build_review_prompt(self._task_model.title, diff, queued_follow_up)
        self._prompt_worker = self.run_worker(
            self._prompt_run(review_prompt, output),
            group="review-run-prompt",
            exclusive=True,
            exit_on_error=False,
        )

    async def _prompt_run(self, prompt: str, output: StreamingOutput) -> None:
        """Run prompt in background, handle errors."""
        if self._agent is None:
            return
        try:
            await self._agent.send_prompt(prompt)
        except RequestError as exc:
            await output.post_note(f"Review failed: {exc}", classes="error")
            self._state_set_phase(StreamPhase.IDLE)

    async def _stream_finish_cycle(self) -> None:
        await asyncio.gather(
            self._queue_refresh_review_state(),
            self._queue_refresh_implementation_state(),
        )
        await self._refresh_runtime_state()

    async def _stream_on_update(self, message: messages.AgentUpdate) -> None:
        if self._phase == StreamPhase.THINKING:
            self._state_set_phase(StreamPhase.STREAMING)
        output = self._stream_target_output()
        await output.post_response(message.text)

    async def _stream_on_complete(self, _: messages.AgentComplete) -> None:
        self._stream_reset_agents()
        self._state_sync_decision_from_output()
        self._state_set_phase(StreamPhase.COMPLETE)
        await self._stream_finish_cycle()
        if not self._read_only and self._review_queue_pending:
            await self._queue_start_review_follow_up_if_needed()

    async def _stream_on_fail(self, message: messages.AgentFail) -> None:
        output = self._stream_target_output()
        exit_code = parse_agent_exit_code(message.message)
        if exit_code == -15:
            await output.post_note(
                "Agent stream ended by cancellation (SIGTERM).",
                classes="dismissed",
            )
            self._state_sync_decision_from_output()
            if extract_review_decision(output.get_text_content()):
                self._state_set_phase(StreamPhase.COMPLETE)
            else:
                self._state_set_phase(StreamPhase.IDLE)
            self._stream_reset_agents()
            await self._stream_finish_cycle()
            return

        await output.post_note(f"Error: {message.message}", classes="error")
        self._stream_reset_agents()
        self._state_set_phase(StreamPhase.IDLE)
        await self._stream_finish_cycle()

    # ------------------------------------------------------------------
    # Queue management
    # ------------------------------------------------------------------

    async def _queue_configure_agent_output_chat(self) -> None:
        panel = self._stream_agent_output_panel()
        panel.set_send_handler(self._queue_send_implementation_follow_up)
        panel.set_get_queued_handler(self._queue_get_implementation_messages)
        panel.set_remove_handler(self._queue_remove_implementation_message)
        await panel.refresh_queued_messages()
        self._queue_sync_agent_output_visibility()

    def _queue_sync_agent_output_visibility(self) -> None:
        enabled = self._task_model.task_type == TaskType.AUTO and (
            self._task_model.status == TaskStatus.IN_PROGRESS
        )
        panel = self._stream_agent_output_panel()
        if enabled:
            panel.remove_class("queue-disabled")
        else:
            panel.add_class("queue-disabled")

    async def _queue_configure_review_chat(self) -> None:
        panel = self._stream_review_output_panel()
        panel.set_send_handler(self._queue_send_review_follow_up)
        panel.set_get_queued_handler(self._queue_get_review_messages)
        panel.set_remove_handler(self._queue_remove_review_message)
        await panel.refresh_queued_messages()
        self._queue_sync_review_visibility()

    def _queue_sync_review_visibility(self) -> None:
        enabled = self._task_model.status == TaskStatus.REVIEW and not self._read_only
        panel = self._stream_review_output_panel()
        if enabled:
            panel.remove_class("queue-disabled")
        else:
            panel.add_class("queue-disabled")

    async def _queue_refresh_review_state(self) -> None:
        if not self.is_mounted:
            return
        self._queue_sync_review_visibility()
        await self._stream_review_output_panel().refresh_queued_messages()
        if self._task_model.status != TaskStatus.REVIEW:
            self._review_queue_pending = False
            self._state_sync_review_actions()
            return
        try:
            status = await self.ctx.api.get_queue_status(self._task_model.id, lane="review")
        except RuntimeError:
            self._review_queue_pending = False
            self._state_sync_review_actions()
            return
        self._review_queue_pending = status.has_queued
        self._state_sync_review_actions()

    async def _queue_refresh_implementation_state(self) -> None:
        if not self.is_mounted:
            return
        self._queue_sync_agent_output_visibility()
        await self._stream_agent_output_panel().refresh_queued_messages()
        if self._task_model.status != TaskStatus.IN_PROGRESS:
            self._implementation_queue_pending = False
            return
        try:
            status = await self.ctx.api.get_queue_status(self._task_model.id, lane="implementation")
        except RuntimeError:
            self._implementation_queue_pending = False
            return
        self._implementation_queue_pending = status.has_queued

    async def _queue_send_review_follow_up(self, content: str) -> None:
        if self._task_model.status != TaskStatus.REVIEW:
            raise RuntimeError("Review queue only available in REVIEW status")
        await self.ctx.api.queue_message(self._task_model.id, content, lane="review")
        await self._queue_refresh_review_state()
        if self._phase == StreamPhase.IDLE and not self._live_review_attached:
            await self._queue_start_review_follow_up_if_needed()

    async def _queue_get_review_messages(self) -> list:
        try:
            return await self.ctx.api.get_queued_messages(self._task_model.id, lane="review")
        except RuntimeError:
            return []

    async def _queue_remove_review_message(self, index: int) -> bool:
        removed = await self.ctx.api.remove_queued_message(
            self._task_model.id,
            index,
            lane="review",
        )
        await self._queue_refresh_review_state()
        return removed

    async def _queue_take_review_follow_up(self) -> str | None:
        try:
            queued = await self.ctx.api.take_queued_message(self._task_model.id, lane="review")
        except RuntimeError:
            return None
        await self._queue_refresh_review_state()
        if queued is None:
            return None
        return truncate_queue_payload(queued.content)

    async def _queue_send_implementation_follow_up(self, content: str) -> None:
        if self._task_model.status != TaskStatus.IN_PROGRESS:
            raise RuntimeError("Implementation queue only available in IN_PROGRESS")
        await self.ctx.api.queue_message(self._task_model.id, content, lane="implementation")
        await self._queue_refresh_implementation_state()

        submitted = await self.ctx.api.submit_job(self._task_model.id, "start_agent")
        terminal = await self._actions_wait_for_job_terminal(
            submitted.job_id, task_id=self._task_model.id
        )
        pending_msg = "Queued follow-up accepted. Agent start requested; waiting for scheduler."
        msg, severity = self._actions_job_result_message(
            terminal,
            failure_msg="Failed to start next implementation run.",
            success_msg="Starting next implementation run...",
            pending_msg=pending_msg,
        )
        if severity == "information" and msg != pending_msg:
            msg = "Queued follow-up accepted. " + msg
        output = self._stream_agent_output_panel().output
        await output.post_note(msg, classes="warning" if severity == "warning" else "info")
        await self._refresh_runtime_state()

    async def _queue_get_implementation_messages(self) -> list:
        try:
            return await self.ctx.api.get_queued_messages(
                self._task_model.id,
                lane="implementation",
            )
        except RuntimeError:
            return []

    async def _queue_remove_implementation_message(self, index: int) -> bool:
        removed = await self.ctx.api.remove_queued_message(
            self._task_model.id,
            index,
            lane="implementation",
        )
        await self._queue_refresh_implementation_state()
        return removed

    async def _queue_start_review_follow_up_if_needed(self) -> None:
        if self._phase not in (StreamPhase.IDLE, StreamPhase.COMPLETE):
            return
        try:
            status = await self.ctx.api.get_queue_status(self._task_model.id, lane="review")
        except RuntimeError:
            return
        if not status.has_queued:
            return
        chat_panel = self._stream_review_output_panel()
        chat_panel.remove_class("hidden")
        await chat_panel.output.post_note("Starting queued review follow-up...", classes="info")
        self._state_set_decision(None)
        self._state_set_phase(StreamPhase.THINKING)
        await self._prompt_generate_review(chat_panel.output)

    # ------------------------------------------------------------------
    # Actions
    # ------------------------------------------------------------------

    async def _actions_wait_for_job_terminal(
        self,
        job_id: str,
        *,
        task_id: str,
        timeout_seconds: float = 0.6,
    ) -> JobRecord | None:
        try:
            return await self.ctx.api.wait_job(
                job_id,
                task_id=task_id,
                timeout_seconds=timeout_seconds,
            )
        except asyncio.CancelledError:
            with contextlib.suppress(Exception):  # quality-allow-broad-except
                await self.ctx.api.cancel_job(job_id, task_id=task_id)
            raise

    @staticmethod
    def _actions_job_result_payload(
        record: JobRecord | None,
    ) -> dict[str, Any] | None:
        if record is None or not isinstance(record.result, dict):
            return None
        return record.result

    @classmethod
    def _actions_job_message(cls, record: JobRecord | None, default: str) -> str:
        payload = cls._actions_job_result_payload(record)
        if payload is not None:
            payload_message = payload.get("message")
            if isinstance(payload_message, str) and payload_message.strip():
                return payload_message
        if record is not None and record.message:
            return record.message
        return default

    def _actions_job_result_message(
        self,
        terminal: JobRecord | None,
        *,
        failure_msg: str,
        success_msg: str,
        pending_msg: str,
    ) -> tuple[str, Literal["warning", "information"]]:
        """Return (message, severity) for job result."""
        payload = self._actions_job_result_payload(terminal)
        if payload is not None and not bool(payload.get("success", False)):
            return (
                self._actions_job_message(terminal, failure_msg),
                "warning",
            )
        if terminal is not None and terminal.status in {
            JobStatus.FAILED,
            JobStatus.CANCELLED,
        }:
            return (
                self._actions_job_message(terminal, failure_msg),
                "warning",
            )
        if payload is None:
            return (pending_msg, "information")
        return (
            self._actions_job_message(terminal, success_msg),
            "information",
        )

    def action_show_summary(self) -> None:
        """Switch to the summary tab."""
        self._actions_set_active_tab("review-summary")

    def action_show_diff(self) -> None:
        """Switch to the diff tab."""
        self._actions_set_active_tab("review-diff")

    def action_show_ai_review(self) -> None:
        """Switch to the review output tab."""
        self._actions_set_active_tab("review-ai")

    def action_show_agent_output(self) -> None:
        """Switch to the agent output tab."""
        self._actions_set_active_tab("review-agent-output")

    def _actions_set_active_tab(self, tab_id: str) -> None:
        tabs = self.query_one("#review-tabs", TabbedContent)
        tabs.active = tab_id

    @staticmethod
    def _actions_parse_diff_modal_result(
        result: object,
    ) -> DiffModalResult | None:
        if result == DIFF_MODAL_APPROVE_RESULT:
            return DIFF_MODAL_APPROVE_RESULT
        if result == DIFF_MODAL_REJECT_RESULT:
            return DIFF_MODAL_REJECT_RESULT
        return None

    def _diff_populate_commits(self, commits: list[str]) -> None:
        table = self.query_one("#commits-table", DataTable)
        table.clear()
        if not table.columns:
            table.add_columns("Repo", "Hash", "Message")
        table.cursor_type = "row"
        table.zebra_stripes = True

        if not commits:
            table.add_row("—", "—", "No commits found")
            return

        for line in commits:
            repo, sha, message = self._diff_parse_commit_line(line)
            table.add_row(repo or "—", sha or "—", message or "—")

    async def _diff_populate_pane(self, workspaces: list) -> None:
        if workspaces:
            try:
                diffs = await self.ctx.api.get_all_diffs(workspaces[0].id)
            except RuntimeError:
                diffs = None
            if diffs is not None:
                self._diff_populate_file_table(diffs)
                total_additions = sum(diff.total_additions for diff in diffs)
                total_deletions = sum(diff.total_deletions for diff in diffs)
                total_files = sum(len(diff.files) for diff in diffs)
                self._diff_set_stats(total_additions, total_deletions, total_files)
                return

        total_additions, total_deletions, total_files = self._diff_parse_totals(self._diff_stats)
        self._diff_set_stats(total_additions, total_deletions, total_files)
        self._diff_render_text(self._diff_text)

    def _diff_populate_file_table(self, diffs: list[RepoDiff]) -> None:
        table = self.query_one("#diff-files", DataTable)
        table.clear()
        if not table.columns:
            table.add_columns("File", "+", "-")
        table.cursor_type = "row"
        table.zebra_stripes = True

        self._file_diffs.clear()
        multi_repo = len(diffs) > 1
        for diff in diffs:
            for file in diff.files:
                key = f"{diff.repo_name}:{file.path}"
                self._file_diffs[key] = file
                label = f"{diff.repo_name}/{file.path}" if multi_repo else file.path
                table.add_row(label, str(file.additions), str(file.deletions), key=key)

        if not self._file_diffs:
            table.add_class("hidden")
            self._diff_render_text(self._diff_text)
            return

        table.remove_class("hidden")
        first_key = next(iter(self._file_diffs))
        self._diff_show_file(first_key)

    def _diff_show_file(self, key: str) -> None:
        file_diff = self._file_diffs.get(key)
        if file_diff is None:
            self._diff_render_text(self._diff_text)
            return
        self._diff_render_text(file_diff.diff_content)

    def _diff_render_text(self, diff_text: str) -> None:
        diff_log = self.query_one("#diff-log", RichLog)
        diff_log.clear()
        for line in diff_text.splitlines() or ["(No diff available)"]:
            diff_log.write(colorize_diff_line(line))
        diff_log.scroll_home(animate=False, immediate=True)

    def _diff_set_stats(self, additions: int, deletions: int, files: int) -> None:
        self.query_one("#review-stats", Horizontal).remove_class("hidden")
        self.query_one("#stat-additions", Static).update(f"+ {additions} Additions")
        self.query_one("#stat-deletions", Static).update(f"- {deletions} Deletions")
        self.query_one("#stat-files", Static).update(f"{files} Files Changed")

    def _diff_parse_totals(self, diff_stats: str) -> tuple[int, int, int]:
        if not diff_stats:
            return 0, 0, 0
        pattern = re.compile(r"\+(\d+)\s+-(\d+)\s+\((\d+)\s+file")
        total_line = ""
        for line in diff_stats.splitlines():
            if line.lower().startswith("total:"):
                total_line = line
                break
        if total_line:
            match = pattern.search(total_line)
            if match:
                return (
                    int(match.group(1)),
                    int(match.group(2)),
                    int(match.group(3)),
                )
        additions = deletions = files = 0
        for line in diff_stats.splitlines():
            match = pattern.search(line)
            if match:
                additions += int(match.group(1))
                deletions += int(match.group(2))
                files += int(match.group(3))
        return additions, deletions, files

    def _diff_parse_commit_line(self, line: str) -> tuple[str, str, str]:
        repo = ""
        rest = line.strip()
        if rest.startswith("["):
            end = rest.find("]")
            if end != -1:
                repo = rest[1:end]
                rest = rest[end + 1 :].strip()
        parts = rest.split(" ", 1)
        sha = parts[0] if parts else ""
        message = parts[1] if len(parts) > 1 else ""
        return repo, sha, message

    async def _diff_open_modal(self) -> None:
        from kagan.tui.ui.modals import DiffModal

        workspaces = await self.ctx.api.list_workspaces(task_id=self._task_model.id)
        title = (
            f"Diff: {self._task_model.short_id} {self._task_model.title[:MODAL_TITLE_MAX_LENGTH]}"
        )

        if not workspaces:
            diff_text = self._diff_text or await self.ctx.api.get_workspace_diff(
                self._task_model.id,
                base_branch=self._base_branch,
            )
            result = await self.app.push_screen(
                DiffModal(title=title, diff_text=diff_text, task=self._task_model)
            )
        else:
            try:
                diffs = await self.ctx.api.get_all_diffs(workspaces[0].id)
            except RuntimeError:
                diff_text = self._diff_text or await self.ctx.api.get_workspace_diff(
                    self._task_model.id,
                    base_branch=self._base_branch,
                )
                result = await self.app.push_screen(
                    DiffModal(title=title, diff_text=diff_text, task=self._task_model)
                )
            else:
                result = await self.app.push_screen(
                    DiffModal(title=title, diffs=diffs, task=self._task_model)
                )

        modal_result = self._actions_parse_diff_modal_result(result)
        if modal_result == DIFF_MODAL_APPROVE_RESULT:
            self.action_approve()
        elif modal_result == DIFF_MODAL_REJECT_RESULT:
            self.action_reject()

    async def action_attach_session(self) -> None:
        """Attach to the running PAIR session."""
        if self._task_model.task_type != TaskType.PAIR:
            return
        if not await self.ctx.api.session_exists(self._task_model.id):
            self.notify("No active session for this task", severity="warning")
            return
        with self.app.suspend():
            await self.ctx.api.attach_session(self._task_model.id)

    async def action_generate_review(self) -> None:
        """Generate or regenerate AI review."""
        from kagan.core.debug_log import log

        if self._read_only:
            self.notify("Read-only history view", severity="warning")
            return
        if self._live_review_attached:
            self.notify("Review is already running", severity="information")
            return

        log.info(f"[ReviewModal] Starting AI review (phase={self._phase})")

        if self._phase == StreamPhase.COMPLETE:
            await self.action_regenerate_review()
            return
        if self._phase != StreamPhase.IDLE:
            return

        self._state_set_decision(None)
        self._state_set_phase(StreamPhase.THINKING)
        chat_panel = self._stream_review_output_panel()
        chat_panel.remove_class("hidden")
        output = chat_panel.output
        await self._prompt_generate_review(output)

    async def action_regenerate_review(self) -> None:
        """Regenerate AI review."""
        if self._phase != StreamPhase.COMPLETE:
            return

        if self._agent:
            await self._agent.stop()
            self._agent = None

        output = self._stream_review_output_panel().output
        await output.clear()
        self._state_set_decision(None)
        self._state_set_phase(StreamPhase.THINKING)
        await self._prompt_generate_review(output)

    async def action_cancel_review(self) -> None:
        if self._live_review_attached and self._agent is None:
            self.notify("Review is managed by automation", severity="warning")
            return
        if self._phase not in (StreamPhase.THINKING, StreamPhase.STREAMING):
            return

        if self._prompt_worker is not None and not self._prompt_worker.is_finished:
            self._prompt_worker.cancel()
        if self._agent:
            await self._agent.stop()
            self._agent = None

        output = self._stream_review_output_panel().output
        await output.post_note("Review cancelled", classes="dismissed")
        self._state_set_phase(StreamPhase.IDLE)

    async def action_view_diff(self) -> None:
        """Open the diff modal for the current task."""
        await self._diff_open_modal()

    async def action_start_agent_output(self) -> None:
        """Start AUTO execution from Task Output > Agent Output tab."""
        latest = await self.ctx.api.get_task(self._task_model.id)
        if latest is None:
            self.notify("Task no longer exists", severity="error")
            return
        self._task_model = latest
        if latest.task_type != TaskType.AUTO or latest.status != TaskStatus.IN_PROGRESS:
            self.notify(
                "Start is available only for AUTO tasks in IN_PROGRESS",
                severity="warning",
            )
            return
        if self._is_running:
            self.notify("Agent is already running", severity="information")
            return

        submitted = await self.ctx.api.submit_job(latest.id, "start_agent")
        terminal = await self._actions_wait_for_job_terminal(submitted.job_id, task_id=latest.id)
        msg, severity = self._actions_job_result_message(
            terminal,
            failure_msg="Failed to start agent",
            success_msg="Agent start requested",
            pending_msg=self.START_JOB_PENDING_MESSAGE,
        )
        self.notify(msg, severity=severity)
        await self._refresh_runtime_state()

    async def action_stop_agent_output(self) -> None:
        """Stop AUTO execution from Task Output > Agent Output tab."""
        latest = await self.ctx.api.get_task(self._task_model.id)
        if latest is None:
            self.notify("Task no longer exists", severity="error")
            return
        self._task_model = latest
        if latest.task_type != TaskType.AUTO or latest.status != TaskStatus.IN_PROGRESS:
            self.notify(
                "Stop is available only for AUTO tasks in IN_PROGRESS",
                severity="warning",
            )
            return

        submitted = await self.ctx.api.submit_job(latest.id, "stop_agent")
        terminal = await self._actions_wait_for_job_terminal(submitted.job_id, task_id=latest.id)
        msg, severity = self._actions_job_result_message(
            terminal,
            failure_msg="No running agent to stop",
            success_msg="Agent stop requested",
            pending_msg=self.STOP_JOB_PENDING_MESSAGE,
        )
        self.notify(msg, severity=severity)
        await self._refresh_runtime_state()

    async def action_rebase(self) -> None:
        """Rebase the task branch onto the base branch."""
        if self._read_only:
            self.notify("Read-only history view", severity="warning")
            return
        self.notify("Rebasing...", severity="information")
        success, message, conflict_files = await self.ctx.api.rebase_workspace(
            self._task_model.id, self._base_branch
        )
        if success:
            self._diff_text = await self.ctx.api.get_workspace_diff(
                self._task_model.id,
                base_branch=self._base_branch,
            )
            self._diff_render_text(self._diff_text)
            diff_stats = await self.ctx.api.get_workspace_diff_stats(
                self._task_model.id,
                base_branch=self._base_branch,
            )
            self.query_one("#diff-stats", Static).update(diff_stats or "[dim](No changes)[/dim]")
            self.notify("Rebase successful", severity="information")
        elif conflict_files:
            self.dismiss("rebase_conflict")
        else:
            self.notify(f"Rebase failed: {message}", severity="error")

    def action_approve(self) -> None:
        """Approve the review."""
        if self._read_only:
            self.notify("Read-only history view", severity="warning")
            return
        if self._phase in (StreamPhase.THINKING, StreamPhase.STREAMING):
            self.notify(
                "Wait for review to complete before approval",
                severity="warning",
            )
            return
        if self._review_queue_pending:
            self.notify(
                "Process queued review messages before approval",
                severity="warning",
            )
            return
        if self._no_changes:
            self.dismiss("exploratory")
        else:
            self.dismiss("approve")

    def action_reject(self) -> None:
        """Reject the review."""
        if self._read_only:
            self.notify("Read-only history view", severity="warning")
            return
        self.dismiss("reject")

    async def action_close_or_cancel(self) -> None:
        """Cancel review if in progress, otherwise close."""
        if self._phase in (StreamPhase.THINKING, StreamPhase.STREAMING):
            if self._agent is None and (self._live_review_attached or self._live_output_attached):
                self.dismiss(None)
                return
            await self.action_cancel_review()
        else:
            self.dismiss(None)

    def action_copy(self) -> None:
        """Copy review content to clipboard."""
        output = self._stream_review_output_panel().output
        review_text = output._agent_response._markdown if output._agent_response else ""

        content_parts = [f"# Review: {self._task_model.title}"]
        if self._diff_stats:
            content_parts.append(f"\n## Changes\n{self._diff_stats}")
        if review_text:
            content_parts.append(f"\n## AI Review\n{review_text}")

        copy_with_notification(self.app, "\n".join(content_parts), "Review")
