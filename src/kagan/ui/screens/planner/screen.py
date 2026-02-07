"""Planner screen for chat-first task creation."""

from __future__ import annotations

from contextlib import suppress
from dataclasses import dataclass
from datetime import datetime
from typing import TYPE_CHECKING, ClassVar, cast

from textual import events, getters, on
from textual.binding import Binding, BindingType
from textual.containers import Center, Vertical
from textual.css.query import NoMatches
from textual.message import Message
from textual.widgets import Footer, Static, TextArea

from kagan.acp import messages
from kagan.agents.agent_factory import AgentFactory, create_agent
from kagan.agents.planner import build_planner_prompt, parse_proposed_plan
from kagan.agents.refiner import PromptRefiner
from kagan.config import get_fallback_agent_config
from kagan.constants import BOX_DRAWING, KAGAN_LOGO, PLANNER_TITLE_MAX_LENGTH
from kagan.core.models.enums import ChatRole
from kagan.git_utils import list_local_branches
from kagan.keybindings import PLANNER_BINDINGS
from kagan.limits import AGENT_TIMEOUT
from kagan.ui.modals import BaseBranchModal
from kagan.ui.screens.base import KaganScreen
from kagan.ui.screens.planner.message_handler import MessageHandler
from kagan.ui.screens.planner.state import (
    ChatMessage,
    NoteInfo,
    PersistentPlannerState,
    PlannerPhase,
    PlannerState,
    SlashCommand,
)
from kagan.ui.screens.task_editor import TaskEditorScreen
from kagan.ui.widgets import StatusBar, StreamingOutput
from kagan.ui.widgets.chat_panel import QueuedMessageRow, QueuedMessagesContainer
from kagan.ui.widgets.header import KaganHeader
from kagan.ui.widgets.offline_banner import OfflineBanner
from kagan.ui.widgets.plan_approval import PlanApprovalWidget
from kagan.ui.widgets.slash_complete import SlashComplete

if TYPE_CHECKING:
    from acp.schema import AvailableCommand
    from textual.app import ComposeResult

    from kagan.app import KaganApp
    from kagan.core.models.entities import Task
    from kagan.services.queued_messages import QueuedMessageService

MIN_INPUT_HEIGHT = 1
MAX_INPUT_HEIGHT = 6


PLANNER_EXAMPLES = [
    '"Add user authentication with OAuth2"',
    '"Refactor the payment module for better testing"',
    '"Fix the bug where users can\'t upload images"',
]


class PlannerEmptyState(Vertical):
    """Empty state widget for planner screen with branded design."""

    DEFAULT_CLASSES = "planner-empty-state"

    def compose(self) -> ComposeResult:
        """Compose the planner empty state layout."""
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

                    with Vertical(id="planner-empty-examples", classes="planner-empty-examples"):
                        yield Static("Examples:", classes="planner-empty-section-title")
                        for example in PLANNER_EXAMPLES:
                            yield Static(
                                f"  {BOX_DRAWING['BULLET']} {example}",
                                classes="planner-empty-example",
                            )


