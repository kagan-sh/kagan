"""Planner screen for chat-first task creation."""

from __future__ import annotations

import asyncio
from collections.abc import Awaitable, Callable
from contextlib import suppress
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, ClassVar, cast

from sqlalchemy.exc import OperationalError
from textual import events, getters, on
from textual.binding import Binding, BindingType
from textual.containers import Center, Vertical
from textual.css.query import NoMatches
from textual.message import Message
from textual.reactive import var
from textual.widgets import Footer, Static, TextArea

from kagan.core.acp import messages as _acp_messages
from kagan.core.adapters.db.repositories.base import RepositoryClosing
from kagan.core.agents.agent_factory import AgentFactory, create_agent
from kagan.core.agents.planner import build_planner_prompt, parse_proposed_plan
from kagan.core.config import get_fallback_agent_config
from kagan.core.constants import BOX_DRAWING, KAGAN_LOGO, PLANNER_TITLE_MAX_LENGTH
from kagan.core.git_utils import get_current_branch
from kagan.core.limits import AGENT_TIMEOUT
from kagan.core.models.enums import ChatRole, ProposalStatus
from kagan.core.services.permission_policy import AgentPermissionScope, resolve_auto_approve
from kagan.core.time import utc_now
from kagan.core.utils import truncate_queue_payload
from kagan.tui.keybindings import PLANNER_BINDINGS
from kagan.tui.ui.screen_result import await_screen_result
from kagan.tui.ui.screens.base import KaganScreen
from kagan.tui.ui.screens.planner.commands import PlannerCommandProvider
from kagan.tui.ui.screens.planner.state import (
    ChatMessage,
    NoteInfo,
    PersistentPlannerState,
    PlannerEvent,
    PlannerPhase,
    PlannerState,
)
from kagan.tui.ui.utils.agent_stream_router import AgentStreamRouter
from kagan.tui.ui.utils.slash_registry import SlashCommandRegistry, parse_slash_command_call
from kagan.tui.ui.widgets import StatusBar, StreamingOutput
from kagan.tui.ui.widgets.chat_panel import QueuedMessageRow, QueuedMessagesContainer
from kagan.tui.ui.widgets.header import KaganHeader
from kagan.tui.ui.widgets.offline_banner import OfflineBanner
from kagan.tui.ui.widgets.plan_approval import PlanApprovalWidget
from kagan.tui.ui.widgets.slash_complete import SlashComplete

if TYPE_CHECKING:
    from collections.abc import Sequence

    from acp.schema import AvailableCommand
    from textual.app import ComposeResult
    from textual.message_pump import MessagePump
    from textual.timer import Timer

    from kagan.core.acp import messages
    from kagan.core.adapters.db.schema import Task
    from kagan.tui.ui.utils.slash_registry import SlashCommand


MIN_INPUT_HEIGHT = 1
MAX_INPUT_HEIGHT = 6
PLANNER_HEADER_SYNC_INTERVAL_SECONDS = 1.0
BRANCH_LOOKUP_TIMEOUT_SECONDS = 1.0
PLANNER_QUEUE_TRUNCATION_PREFIX = "[queued planner context truncated]\n"
PLANNER_QUEUE_TRUNCATION_MAX_CHARS = 6000

type PlannerSlashHandler = Callable[[str], None | Awaitable[None]]

PLANNER_EXAMPLES = [
    '"Add user authentication with OAuth2"',
    '"Refactor the payment module for better testing"',
    '"Fix the bug where users can\'t upload images"',
]


class PlannerEmptyState(Vertical):
    """Empty state widget for planner screen with branded design."""

    DEFAULT_CLASSES = "planner-empty-state"

    def compose(self) -> ComposeResult:
        with Center():
            with Vertical(classes="planner-empty-content"):
                with Vertical(classes="planner-empty-card"):
                    with Center():
                        yield Static(
                            KAGAN_LOGO,
                            id="planner-empty-logo",
                            classes="planner-empty-logo",
                        )

                    yield Static(
                        "🎯 Plan Your Work",
                        id="planner-empty-heading",
                        classes="planner-empty-heading",
                    )

                    yield Static(
                        "Describe what you want to build or accomplish.\n"
                        "Kagan will help break it down into actionable tasks.",
                        id="planner-empty-description",
                        classes="planner-empty-description",
                    )

                    with Vertical(
                        id="planner-empty-examples",
                        classes="planner-empty-examples",
                    ):
                        yield Static("Examples:", classes="planner-empty-section-title")
                        for example in PLANNER_EXAMPLES:
                            yield Static(
                                f"  {BOX_DRAWING['BULLET']} {example}",
                                classes="planner-empty-example",
                            )


class PlannerInput(TextArea):
    """TextArea that submits on Enter (Shift+Enter/Ctrl+J for newline)."""

    BINDINGS: ClassVar[list[BindingType]] = [
        Binding(
            "shift+enter,ctrl+j",
            "insert_newline",
            "New Line",
            show=False,
            priority=True,
        ),
    ]

    @dataclass
    class SubmitRequested(Message):
        """Request planner prompt submission."""

        text: str

    @dataclass
    class SlashKey(Message):
        """Forward slash-navigation key to slash complete."""

        key: str

    def action_insert_newline(self) -> None:
        """Insert a newline character."""
        self.insert("\n")

    async def _on_key(self, event: events.Key) -> None:
        if self.text.startswith("/"):
            if event.key in ("up", "down"):
                event.prevent_default()
                event.stop()
                self.post_message(self.SlashKey(event.key))
                return
            if event.key == "enter":
                event.prevent_default()
                event.stop()
                self.post_message(self.SlashKey("enter"))
                return
            if event.key == "escape":
                event.prevent_default()
                event.stop()
                self.post_message(self.SlashKey("escape"))
                return

        if event.key == "enter":
            event.prevent_default()
            event.stop()
            self.post_message(self.SubmitRequested(self.text))
            return
        await super()._on_key(event)


