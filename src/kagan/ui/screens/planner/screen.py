"""Planner screen for chat-first ticket creation."""

from __future__ import annotations

from contextlib import suppress
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING, ClassVar

from textual import events, on
from textual.binding import Binding, BindingType
from textual.containers import Vertical
from textual.css.query import NoMatches
from textual.message import Message
from textual.widgets import Footer, Static, TextArea

from kagan.acp import messages
from kagan.acp.agent import Agent
from kagan.agents.planner import build_planner_prompt, parse_plan, parse_todos
from kagan.agents.refiner import PromptRefiner
from kagan.config import get_fallback_agent_config
from kagan.constants import PLANNER_TITLE_MAX_LENGTH
from kagan.keybindings import PLANNER_BINDINGS
from kagan.limits import AGENT_TIMEOUT
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
from kagan.ui.screens.ticket_editor import TicketEditorScreen
from kagan.ui.widgets import EmptyState, StatusBar, StreamingOutput
from kagan.ui.widgets.header import KaganHeader
from kagan.ui.widgets.plan_approval import PlanApprovalWidget
from kagan.ui.widgets.slash_complete import SlashComplete

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.acp import protocol
    from kagan.database.models import Ticket

MIN_INPUT_HEIGHT = 1
MAX_INPUT_HEIGHT = 6


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
        # Check if slash complete mode is active (input starts with /)
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
    """Chat-first planner for creating tickets."""

    BINDINGS = PLANNER_BINDINGS

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self._state = PlannerState()
        self._message_handler = MessageHandler(self)

        # Agent-related state (not part of PlannerState to avoid circular issues)
        self._current_mode: str = ""
        self._available_modes: dict[str, messages.Mode] = {}
        self._available_commands: list[protocol.AvailableCommand] = []

        # UI-only state
        self._slash_complete: SlashComplete | None = None
        self._builtin_commands: list[SlashCommand] = [
            SlashCommand("clear", "Clear conversation and start fresh"),
            SlashCommand("help", "Show available commands"),
        ]

    def compose(self) -> ComposeResult:
        yield KaganHeader()
        with Vertical(id="planner-container"):
            yield Static("Plan Mode", id="planner-header")
            yield EmptyState()
            yield StreamingOutput(id="planner-output")
            with Vertical(id="planner-bottom"):
                yield StatusBar()
                yield PlannerInput("", id="planner-input", show_line_numbers=False)
        yield Footer()

    async def on_mount(self) -> None:
        self._update_status("initializing", "Initializing agent...")
        self._disable_input()

        # Restore session state if available
        persistent_state = self.kagan_app.planner_state
        if persistent_state is not None:
            await self._restore_state(persistent_state)
        else:
            await self._start_planner()

        self._focus_input()

    async def _restore_state(self, persistent: PersistentPlannerState) -> None:
        """Restore state from a previous session."""
        output = self._get_output()
        self._show_output()

        # Restore conversation history
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

        # Restore input text
        if persistent.input_text:
            planner_input = self.query_one("#planner-input", PlannerInput)
            planner_input.insert(persistent.input_text)

        # Restore pending plan
        if persistent.pending_plan:
            self._state.pending_plan = persistent.pending_plan
            self._state.has_pending_plan = True
            await output.post_plan_approval(persistent.pending_plan)

        # Restore agent if it exists
        if persistent.agent is not None:
            self._state.agent = persistent.agent
            self._state.agent.set_message_target(self)
            self._state.agent_ready = persistent.agent_ready
            self._state.refiner = persistent.refiner

            if self._state.agent_ready:
                self._enable_input()
                self._update_status("ready", "Press F1 for help")
        else:
            await self._start_planner()

    async def _start_planner(self) -> None:
        """Start a new planner agent."""
        config = self.kagan_app.config
        agent_config = config.get_worker_agent()
        if agent_config is None:
            agent_config = get_fallback_agent_config()

        agent = Agent(Path.cwd(), agent_config, read_only=True)
        agent.set_auto_approve(config.general.auto_approve)
        agent.start(self)

        self._state.agent = agent

    def _get_output(self) -> StreamingOutput:
        return self.query_one("#planner-output", StreamingOutput)

    def _show_output(self) -> None:
        if not self._state.has_output:
            self._state.has_output = True
            with suppress(NoMatches):
                self.query_one(EmptyState).add_class("hidden")
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

    # -------------------------------------------------------------------------
    # Submit / Send to Agent
    # -------------------------------------------------------------------------

    @on(PlannerInput.SubmitRequested)
    async def on_submit_requested(self, event: PlannerInput.SubmitRequested) -> None:
        if self._state.has_pending_plan:
            self.notify("Please approve or dismiss the pending plan first", severity="warning")
            return
        if not self._state.can_submit():
            return
        await self._submit_prompt()

    async def _submit_prompt(self) -> None:
        planner_input = self.query_one("#planner-input", PlannerInput)
        text = planner_input.text.strip()
        if not text:
            return

        # Transition to processing
        self._state = self._state.transition("submit")
        self._state.todos_displayed = False
        self._state.thinking_shown = False

        self._disable_input()
        planner_input.clear()
        self._update_status("thinking", "Processing...")

        self._show_output()
        output = self._get_output()
        output.reset_turn()

        await output.post_user_input(text)

        # Store user message
        self._state.conversation_history.append(
            ChatMessage(role="user", content=text, timestamp=datetime.now())
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

            # Store assistant response
            if self._state.accumulated_response:
                full_response = "".join(self._state.accumulated_response)
                todos = parse_todos(full_response) if self._state.todos_displayed else None
                self._state.conversation_history.append(
                    ChatMessage(
                        role="assistant",
                        content=full_response,
                        timestamp=datetime.now(),
                        todos=todos,
                    )
                )

            await self._try_create_tickets()
        except Exception as e:
            await self._get_output().post_note(f"Error: {e}", classes="error")
            self._state = self._state.transition("error")
            self._enable_input()
        else:
            if not self._state.has_pending_plan:
                self._state = self._state.transition("done")
                self._enable_input()

        self._update_status("ready", "Press F1 for help")

    async def _try_create_tickets(self) -> None:
        if not self._state.accumulated_response:
            return

        full_response = "".join(self._state.accumulated_response)
        tickets = parse_plan(full_response)
        if tickets:
            # Set pending plan, then transition (transition preserves has_pending_plan)
            self._state = self._state.with_pending_plan(tickets)
            self._state = self._state.transition("plan_received")
            await self._get_output().post_plan_approval(tickets)

    # -------------------------------------------------------------------------
    # ACP Message Handlers (delegated to MessageHandler)
    # -------------------------------------------------------------------------

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
        self._message_handler.handle_tool_call_update(message)

    @on(messages.AgentReady)
    async def on_agent_ready(self, message: messages.AgentReady) -> None:
        await self._message_handler.handle_agent_ready(message)

    @on(messages.AgentFail)
    async def on_agent_fail(self, message: messages.AgentFail) -> None:
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

    # -------------------------------------------------------------------------
    # Plan Approval Handlers
    # -------------------------------------------------------------------------

    @on(PlanApprovalWidget.Approved)
    async def on_plan_approved(self, event: PlanApprovalWidget.Approved) -> None:
        """Handle plan approval - create tickets."""
        self._state = self._state.transition("approved")
        output = self._get_output()
        created_tickets: list[tuple[str, str, str]] = []

        for ticket_data in event.tickets:
            try:
                ticket = await self.kagan_app.state_manager.create_ticket(ticket_data)
                self.notify(
                    f"Created: {ticket.title[:PLANNER_TITLE_MAX_LENGTH]}", severity="information"
                )
                created_tickets.append(
                    (
                        ticket.title,
                        ticket.ticket_type.value if ticket.ticket_type else "PAIR",
                        ticket.priority.label if ticket.priority else "Medium",
                    )
                )
            except Exception as e:
                self.notify(f"Failed to create ticket: {e}", severity="error")

        self._state.pending_plan = None
        self._state.has_pending_plan = False
        self._state.accumulated_response.clear()
        self._state = self._state.transition("done")

        if created_tickets:
            lines = [f"[bold]Created {len(created_tickets)} ticket(s):[/bold]", ""]
            for title, ticket_type, priority in created_tickets:
                display_title = title[:60] + "..." if len(title) > 60 else title
                lines.append(
                    f"  - [dim]{ticket_type}[/dim] {display_title} [italic]({priority})[/italic]"
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
        """Handle request to edit tickets before approval."""
        self.app.push_screen(
            TicketEditorScreen(event.tickets),
            self._on_ticket_editor_result,
        )

    async def _on_ticket_editor_result(self, result: list[Ticket] | None) -> None:
        """Handle result from ticket editor."""
        if result is None:
            if self._state.pending_plan:
                await self._get_output().post_plan_approval(self._state.pending_plan)
        else:
            self._state.pending_plan = result
            await self._get_output().post_plan_approval(result)

    # -------------------------------------------------------------------------
    # Slash Commands
    # -------------------------------------------------------------------------

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

    async def _execute_clear(self) -> None:
        """Reset planner session completely."""
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
        self.notify("Conversation cleared")

    async def _execute_help(self) -> None:
        help_text = "**Available Commands:**\n"
        for cmd in self._builtin_commands:
            help_text += f"- `/{cmd.command}` - {cmd.help}\n"
        await self._get_output().post_note(help_text)

    # -------------------------------------------------------------------------
    # Actions
    # -------------------------------------------------------------------------

    async def action_cancel(self) -> None:
        """Cancel the current agent operation and preserve context."""
        if not self._state.agent or self._state.phase != PlannerPhase.PROCESSING:
            return

        if self._state.accumulated_response:
            partial_content = "".join(self._state.accumulated_response) + "\n\n*[interrupted]*"
            todos = parse_todos(partial_content) if self._state.todos_displayed else None
            self._state.conversation_history.append(
                ChatMessage(
                    role="assistant",
                    content=partial_content,
                    timestamp=datetime.now(),
                    todos=todos,
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
                self._state.refiner = PromptRefiner(Path.cwd(), agent_config)

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
            self._update_status("ready", "Press F1 for help")

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

        # Check if there's a KanbanScreen in the stack (underneath PlannerScreen)
        has_kanban_underneath = any(
            isinstance(screen, KanbanScreen) for screen in self.app.screen_stack[:-1]
        )

        if has_kanban_underneath:
            self.app.pop_screen()
        else:
            # Empty board boot case - no KanbanScreen underneath, switch to it
            self.app.switch_screen(KanbanScreen())

    # -------------------------------------------------------------------------
    # Lifecycle
    # -------------------------------------------------------------------------

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
            agent=self._state.agent,
            refiner=self._state.refiner,
            is_running=self._state.agent is not None,
            agent_ready=self._state.agent_ready,
        )