class PlannerInput(TextArea):
    """TextArea that submits on Enter (Shift+Enter/Ctrl+J for newline)."""

    BINDINGS: ClassVar[list[BindingType]] = [
        Binding("shift+enter,ctrl+j", "insert_newline", "New Line", show=False, priority=True),
    ]

    @dataclass
    class SubmitRequested(Message):
        text: str

    @dataclass
    class SlashKey(Message):
        """Forward key to slash complete."""

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
            elif event.key == "enter":
                event.prevent_default()
                event.stop()
                self.post_message(self.SlashKey("enter"))
                return
            elif event.key == "escape":
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

    BINDINGS = PLANNER_BINDINGS
    header = getters.query_one(KaganHeader)

    def __init__(self, agent_factory: AgentFactory = create_agent, **kwargs) -> None:
        super().__init__(**kwargs)
        self._state = PlannerState()
        self._message_handler = MessageHandler(self)
        self._agent_factory = agent_factory

        self._current_mode: str = ""
        self._available_modes: dict[str, messages.Mode] = {}
        self._available_commands: list[AvailableCommand] = []

        self._clearing: bool = False
        self._slash_complete: SlashComplete | None = None
        self._builtin_commands: list[SlashCommand] = [
            SlashCommand("clear", "Clear conversation and start fresh"),
            SlashCommand("help", "Show available commands"),
        ]
        self._planner_queue_pending = False

    def compose(self) -> ComposeResult:
        yield KaganHeader()
        with Vertical(id="planner-container"):
            yield Static("Plan Mode", id="planner-header")
            yield PlannerEmptyState()
            yield StreamingOutput(id="planner-output")
            with Vertical(id="planner-bottom"):
                yield StatusBar()
                yield QueuedMessagesContainer(
                    id="planner-queued-messages",
                    classes="queued-messages-container planner-queued-messages",
                )
                yield PlannerInput(
                    "",
                    id="planner-input",
                    show_line_numbers=False,
                    placeholder="Describe your task... (/ for commands)",
                )
        yield Footer(show_command_palette=False)

    async def on_mount(self) -> None:
        await self.sync_header_context(self.header)
        with suppress(NoMatches):
            self.query_one("#planner-queued-messages", QueuedMessagesContainer).display = False
        from kagan.ui.widgets.header import _get_git_branch

        if self.ctx.active_repo_id is None:
            banner = OfflineBanner(message="Select a repository to start planning")
            container = self.query_one("#planner-container", Vertical)
            await container.mount(banner, before=0)
            self._disable_input()
            self._update_status("offline", "No repository selected")
            await self._discard_persistent_state(self.kagan_app.planner_state)
            return

        branch = await _get_git_branch(self.kagan_app.project_root)
        self.header.update_branch(branch)

        if not self.ctx.agent_health.is_available():
            message = self.ctx.agent_health.get_status_message()
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

        self._focus_input()
        await self._refresh_planner_queue_messages()

    @on(OfflineBanner.Reconnect)
    async def on_offline_banner_reconnect(self, event: OfflineBanner.Reconnect) -> None:
        """Handle reconnect from offline banner - refresh agent health check."""
        self.ctx.agent_health.refresh()
        if self.ctx.agent_health.is_available():
            for banner in self.query(OfflineBanner):
                await banner.remove()

            self._update_status("initializing", "Initializing agent...")
            await self._start_planner()
            self._focus_input()
            self.notify("Agent is now available", severity="information")
        else:
            self.notify("Agent still unavailable", severity="warning")

    async def _restore_state(self, persistent: PersistentPlannerState) -> None:
        """Restore state from a previous session."""
        output = self._get_output()

        has_content = bool(persistent.conversation_history or persistent.pending_plan)
        if has_content:
            self._show_output()

        self._state.conversation_history = list(persistent.conversation_history)
        for msg in persistent.conversation_history:
            if msg.role == "user":
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
            self._state.agent.set_message_target(self)
            self._state.agent_ready = persistent.agent_ready
            self._state.refiner = persistent.refiner

            if self._state.agent_ready:
                self._enable_input()
                self._update_status("ready", "Press ? for help")
        else:
            await self._start_planner()

    async def _start_planner(self) -> None:
        """Start a new planner agent."""
        config = self.kagan_app.config
        agent_config = config.get_worker_agent()
        if agent_config is None:
            agent_config = get_fallback_agent_config()

        agent = self._agent_factory(self.kagan_app.project_root, agent_config, read_only=True)
        agent.set_auto_approve(config.general.auto_approve)
        agent.start(self)

        self._state.agent = agent

    def _is_persistent_state_compatible(self, state: PersistentPlannerState) -> bool:
        if state.active_repo_id != self.ctx.active_repo_id:
            return False
        return state.project_root == str(self.kagan_app.project_root)

    async def _discard_persistent_state(self, state: PersistentPlannerState | None) -> None:
        if state is None:
            return
        if state.agent:
            await state.agent.stop()
        if state.refiner:
            await state.refiner.stop()
        self.kagan_app.planner_state = None

    def _get_output(self) -> StreamingOutput:
        return self.query_one("#planner-output", StreamingOutput)

    def _show_output(self) -> None:
        if not self._state.has_output:
            self._state.has_output = True
            with suppress(NoMatches):
                self.query_one(PlannerEmptyState).add_class("hidden")
            with suppress(NoMatches):
                self._get_output().add_class("visible")

    def _update_status(self, status: str, message: str) -> None:
        with suppress(NoMatches):
            self.query_one(StatusBar).update_status(status, message)

    def _enable_input(self) -> None:
        with suppress(NoMatches):
            planner_input = self.query_one("#planner-input", PlannerInput)
            planner_input.remove_class("-disabled")
            planner_input.read_only = False

    def _disable_input(self) -> None:
        with suppress(NoMatches):
            planner_input = self.query_one("#planner-input", PlannerInput)
            planner_input.add_class("-disabled")
            planner_input.read_only = True

    def _focus_input(self) -> None:
        with suppress(NoMatches):
            self.query_one("#planner-input", PlannerInput).focus()

    @on(PlannerInput.SubmitRequested)
    async def on_submit_requested(self, event: PlannerInput.SubmitRequested) -> None:
        if self._state.phase == PlannerPhase.PROCESSING:
            queued_text = event.text.strip()
            if queued_text:
                await self._queue_planner_message(queued_text)
                self.query_one("#planner-input", PlannerInput).clear()
            return
        if self._state.has_pending_plan:
            self.notify("Please approve or dismiss the pending plan first", severity="warning")
            return
        if not self._state.can_submit():
            return
        await self._submit_prompt()

    async def _submit_prompt(self, prompt_text: str | None = None) -> None:
        planner_input = self.query_one("#planner-input", PlannerInput)
        text = (prompt_text if prompt_text is not None else planner_input.text).strip()
        if not text:
            return

        self._state = self._state.transition("submit")
        self._state.todos_displayed = False
        self._state.thinking_shown = False

        self._update_status("thinking", "Processing...")
        if prompt_text is None:
            planner_input.clear()
        else:
            await self._get_output().post_note(
                "Processing queued planner follow-up...", classes="info"
            )

        self._show_output()
        output = self._get_output()
        output.reset_turn()

        await output.post_user_input(text)

        self._state.conversation_history.append(
            ChatMessage(role=ChatRole.USER, content=text, timestamp=datetime.now())
        )

        if self._state.agent:
            self.run_worker(self._send_to_agent(text))
        else:
            self._state = self._state.transition("error")
            self._enable_input()
            self.notify("Planner not running", severity="warning")

    async def _send_to_agent(self, text: str) -> None:
        if not self._state.agent:
            self._state = self._state.transition("error")
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
                        timestamp=datetime.now(),
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
                self._state = self._state.with_pending_plan(tasks)
                self._state = self._state.transition("plan_received")
                await output.post_plan_approval(tasks)
        except Exception as e:
            await self._get_output().post_note(f"Error: {e}", classes="error")
            self._state = self._state.transition("error")
            self._enable_input()
        else:
            if not self._state.has_pending_plan:
                self._state = self._state.transition("done")
                self._enable_input()

        self._update_status("ready", "Press ? for help")
        await self._consume_planner_queue_if_needed()

    def _planner_queue_key(self) -> str:
        repo_id = self.ctx.active_repo_id or "global"
        return f"planner:{repo_id}"

    def _get_queue_service(self) -> QueuedMessageService | None:
        app = cast("KaganApp", self.app)
        service = getattr(app.ctx, "queued_message_service", None)
        if service is None:
            return None
        return cast("QueuedMessageService", service)

    async def _refresh_planner_queue_messages(self) -> None:
        with suppress(NoMatches):
            container = self.query_one("#planner-queued-messages", QueuedMessagesContainer)
            service = self._get_queue_service()
            if service is None:
                container.display = False
                self._planner_queue_pending = False
                return
            messages = await service.get_queued(self._planner_queue_key(), lane="planner")
            container.update_messages(messages)
            self._planner_queue_pending = bool(messages)

    async def _queue_planner_message(self, content: str) -> None:
        service = self._get_queue_service()
        if service is None:
            self.notify("Planner queue unavailable", severity="error")
            return
        await service.queue_message(self._planner_queue_key(), content, lane="planner")
        await self._refresh_planner_queue_messages()
        await self._get_output().post_note("Queued message for next planner turn.", classes="info")
        self._update_status("queued", "Planner follow-up queued")

    async def _consume_planner_queue_if_needed(self) -> None:
        if self._state.phase != PlannerPhase.IDLE:
            return
        if self._state.has_pending_plan:
            return
        service = self._get_queue_service()
        if service is None:
            return
        queued = await service.take_queued(self._planner_queue_key(), lane="planner")
        await self._refresh_planner_queue_messages()
        if queued is None:
            return
        await self._submit_prompt(prompt_text=_truncate_queue_payload(queued.content))

    @on(QueuedMessageRow.RemoveRequested)
    async def on_queue_remove_requested(self, event: QueuedMessageRow.RemoveRequested) -> None:
        service = self._get_queue_service()
        if service is None:
            return
        await service.remove_message(
            self._planner_queue_key(),
            event.index,
            lane="planner",
        )
        await self._refresh_planner_queue_messages()

    @on(messages.AgentUpdate)
    async def on_agent_update(self, message: messages.AgentUpdate) -> None:
        await self._message_handler.handle_agent_update(message)

    @on(messages.Thinking)
    async def on_agent_thinking(self, message: messages.Thinking) -> None:
        await self._message_handler.handle_thinking(message)

    @on(messages.ToolCall)
    async def on_tool_call(self, message: messages.ToolCall) -> None:
        await self._message_handler.handle_tool_call(message)

    @on(messages.ToolCallUpdate)
    async def on_tool_call_update(self, message: messages.ToolCallUpdate) -> None:
        await self._message_handler.handle_tool_call_update(message)

    @on(messages.AgentReady)
    async def on_agent_ready(self, message: messages.AgentReady) -> None:
        self._clearing = False
        await self._message_handler.handle_agent_ready(message)

    @on(messages.AgentFail)
    async def on_agent_fail(self, message: messages.AgentFail) -> None:
        if self._clearing:
            return
        await self._message_handler.handle_agent_fail(message)

    @on(messages.Plan)
    async def on_plan(self, message: messages.Plan) -> None:
        await self._message_handler.handle_plan(message)

    @on(messages.SetModes)
    def on_set_modes(self, message: messages.SetModes) -> None:
        self._message_handler.handle_set_modes(message)

    @on(messages.ModeUpdate)
    def on_mode_update(self, message: messages.ModeUpdate) -> None:
        self._message_handler.handle_mode_update(message)

    @on(messages.AvailableCommandsUpdate)
    def on_commands_update(self, message: messages.AvailableCommandsUpdate) -> None:
        self._message_handler.handle_commands_update(message)

    @on(messages.RequestPermission)
    async def on_request_permission(self, message: messages.RequestPermission) -> None:
        await self._message_handler.handle_request_permission(message)

    @on(PlanApprovalWidget.Approved)
    async def on_plan_approved(self, event: PlanApprovalWidget.Approved) -> None:
        """Handle plan approval - create tasks."""
        self._state = self._state.transition("approved")
        output = self._get_output()
        created_tasks: list[tuple[str, str, str]] = []

        for task_data in event.tasks:
            try:
                project_id = self.ctx.active_project_id
                if project_id is None:
                    if task_data.project_id and task_data.project_id != "plan":
                        project_id = task_data.project_id
                task = await self.ctx.task_service.create_task(
                    task_data.title,
                    task_data.description,
                    project_id=project_id,
                    created_by=None,
                )
                await self.ctx.task_service.update_fields(
                    task.id,
                    priority=task_data.priority,
                    task_type=task_data.task_type,
                    assigned_hat=task_data.assigned_hat,
                    agent_backend=task_data.agent_backend,
                    acceptance_criteria=task_data.acceptance_criteria,
                )
                self.notify(
                    f"Created: {task.title[:PLANNER_TITLE_MAX_LENGTH]}", severity="information"
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
        self._state = self._state.transition("done")

        if created_tasks:
            lines = [f"[bold]Created {len(created_tasks)} task(s):[/bold]", ""]
            for title, task_type, priority in created_tasks:
                display_title = title[:60] + "..." if len(title) > 60 else title
                lines.append(
                    f"  - [dim]{task_type}[/dim] {display_title} [italic]({priority})[/italic]"
                )
            note_text = "\n".join(lines)

            await output.post_note(note_text, classes="success")
            if self._state.conversation_history:
                last_msg = self._state.conversation_history[-1]
                if last_msg.role == "assistant":
                    last_msg.notes.append(NoteInfo(text=note_text, classes="success"))
            await self.action_to_board()
        else:
            self._enable_input()

    @on(PlanApprovalWidget.Dismissed)
    async def on_plan_dismissed(self, event: PlanApprovalWidget.Dismissed) -> None:
        """Handle plan dismissal - ask user what to change."""
        self._state.pending_plan = None
        self._state.has_pending_plan = False
        self._state = self._state.transition("rejected")

        if self._state.agent:
            self._state = self._state.transition("submit")
            self._state.todos_displayed = False
            self._state.thinking_shown = False

            self._disable_input()
            self._update_status("thinking", "Processing...")

            output = self._get_output()
            output.reset_turn()
            await output.post_note("Plan dismissed", classes="dismissed")

            context = (
                "The user dismissed the proposed plan. "
                "Ask them what they would like to change or clarify about the requirements."
            )
            self.run_worker(self._send_to_agent(context))
        else:
            self._enable_input()

    @on(PlanApprovalWidget.EditRequested)
    async def on_plan_edit_requested(self, event: PlanApprovalWidget.EditRequested) -> None:
        """Handle request to edit tasks before approval."""
        self.app.push_screen(
            TaskEditorScreen(event.tasks),
            self._on_task_editor_result,
        )

    async def _on_task_editor_result(self, result: list[Task] | None) -> None:
        """Handle result from task editor."""
        if result is None:
            if self._state.pending_plan:
                await self._get_output().post_plan_approval(self._state.pending_plan)
        else:
            self._state.pending_plan = result
            await self._get_output().post_plan_approval(result)

    @on(TextArea.Changed, "#planner-input")
    def on_planner_input_changed(self, event: TextArea.Changed) -> None:
        self._check_slash_trigger()

    def _check_slash_trigger(self) -> None:
        with suppress(NoMatches):
            planner_input = self.query_one("#planner-input", PlannerInput)
            text = planner_input.text

            if text.startswith("/") and len(text) <= 2:
                self.run_worker(self._show_slash_complete())
            elif self._slash_complete is not None and not text.startswith("/"):
                self.run_worker(self._hide_slash_complete())

    async def _show_slash_complete(self) -> None:
        if self._slash_complete is None:
            self._slash_complete = SlashComplete(id="slash-complete")
            self._slash_complete.slash_commands = self._builtin_commands
            bottom = self.query_one("#planner-bottom", Vertical)
            planner_input = self.query_one("#planner-input", PlannerInput)
            await bottom.mount(self._slash_complete, before=planner_input)
            planner_input.focus()

    async def _hide_slash_complete(self) -> None:
        if self._slash_complete is not None:
            await self._slash_complete.remove()
            self._slash_complete = None

    @on(SlashComplete.Completed)
    async def on_slash_completed(self, event: SlashComplete.Completed) -> None:
        planner_input = self.query_one("#planner-input", PlannerInput)
        planner_input.clear()
        await self._hide_slash_complete()

        if event.command == "clear":
            await self._execute_clear()
        elif event.command == "help":
            await self._execute_help()

    @on(SlashComplete.Dismissed)
    async def on_slash_dismissed(self, event: SlashComplete.Dismissed) -> None:
        await self._hide_slash_complete()
        self._focus_input()

    @on(PlannerInput.SlashKey)
    async def on_slash_key(self, event: PlannerInput.SlashKey) -> None:
        if self._slash_complete is None:
            return

        if event.key == "up":
            self._slash_complete.action_cursor_up()
        elif event.key == "down":
            self._slash_complete.action_cursor_down()
        elif event.key == "enter":
            self._slash_complete.action_select()
        elif event.key == "escape":
            planner_input = self.query_one("#planner-input", PlannerInput)
            planner_input.clear()
            await self._hide_slash_complete()

    async def _execute_clear(self, *, notify: bool = True) -> None:
        """Reset planner session completely."""
        self._clearing = True
        self._state.accumulated_response.clear()
        self._state.conversation_history.clear()
        self._state.pending_plan = None
        self._state.has_pending_plan = False
        await self._get_output().clear()

        if self._state.refiner:
            await self._state.refiner.stop()
            self._state.refiner = None
        if self._state.agent:
            await self._state.agent.stop()
            self._state.agent = None
            self._state.agent_ready = False

        self.kagan_app.planner_state = None
        await self._start_planner()
        if notify:
            self.notify("Conversation cleared")

    async def reset_for_repo_change(self) -> None:
        """Reset planner state when the active repo changes."""
        with suppress(NoMatches):
            self.query_one(OfflineBanner).remove()
        await self._execute_clear(notify=False)

    async def _execute_help(self) -> None:
        self._show_output()
        help_text = "**Available Commands:**\n"
        for cmd in self._builtin_commands:
            help_text += f"- `/{cmd.command}` - {cmd.help}\n"
        await self._get_output().post_note(help_text)

    async def action_cancel(self) -> None:
        """Cancel the current agent operation and preserve context."""
        if not self._state.agent or self._state.phase != PlannerPhase.PROCESSING:
            return

        if self._state.accumulated_response:
            partial_content = "".join(self._state.accumulated_response) + "\n\n*[interrupted]*"
            self._state.conversation_history.append(
                ChatMessage(
                    role=ChatRole.ASSISTANT,
                    content=partial_content,
                    timestamp=datetime.now(),
                )
            )

        await self._state.agent.cancel()
        await self._get_output().post_note("*Interrupted by user*", classes="warning")

        self._state = self._state.transition("done")
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

        self._state = self._state.transition("refine")
        planner_input.add_class("-refining")
        self._update_status("refining", "Enhancing prompt...")

        try:
            if not self._state.refiner:
                agent_config = config.get_worker_agent()
                if agent_config is None:
                    agent_config = get_fallback_agent_config()
                self._state.refiner = PromptRefiner(
                    self.kagan_app.project_root, agent_config, self._agent_factory
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
            self._state = self._state.transition("done")
            planner_input.remove_class("-refining")
            self._focus_input()
            self._update_status("ready", "Press ? for help")

    async def action_to_board(self) -> None:
        """Navigate to the Kanban board.

        Handles both cases:
        - Normal flow: KanbanScreen is underneath, pop to it
        - Empty board boot: Only PlannerScreen exists, switch to KanbanScreen

        Note: Textual always has a default screen at the bottom of the stack,
        so we need to check for > 2 screens (default + PlannerScreen + KanbanScreen)
        to know if KanbanScreen is underneath.
        """
        from kagan.ui.screens.kanban import KanbanScreen

        has_kanban_underneath = any(
            isinstance(screen, KanbanScreen) for screen in self.app.screen_stack[:-1]
        )

        if has_kanban_underneath:
            self.app.pop_screen()
        else:
            self.app.switch_screen(KanbanScreen())

    def action_set_task_branch(self) -> None:
        self.notify("Select a task on the Kanban board to set its branch", severity="warning")

    async def action_set_default_branch(self) -> None:
        config = self.kagan_app.config
        current = config.general.default_base_branch

        branches = await list_local_branches(self.kagan_app.project_root)

        result = await self.app.push_screen(
            BaseBranchModal(
                branches=branches,
                current_value=current,
                title="Set Default Base Branch",
                description="Branch used for new tasks (e.g. main, develop):",
            )
        )

        if result is not None:
            config.general.default_base_branch = result
            await config.save(self.kagan_app.config_path)
            self.notify(f"Default branch set to: {result}")

    async def on_unmount(self) -> None:
        input_text = ""
        with suppress(NoMatches):
            planner_input = self.query_one("#planner-input", PlannerInput)
            input_text = planner_input.text

        if self._state.agent:
            self._state.agent.set_message_target(None)

        self.kagan_app.planner_state = PersistentPlannerState(
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


def _truncate_queue_payload(content: str, max_chars: int = 6000) -> str:
    """Bound queued planner context so follow-up prompts stay lightweight."""
    if len(content) <= max_chars:
        return content
    prefix = "[queued planner context truncated]\n"
    return f"{prefix}{content[-(max_chars - len(prefix)) :]}"