class PlannerScreen(KaganScreen):
    """Chat-first planner for creating tasks."""

    COMMANDS = {PlannerCommandProvider}
    BINDINGS = PLANNER_BINDINGS
    planner_status: var[str] = var("waiting", init=False)
    planner_hint: var[str] = var("Initializing agent...", init=False)
    slash_commands: var[list[SlashCommand[PlannerSlashHandler]]] = var(list, init=False)
    header = getters.query_one(KaganHeader)

    def __init__(self, agent_factory: AgentFactory = create_agent, **kwargs) -> None:
        super().__init__(**kwargs)
        self._state = PlannerState()
        self._agent_factory = agent_factory

        self._current_mode: str = ""
        self._available_modes: dict[str, messages.Mode] = {}
        self._available_commands: list[AvailableCommand] = []

        self._clearing: bool = False
        self._slash_complete: SlashComplete | None = None
        self._slash_registry: SlashCommandRegistry[PlannerSlashHandler] = SlashCommandRegistry()
        self._register_slash_commands()
        self.set_reactive(
            PlannerScreen.slash_commands,
            self._slash_registry.list_commands(),
        )
        self._planner_queue_pending = False
        self._pending_proposal_id: str | None = None
        self._header_sync_timer: Timer | None = None
        self._agent_stream = AgentStreamRouter(
            get_output=self._get_output,
            show_output=self._show_output,
            on_update=self._handle_agent_update,
            on_thinking=self._handle_thinking,
            on_ready=self._handle_agent_ready,
            on_fail=self._handle_agent_fail,
            on_request_permission=self._handle_request_permission,
            on_set_modes=self._handle_set_modes,
            on_mode_update=self._handle_mode_update,
            on_commands_update=self._handle_commands_update,
            ignore_fail=lambda _message: self._clearing,
        )

    # ------------------------------------------------------------------
    # Compose & event wiring
    # ------------------------------------------------------------------

    def compose(self) -> ComposeResult:
        yield KaganHeader()
        with Vertical(id="planner-container"):
            yield Static("Plan Mode", id="planner-header")
            yield PlannerEmptyState()
            yield StreamingOutput(id="planner-output")
            with Vertical(id="planner-bottom"):
                yield StatusBar().data_bind(
                    status=PlannerScreen.planner_status,
                    hint=PlannerScreen.planner_hint,
                )
                yield QueuedMessagesContainer(
                    id="planner-queued-messages",
                    classes="queued-messages-container planner-queued-messages",
                )
                yield PlannerInput(
                    "",
                    id="planner-input",
                    show_line_numbers=False,
                    placeholder=("Describe your task... (/ for commands, Ctrl+C to clear or stop)"),
                )
        yield Footer(show_command_palette=False)

    @on(PlannerInput.SubmitRequested)
    async def on_submit_requested(self, event: PlannerInput.SubmitRequested) -> None:
        """Handle planner input submit messages."""
        await self._handle_submit_requested(event)

    @on(PlannerInput.SlashKey)
    async def on_slash_key(self, event: PlannerInput.SlashKey) -> None:
        """Handle slash-complete navigation key messages."""
        await self._handle_slash_key(event)

    @on(OfflineBanner.Reconnect)
    async def on_offline_banner_reconnect(self, event: OfflineBanner.Reconnect) -> None:
        """Handle reconnect requests from the offline banner."""
        await self._handle_offline_reconnect(event)

    @on(_acp_messages.AgentMessage)
    async def on_agent_message(self, message: _acp_messages.AgentMessage) -> None:
        """Route ACP agent messages into planner stream handlers."""
        await self._handle_agent_message(message)

    @on(TextArea.Changed, "#planner-input")
    async def on_planner_input_changed(self, event: TextArea.Changed) -> None:
        """Handle planner input text changes."""
        await self._handle_planner_input_changed(event)

    @on(SlashComplete.Completed)
    async def on_slash_completed(self, event: SlashComplete.Completed) -> None:
        """Handle slash command selection from completion UI."""
        await self._handle_slash_completed(event)

    @on(SlashComplete.Dismissed)
    async def on_slash_dismissed(self, event: SlashComplete.Dismissed) -> None:
        """Handle slash completion dismissal."""
        await self._handle_slash_dismissed(event)

    @on(PlanApprovalWidget.Approved)
    async def on_plan_approved(self, event: PlanApprovalWidget.Approved) -> None:
        """Handle plan approval from plan approval widget."""
        await self._handle_plan_approved(event)

    @on(PlanApprovalWidget.Dismissed)
    async def on_plan_dismissed(self, event: PlanApprovalWidget.Dismissed) -> None:
        """Handle plan dismissal from plan approval widget."""
        await self._handle_plan_dismissed(event)

    @on(PlanApprovalWidget.EditRequested)
    async def on_plan_edit_requested(self, event: PlanApprovalWidget.EditRequested) -> None:
        """Handle edit requests from plan approval widget."""
        await self._handle_plan_edit_requested(event)

    @on(QueuedMessageRow.RemoveRequested)
    async def on_queue_remove_requested(self, event: QueuedMessageRow.RemoveRequested) -> None:
        """Handle queued message removal from planner queue list."""
        await self._handle_queue_remove_requested(event)

    # ------------------------------------------------------------------
    # Lifecycle
    # ------------------------------------------------------------------

    async def on_mount(self) -> None:
        """Mount planner widgets and initialize agent/session state."""
        await self.sync_header_context(self.header)
        self.kagan_app.task_changed_signal.subscribe(self, self._on_task_changed)
        self._start_header_sync()
        with suppress(NoMatches):
            self.query_one("#planner-queued-messages", QueuedMessagesContainer).display = False

        if self.ctx.active_repo_id is None:
            banner = OfflineBanner(message="Select a repository to start planning")
            container = self.query_one("#planner-container", Vertical)
            await container.mount(banner, before=0)
            self._disable_input()
            self._update_status("offline", "No repository selected")
            await self._discard_persistent_state(self.kagan_app.planner_state)
            return

        branch = await get_current_branch(self.kagan_app.project_root)
        self.header.update_branch(branch)

        if not self.ctx.api.is_agent_available():
            message = self.ctx.api.get_agent_status_message()
            banner = OfflineBanner(message=message or "Planner requires an active agent")
            container = self.query_one("#planner-container", Vertical)
            await container.mount(banner, before=0)
            self._disable_input()
            self._update_status("offline", "Agent unavailable")
            return

        self._update_status("initializing", "Initializing agent...")
        self._disable_input()

        persistent_state = self.kagan_app.planner_state
        if persistent_state is not None and self._is_persistent_state_compatible(persistent_state):
            await self._restore_state(persistent_state)
        else:
            await self._discard_persistent_state(persistent_state)
            await self._start_planner()

        if not self._state.has_pending_plan:
            await self._load_pending_proposals()

        self._focus_input()
        await self._refresh_planner_queue_messages()

    async def on_screen_suspend(self) -> None:
        """Stop background header synchronization when screen is suspended."""
        self._stop_header_sync()

    async def on_screen_resume(self) -> None:
        """Resume header synchronization when screen becomes active again."""
        self._start_header_sync()
        self._schedule_header_refresh()

    async def on_unmount(self) -> None:
        """Persist planner session state and detach resources."""
        self._stop_header_sync()
        with suppress(Exception):
            self.kagan_app.task_changed_signal.unsubscribe(self)
        input_text = ""
        with suppress(NoMatches):
            planner_input = self.query_one("#planner-input", PlannerInput)
            input_text = planner_input.text

        if self._state.agent:
            self._state.agent.set_message_target(None)

        self.kagan_app.planner_state = self._persistent_state(input_text)

    def _persistent_state(self, input_text: str) -> PersistentPlannerState:
        """Return the persistent planner state snapshot for the current screen."""
        return PersistentPlannerState(
            conversation_history=self._state.conversation_history,
            pending_plan=self._state.pending_plan,
            input_text=input_text,
            active_repo_id=self.ctx.active_repo_id,
            project_root=str(self.kagan_app.project_root),
            agent=self._state.agent,
            refiner=self._state.refiner,
            is_running=self._state.agent is not None,
            agent_ready=self._state.agent_ready,
        )

    def _start_header_sync(self) -> None:
        """Start periodic header refresh polling if not already running."""
        if self._header_sync_timer is not None:
            return
        self._header_sync_timer = self.set_interval(
            PLANNER_HEADER_SYNC_INTERVAL_SECONDS,
            self._schedule_header_refresh,
        )

    def _stop_header_sync(self) -> None:
        """Stop periodic header refresh polling if active."""
        if self._header_sync_timer is None:
            return
        self._header_sync_timer.stop()
        self._header_sync_timer = None

    def _schedule_header_refresh(self) -> None:
        """Schedule a background worker to refresh header task counts."""
        if not self.is_mounted or not self.app.is_running:
            return
        self.run_worker(
            self._refresh_header_task_count,
            group="planner-header-sync",
            exclusive=True,
            exit_on_error=False,
        )

    async def _on_task_changed(self, _task_id: str) -> None:
        """Refresh planner header counters when tasks change."""
        if not self.is_mounted:
            return
        self._schedule_header_refresh()

    async def _refresh_header_task_count(self) -> None:
        """Synchronize planner header counters from the task backend."""
        try:
            await self.sync_header_context(self.header)
            await self.auto_sync_branch(self.header)
        except (RepositoryClosing, OperationalError):
            return

    async def _restore_state(self, persistent: PersistentPlannerState) -> None:
        """Restore planner session state from persisted screen data."""
        output = self._get_output()

        has_content = bool(persistent.conversation_history or persistent.pending_plan)
        if has_content:
            self._show_output()

        self._state.conversation_history = list(persistent.conversation_history)
        for msg in persistent.conversation_history:
            if msg.role == ChatRole.USER:
                await output.post_user_input(msg.content)
            else:
                await output.post_response(msg.content)
                if msg.todos:
                    await output.post_plan(msg.todos)
            for note in msg.notes:
                await output.post_note(note.text, classes=note.classes)

        if persistent.input_text:
            planner_input = self.query_one("#planner-input", PlannerInput)
            planner_input.insert(persistent.input_text)

        if persistent.pending_plan:
            self._state.pending_plan = persistent.pending_plan
            self._state.has_pending_plan = True
            await output.post_plan_approval(persistent.pending_plan)

        if persistent.agent is not None:
            self._state.agent = persistent.agent
            self._state.agent.set_message_target(cast("MessagePump", self))
            self._state.agent_ready = persistent.agent_ready
            self._state.refiner = persistent.refiner

            if self._state.agent_ready:
                self._enable_input()
                self._update_status("ready", "Press ? for help")
        else:
            await self._start_planner()

    async def _start_planner(self) -> None:
        """Start a fresh planner agent instance."""
        config = self.kagan_app.config
        agent_config = config.get_worker_agent()
        if agent_config is None:
            agent_config = get_fallback_agent_config()

        agent = self._agent_factory(self.kagan_app.project_root, agent_config, read_only=True)
        auto_approve = resolve_auto_approve(
            scope=AgentPermissionScope.PLANNER,
            planner_auto_approve=config.general.auto_approve,
        )
        agent.set_auto_approve(auto_approve)
        agent.start(self)

        self._state.agent = agent

    def _is_persistent_state_compatible(self, state: PersistentPlannerState) -> bool:
        """Return whether persisted planner state matches active repo and project root."""
        if state.active_repo_id != self.ctx.active_repo_id:
            return False
        return state.project_root == str(self.kagan_app.project_root)

    async def _discard_persistent_state(self, state: PersistentPlannerState | None) -> None:
        """Stop and clear persisted planner state when it cannot be restored."""
        if state is None:
            return
        if state.agent:
            await state.agent.stop()
        if state.refiner:
            await state.refiner.stop()
        self.kagan_app.planner_state = None

    def _get_output(self) -> StreamingOutput:
        """Return the planner streaming output widget."""
        return self.query_one("#planner-output", StreamingOutput)

    def _show_output(self) -> None:
        """Switch planner UI from empty state to streaming output mode."""
        if not self._state.has_output:
            self._state.has_output = True
            with suppress(NoMatches):
                self.query_one(PlannerEmptyState).add_class("hidden")
            with suppress(NoMatches):
                self._get_output().add_class("visible")

    def _update_status(self, status: str, message: str) -> None:
        """Set planner status badge and hint copy."""
        self.planner_status = status
        self.planner_hint = message

    def _enable_input(self) -> None:
        """Enable planner prompt input interactions."""
        with suppress(NoMatches):
            planner_input = self.query_one("#planner-input", PlannerInput)
            planner_input.remove_class("-disabled")
            planner_input.read_only = False

    def _disable_input(self) -> None:
        """Disable planner prompt input interactions."""
        with suppress(NoMatches):
            planner_input = self.query_one("#planner-input", PlannerInput)
            planner_input.add_class("-disabled")
            planner_input.read_only = True

    def _focus_input(self) -> None:
        """Focus planner prompt input when available."""
        with suppress(NoMatches):
            self.query_one("#planner-input", PlannerInput).focus()

    # ------------------------------------------------------------------
    # Rendering & stream handlers
    # ------------------------------------------------------------------

    async def _handle_agent_update(self, message: messages.AgentUpdate) -> None:
        self._state.accumulated_response.append(message.text)
        await self._get_output().post_response(message.text)

    async def _handle_thinking(self, message: messages.Thinking) -> None:
        if not self._state.thinking_shown:
            self._state.thinking_shown = True
            await self._get_output().post_thinking_indicator()
        await self._get_output().post_thought(message.text)

    async def _handle_agent_ready(self, _message: messages.AgentReady) -> None:
        self._clearing = False
        self._state = self._state.with_agent_ready(True)
        self._enable_input()
        self._update_status("ready", "Press ? for help")

    async def _handle_agent_fail(self, message: messages.AgentFail) -> None:
        from kagan.tui.ui.utils.helpers import is_graceful_agent_termination

        if is_graceful_agent_termination(message.message):
            self._update_status("ready", "Agent stream ended (cancelled)")
            self._enable_input()
            await self._get_output().post_note(
                "Agent stream ended by cancellation (SIGTERM).",
                classes="dismissed",
            )
            return

        self._update_status("error", f"Error: {message.message}")
        self._disable_input()
        output = self._get_output()
        await output.post_note(f"Error: {message.message}", classes="error")
        if message.details:
            await output.post_note(message.details)

    def _handle_set_modes(self, message: messages.SetModes) -> None:
        self._current_mode = message.current_mode
        self._available_modes = message.modes

    def _handle_mode_update(self, message: messages.ModeUpdate) -> None:
        self._current_mode = message.current_mode

    def _handle_commands_update(self, message: messages.AvailableCommandsUpdate) -> None:
        self._available_commands = message.commands

    async def _handle_request_permission(self, message: messages.RequestPermission) -> None:
        await self._get_output().post_permission_request(
            message.options,
            message.tool_call,
            message.result_future,
            timeout=300.0,
        )

    # ------------------------------------------------------------------
    # Proposal persistence
    # ------------------------------------------------------------------

    async def _persist_proposal_draft(
        self,
        tasks: list[Task],
        todos: Sequence[object] | None,
    ) -> None:
        """Save the proposal as a draft in the DB before UI rendering."""
        project_id = self.ctx.active_project_id
        if project_id is None:
            return
        tasks_json = [
            {
                "title": t.title,
                "description": t.description,
                "priority": t.priority.label if t.priority else "MED",
                "task_type": t.task_type.value if t.task_type else "PAIR",
                "acceptance_criteria": t.acceptance_criteria or [],
            }
            for t in tasks
        ]
        todos_json = []
        if todos:
            for entry in todos:
                if hasattr(entry, "content"):
                    todos_json.append(
                        {
                            "content": entry.content,
                            "status": getattr(entry, "status", "completed"),
                        }
                    )
                elif isinstance(entry, dict):
                    todos_json.append(entry)
        proposal = await self.ctx.api.save_planner_draft(
            project_id=project_id,
            repo_id=self.ctx.active_repo_id,
            tasks_json=tasks_json,
            todos_json=todos_json,
        )
        if proposal is not None:
            proposal_id = self._proposal_identifier(proposal)
            if proposal_id is not None:
                self._pending_proposal_id = proposal_id

    async def _update_proposal_status(self, status: ProposalStatus) -> None:
        """Update the persisted proposal status (approved / rejected)."""
        if self._pending_proposal_id is None:
            return
        await self.ctx.api.update_planner_draft_status(self._pending_proposal_id, status)
        self._pending_proposal_id = None

    def _proposal_identifier(self, proposal: object) -> str | None:
        """Return a normalized planner proposal ID across payload variants."""
        raw_id: object | None = None
        if isinstance(proposal, dict):
            raw_id = proposal.get("id") or proposal.get("proposal_id")
        else:
            raw_id = getattr(proposal, "id", None) or getattr(proposal, "proposal_id", None)

        if raw_id is None:
            return None
        proposal_id = str(raw_id).strip()
        return proposal_id or None

    async def _load_pending_proposals(self) -> None:
        """Load draft proposals from DB and display the most recent one."""
        project_id = self.ctx.active_project_id
        if project_id is None:
            return
        proposals = await self.ctx.api.list_pending_planner_drafts(
            project_id,
            repo_id=self.ctx.active_repo_id,
        )
        if not proposals:
            return
        latest = proposals[0]
        tasks = self._proposal_to_tasks(latest)
        if not tasks:
            return
        self._pending_proposal_id = self._proposal_identifier(latest)
        self._state = self._state.with_pending_plan(tasks)
        self._show_output()
        await self._get_output().post_plan_approval(tasks)

    def _proposal_to_tasks(self, proposal: object) -> list[Task]:
        """Convert a persisted PlannerProposal into in-memory Task objects."""
        from uuid import uuid4

        from kagan.core.adapters.db.schema import Task
        from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType

        tasks_data = getattr(proposal, "tasks_json", [])
        if not isinstance(tasks_data, list):
            return []
        priority_map = {
            "LOW": TaskPriority.LOW,
            "MED": TaskPriority.MEDIUM,
            "HIGH": TaskPriority.HIGH,
        }
        result: list[Task] = []
        now = utc_now()
        project_id = getattr(proposal, "project_id", "plan")
        for item in tasks_data:
            if not isinstance(item, dict):
                continue
            task_type_str = item.get("task_type", "PAIR")
            task_type = TaskType.AUTO if task_type_str == "AUTO" else TaskType.PAIR
            priority_str = str(item.get("priority", "MED")).upper()
            priority = priority_map.get(priority_str, TaskPriority.MEDIUM)
            result.append(
                Task(
                    id=uuid4().hex[:8],
                    project_id=project_id,
                    title=str(item.get("title", ""))[:200],
                    description=str(item.get("description", "")),
                    status=TaskStatus.BACKLOG,
                    priority=priority,
                    task_type=task_type,
                    acceptance_criteria=item.get("acceptance_criteria", []),
                    created_at=now,
                    updated_at=now,
                )
            )
        return result

    # ------------------------------------------------------------------
    # Planner queue display
    # ------------------------------------------------------------------

    async def _refresh_planner_queue_messages(self) -> None:
        with suppress(NoMatches):
            container = self.query_one("#planner-queued-messages", QueuedMessagesContainer)
            queue_messages = await self.ctx.api.get_queued_messages(
                self._planner_queue_key(),
                lane="planner",
            )
            container.update_messages(queue_messages)
            self._planner_queue_pending = bool(queue_messages)

    async def _queue_planner_message(self, content: str) -> None:
        await self.ctx.api.queue_message(self._planner_queue_key(), content, lane="planner")
        await self._refresh_planner_queue_messages()
        await self._get_output().post_note("Queued message for next planner turn.", classes="info")
        self._update_status("queued", "Planner follow-up queued")

    async def _consume_planner_queue_if_needed(self) -> None:
        if self._state.phase != PlannerPhase.IDLE:
            return
        if self._state.has_pending_plan:
            return
        queued = await self.ctx.api.take_queued_message(self._planner_queue_key(), lane="planner")
        await self._refresh_planner_queue_messages()
        if queued is None:
            return
        await self._submit_prompt(
            prompt_text=truncate_queue_payload(
                queued.content,
                prefix=PLANNER_QUEUE_TRUNCATION_PREFIX,
                max_chars=PLANNER_QUEUE_TRUNCATION_MAX_CHARS,
            )
        )

    # ------------------------------------------------------------------
    # Actions
    # ------------------------------------------------------------------

    async def _handle_offline_reconnect(self, event: OfflineBanner.Reconnect) -> None:
        self.ctx.api.refresh_agent_health()
        if self.ctx.api.is_agent_available():
            for banner in self.query(OfflineBanner):
                await banner.remove()

            self._update_status("initializing", "Initializing agent...")
            await self._start_planner()
            self._focus_input()
            self.notify("Agent is now available", severity="information")
        else:
            self.notify("Agent still unavailable", severity="warning")

    async def _handle_agent_message(self, message: _acp_messages.AgentMessage) -> None:
        await self._agent_stream.dispatch(message)

    # ------------------------------------------------------------------
    # Slash registration and command execution
    # ------------------------------------------------------------------

    def _register_slash_commands(self) -> None:
        @self._slash_registry.command(aliases=["cls"])
        async def clear(_args: str) -> None:
            await self._execute_clear()

        @self._slash_registry.command(aliases=["h", "?"])
        async def help(_args: str) -> None:
            await self._execute_help()

    async def _execute_slash_command(self, command_name: str, args: str) -> bool:
        command = self._slash_registry.find_command(command_name)
        if command is None:
            self.notify(f"Unknown command: /{command_name}", severity="warning")
            return False

        result = command.func(args)
        if asyncio.iscoroutine(result):
            await result
        return True

    # ------------------------------------------------------------------
    # Input change / slash commands
    # ------------------------------------------------------------------

    async def _handle_planner_input_changed(self, _event: TextArea.Changed) -> None:
        await self._check_slash_trigger()

    async def _check_slash_trigger(self) -> None:
        with suppress(NoMatches):
            planner_input = self.query_one("#planner-input", PlannerInput)
            text = planner_input.text

            if text.startswith("/") and len(text) <= 2:
                await self._show_slash_complete()
            elif self._slash_complete is not None and not text.startswith("/"):
                await self._hide_slash_complete()

    async def _show_slash_complete(self) -> None:
        if self._slash_complete is None:
            self._slash_complete = SlashComplete(id="slash-complete").data_bind(
                slash_commands=PlannerScreen.slash_commands
            )
            bottom = self.query_one("#planner-bottom", Vertical)
            planner_input = self.query_one("#planner-input", PlannerInput)
            await bottom.mount(self._slash_complete, before=planner_input)
            planner_input.focus()

    async def _hide_slash_complete(self) -> None:
        if self._slash_complete is not None:
            await self._slash_complete.remove()
            self._slash_complete = None

    async def _handle_slash_completed(self, event: SlashComplete.Completed) -> None:
        planner_input = self.query_one("#planner-input", PlannerInput)
        planner_input.clear()
        await self._hide_slash_complete()
        await self._execute_slash_command(event.command, "")

    async def _handle_slash_dismissed(self, event: SlashComplete.Dismissed) -> None:
        await self._hide_slash_complete()
        self._focus_input()

    # ------------------------------------------------------------------
    # Slash key forwarding from PlannerInput
    # ------------------------------------------------------------------

    async def _handle_slash_key(self, event: PlannerInput.SlashKey) -> None:
        if self._slash_complete is None:
            return

        key = event.key
        if key == "up":
            self._slash_complete.action_cursor_up()
        elif key == "down":
            self._slash_complete.action_cursor_down()
        elif key == "enter":
            self._slash_complete.action_select()
        elif key == "escape":
            planner_input = self.query_one("#planner-input", PlannerInput)
            planner_input.clear()
            await self._hide_slash_complete()

    # ------------------------------------------------------------------
    # Submit handling
    # ------------------------------------------------------------------

    async def _handle_submit_requested(self, event: PlannerInput.SubmitRequested) -> None:
        if slash_call := parse_slash_command_call(event.text):
            await self._execute_slash_command(slash_call.name, slash_call.args)
            self.query_one("#planner-input", PlannerInput).clear()
            await self._hide_slash_complete()
            return

        if self._state.phase == PlannerPhase.PROCESSING:
            queued_text = event.text.strip()
            if queued_text:
                await self._queue_planner_message(queued_text)
                self.query_one("#planner-input", PlannerInput).clear()
            return
        if self._state.has_pending_plan:
            self.notify(
                "Please approve or dismiss the pending plan first",
                severity="warning",
            )
            return
        if not self._state.can_submit():
            return
        await self._submit_prompt()

    async def _submit_prompt(self, prompt_text: str | None = None) -> None:
        planner_input = self.query_one("#planner-input", PlannerInput)
        text = (prompt_text if prompt_text is not None else planner_input.text).strip()
        if not text:
            return

        if self._state.agent is not None and not self._actions_agent_matches_active_repo():
            await self._execute_clear(notify=False)

        self._state = self._state.transition(PlannerEvent.SUBMIT)
        self._state.todos_displayed = False
        self._state.thinking_shown = False
        self._update_status("thinking", "Processing...")

        if prompt_text is None:
            planner_input.clear()
        else:
            await self._get_output().post_note(
                "Processing queued planner follow-up...",
                classes="info",
            )

        self._show_output()
        output = self._get_output()
        output.reset_turn()
        await output.post_user_input(text)

        self._state.conversation_history.append(
            ChatMessage(role=ChatRole.USER, content=text, timestamp=utc_now())
        )

        if self._state.agent:
            self.run_worker(
                self._send_to_agent(text),
                group="planner-send-to-agent",
                exclusive=True,
                exit_on_error=False,
            )
        else:
            self._state = self._state.transition(PlannerEvent.ERROR)
            self._enable_input()
            self.notify("Planner not running", severity="warning")

    async def _send_to_agent(self, text: str) -> None:
        if not self._state.agent:
            self._state = self._state.transition(PlannerEvent.ERROR)
            self._enable_input()
            return

        self._state.accumulated_response.clear()
        history: list[tuple[str, str]] = [
            (msg.role, msg.content) for msg in self._state.conversation_history
        ]
        prompt = build_planner_prompt(text, conversation_history=history if history else None)

        try:
            await self._state.agent.wait_ready(timeout=AGENT_TIMEOUT)
            await self._state.agent.send_prompt(prompt)
            tasks, todos, plan_error = parse_proposed_plan(self._state.agent.tool_calls)

            if self._state.accumulated_response:
                full_response = "".join(self._state.accumulated_response)
                self._state.conversation_history.append(
                    ChatMessage(
                        role=ChatRole.ASSISTANT,
                        content=full_response,
                        timestamp=utc_now(),
                        plan_tasks=tasks or None,
                        todos=todos,
                    )
                )

            output = self._get_output()
            if todos and not self._state.todos_displayed:
                self._state.todos_displayed = True
                await output.post_plan(todos)

            if plan_error:
                await output.post_note(plan_error, classes="error")

            if tasks:
                await self._persist_proposal_draft(tasks, todos)
                self._state = self._state.with_pending_plan(tasks)
                self._state = self._state.transition(PlannerEvent.PLAN_RECEIVED)
                await output.post_plan_approval(tasks)
        except Exception as exc:
            await self._get_output().post_note(f"Error: {exc}", classes="error")
            self._state = self._state.transition(PlannerEvent.ERROR)
            self._enable_input()
        else:
            if not self._state.has_pending_plan:
                self._state = self._state.transition(PlannerEvent.DONE)
                self._enable_input()

        self._update_status("ready", "Press ? for help")
        await self._consume_planner_queue_if_needed()

    def _actions_agent_matches_active_repo(self) -> bool:
        agent = self._state.agent
        if agent is None:
            return False
        agent_root = getattr(agent, "project_root", None)
        if agent_root is None:
            return True
        try:
            return Path(agent_root).resolve() == self.kagan_app.project_root.resolve()
        except (OSError, RuntimeError, TypeError, ValueError):
            return True

    # ------------------------------------------------------------------
    # Plan approval / dismissal / edit
    # ------------------------------------------------------------------

    async def _handle_plan_approved(self, event: PlanApprovalWidget.Approved) -> None:
        self._state = self._state.transition(PlannerEvent.APPROVED)
        await self._update_proposal_status(ProposalStatus.APPROVED)
        output = self._get_output()
        created_tasks: list[tuple[str, str, str]] = []

        for task_data in event.tasks:
            try:
                project_id = self.ctx.active_project_id
                if project_id is None:
                    if task_data.project_id and task_data.project_id != "plan":
                        project_id = task_data.project_id
                task = await self.ctx.api.create_task(
                    task_data.title,
                    task_data.description,
                    project_id=project_id,
                    created_by=None,
                )
                await self.ctx.api.update_task(
                    task.id,
                    priority=task_data.priority,
                    task_type=task_data.task_type,
                    agent_backend=task_data.agent_backend,
                    acceptance_criteria=task_data.acceptance_criteria,
                )
                self.notify(
                    f"Created: {task.title[:PLANNER_TITLE_MAX_LENGTH]}",
                    severity="information",
                )
                created_tasks.append(
                    (
                        task.title,
                        task.task_type.value if task.task_type else "PAIR",
                        task.priority.label if task.priority else "Medium",
                    )
                )
            except Exception as e:
                self.notify(f"Failed to create task: {e}", severity="error")

        self._state.pending_plan = None
        self._state.has_pending_plan = False
        self._state.accumulated_response.clear()
        self._state = self._state.transition(PlannerEvent.DONE)

        if created_tasks:
            lines = [
                f"[bold]Created {len(created_tasks)} task(s):[/bold]",
                "",
            ]
            for title, task_type, priority in created_tasks:
                display_title = title[:60] + "..." if len(title) > 60 else title
                lines.append(
                    f"  - [dim]{task_type}[/dim] {display_title} [italic]({priority})[/italic]"
                )
            note_text = "\n".join(lines)

            await output.post_note(note_text, classes="success")
            if self._state.conversation_history:
                last_msg = self._state.conversation_history[-1]
                if last_msg.role == ChatRole.ASSISTANT:
                    last_msg.notes.append(NoteInfo(text=note_text, classes="success"))
            await self.action_to_board()
        else:
            self._enable_input()

    async def _handle_plan_dismissed(self, event: PlanApprovalWidget.Dismissed) -> None:
        await self._update_proposal_status(ProposalStatus.REJECTED)
        self._state.pending_plan = None
        self._state.has_pending_plan = False
        self._state = self._state.transition(PlannerEvent.REJECTED)

        if self._state.agent:
            self._state = self._state.transition(PlannerEvent.SUBMIT)
            self._state.todos_displayed = False
            self._state.thinking_shown = False

            self._disable_input()
            self._update_status("thinking", "Processing...")

            output = self._get_output()
            output.reset_turn()
            await output.post_note("Plan dismissed", classes="dismissed")

            context = (
                "The user dismissed the proposed plan. "
                "Ask them what they would like to change or clarify "
                "about the requirements."
            )
            self.run_worker(
                self._send_to_agent(context),
                group="planner-send-to-agent",
                exclusive=True,
                exit_on_error=False,
            )
        else:
            self._enable_input()

    async def _handle_plan_edit_requested(self, event: PlanApprovalWidget.EditRequested) -> None:
        from kagan.tui.ui.screens.task_editor import TaskEditorScreen

        self._state = self._state.transition(PlannerEvent.EDIT)
        result = await await_screen_result(self.app, TaskEditorScreen(event.tasks))
        await self._on_task_editor_result(result)

    async def _on_task_editor_result(self, result: list[Task] | None) -> None:
        if result is None:
            if self._state.pending_plan:
                await self._get_output().post_plan_approval(self._state.pending_plan)
        else:
            self._state.pending_plan = result
            await self._get_output().post_plan_approval(result)

    # ------------------------------------------------------------------
    # Queued message removal
    # ------------------------------------------------------------------

    async def _handle_queue_remove_requested(self, event: QueuedMessageRow.RemoveRequested) -> None:
        await self.ctx.api.remove_queued_message(
            self._planner_queue_key(),
            event.index,
            lane="planner",
        )
        await self._refresh_planner_queue_messages()

    def _planner_queue_key(self) -> str:
        repo_id = self.ctx.active_repo_id or "global"
        return f"planner:{repo_id}"

    # ------------------------------------------------------------------
    # Control actions
    # ------------------------------------------------------------------

    async def action_cancel(self) -> None:
        """Cancel the current agent operation or clear the input field."""
        planner_input = self.query_one("#planner-input", PlannerInput)
        if not self._state.agent or self._state.phase != PlannerPhase.PROCESSING:
            if planner_input.text:
                planner_input.clear()
            return

        if self._state.accumulated_response:
            partial_content = "".join(self._state.accumulated_response) + "\n\n*[interrupted]*"
            self._state.conversation_history.append(
                ChatMessage(
                    role=ChatRole.ASSISTANT,
                    content=partial_content,
                    timestamp=utc_now(),
                )
            )

        await self._state.agent.cancel()
        await self._get_output().post_note("*Interrupted by user*", classes="warning")

        self._state = self._state.transition(PlannerEvent.DONE)
        self._enable_input()
        self.notify("Interrupted - you can continue the conversation")

    async def action_refine(self) -> None:
        """Refine the current prompt using dedicated ACP agent."""
        if not self._state.can_refine() or self._state.phase == PlannerPhase.REFINING:
            return

        planner_input = self.query_one("#planner-input", PlannerInput)
        text = planner_input.text.strip()

        if not text:
            self.notify("Nothing to enhance", severity="warning")
            return

        config = self.kagan_app.config
        refinement_config = config.refinement

        if not refinement_config.enabled:
            self.notify("Prompt refinement is disabled", severity="warning")
            return

        if len(text) < refinement_config.skip_length_under:
            self.notify("Input too short to enhance", severity="warning")
            return

        if any(text.startswith(prefix) for prefix in refinement_config.skip_prefixes):
            self.notify("Commands cannot be enhanced", severity="warning")
            return

        self._state = self._state.transition(PlannerEvent.REFINE)
        planner_input.add_class("-refining")
        self._update_status("refining", "Enhancing prompt...")

        try:
            if not self._state.refiner:
                from kagan.core.agents.refiner import PromptRefiner

                agent_config = config.get_worker_agent()
                if agent_config is None:
                    agent_config = get_fallback_agent_config()
                self._state.refiner = PromptRefiner(
                    self.kagan_app.project_root,
                    agent_config,
                    self._agent_factory,
                )

            refined = await self._state.refiner.refine(text)

            planner_input.clear()
            planner_input.insert(refined)
            self.notify("Prompt enhanced - review and press Enter")

        except TimeoutError:
            self.notify("Refinement timed out", severity="error")
        except Exception as e:
            self.notify(f"Refinement failed: {e}", severity="error")
        finally:
            self._state = self._state.transition(PlannerEvent.DONE)
            planner_input.remove_class("-refining")
            self._focus_input()
            self._update_status("ready", "Press ? for help")

    async def action_to_board(self) -> None:
        """Navigate back to the Kanban board."""
        from kagan.tui.ui.screens.kanban import KanbanScreen

        target_kanban = next(
            (
                screen
                for screen in reversed(self.app.screen_stack[:-1])
                if isinstance(screen, KanbanScreen)
            ),
            None,
        )

        if target_kanban is not None:
            await target_kanban.prepare_for_planner_return()
            self.app.pop_screen()
        else:
            self.app.switch_screen(KanbanScreen())

    def action_set_task_branch(self) -> None:
        """Warn that task branch selection is available from Kanban."""
        self.notify(
            "Select a task on the Kanban board to set its branch",
            severity="warning",
        )

    async def _execute_clear(self, *, notify: bool = True) -> None:
        """Reset planner session and restart planner agent."""
        self._clearing = True
        self._state.agent_ready = False
        self._state.accumulated_response.clear()
        self._state.conversation_history.clear()
        self._state.pending_plan = None
        self._state.has_pending_plan = False
        self._pending_proposal_id = None
        await self._get_output().clear()

        if self._state.refiner:
            await self._state.refiner.stop()
            self._state.refiner = None
        if self._state.agent:
            await self._state.agent.stop()
            self._state.agent = None

        self.kagan_app.planner_state = None
        await self._start_planner()
        if notify:
            self.notify("Conversation cleared")

    async def reset_for_repo_change(self) -> None:
        """Reset planner state when repository context changes."""
        with suppress(NoMatches):
            self.query_one(OfflineBanner).remove()
        await self._execute_clear(notify=False)

    async def _execute_help(self) -> None:
        """Render planner slash-command help into the output panel."""
        self._show_output()
        help_text = "**Available Commands:**\n"
        commands: list[SlashCommand] = sorted(
            self._slash_registry.list_commands(),
            key=lambda cmd: cmd.command,
        )
        for cmd in commands:
            aliases = ""
            if cmd.aliases:
                alias_text = ", ".join(f"/{alias}" for alias in cmd.aliases)
                aliases = f" ({alias_text})"
            help_text += f"- `/{cmd.command}`{aliases} - {cmd.help}\n"
        await self._get_output().post_note(help_text)
