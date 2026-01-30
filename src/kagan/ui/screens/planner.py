"""Planner screen for chat-first ticket creation."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, ClassVar

from textual import events, on
from textual.binding import Binding, BindingType
from textual.containers import Vertical
from textual.message import Message
from textual.widgets import Footer, Static, TextArea

from kagan.acp import messages
from kagan.acp.agent import Agent
from kagan.agents.planner import build_planner_prompt, parse_plan, parse_todos
from kagan.agents.refiner import PromptRefiner
from kagan.config import get_fallback_agent_config
from kagan.constants import PLANNER_TITLE_MAX_LENGTH
from kagan.keybindings import PLANNER_BINDINGS, to_textual_bindings
from kagan.limits import AGENT_TIMEOUT
from kagan.ui.screens.approval import ApprovalScreen
from kagan.ui.screens.base import KaganScreen
from kagan.ui.widgets import EmptyState, StatusBar, StreamingOutput
from kagan.ui.widgets.header import KaganHeader

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.acp import protocol
    from kagan.database.models import TicketCreate

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

    def action_insert_newline(self) -> None:
        """Insert a newline character."""
        self.insert("\n")

    async def _on_key(self, event: events.Key) -> None:
        if event.key == "enter":
            event.prevent_default()
            event.stop()
            self.post_message(self.SubmitRequested(self.text))
            return
        await super()._on_key(event)


class PlannerScreen(KaganScreen):
    """Chat-first planner for creating tickets."""

    BINDINGS = to_textual_bindings(PLANNER_BINDINGS)

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self._agent: Agent | None = None
        self._refiner: PromptRefiner | None = None
        self._is_running = False
        self._is_processing = False
        self._is_refining = False
        self._accumulated_response: list[str] = []
        self._agent_ready = False
        self._has_agent_output = False
        self._current_mode: str = ""
        self._available_modes: dict[str, messages.Mode] = {}
        self._available_commands: list[protocol.AvailableCommand] = []
        self._todos_displayed = False
        self._thinking_shown = False

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
        from contextlib import suppress

        from textual.css.query import NoMatches

        with suppress(NoMatches):
            self.query_one(StatusBar).update_status("initializing", "Initializing agent...")

        planner_input = self.query_one("#planner-input", PlannerInput)
        planner_input.add_class("-disabled")
        planner_input.read_only = True

        await self._start_planner()
        planner_input.focus()

    async def _start_planner(self) -> None:
        config = self.kagan_app.config
        agent_config = config.get_worker_agent()
        if agent_config is None:
            agent_config = get_fallback_agent_config()

        self._agent = Agent(Path.cwd(), agent_config, read_only=True)
        self._agent.set_auto_approve(config.general.auto_approve)
        self._agent.start(self)
        self._is_running = True

    def _get_output(self) -> StreamingOutput:
        return self.query_one("#planner-output", StreamingOutput)

    def _show_output(self) -> None:
        from contextlib import suppress

        from textual.css.query import NoMatches

        if not self._has_agent_output:
            self._has_agent_output = True
            with suppress(NoMatches):
                self.query_one(EmptyState).add_class("hidden")
            with suppress(NoMatches):
                self._get_output().add_class("visible")

    @on(PlannerInput.SubmitRequested)
    async def on_submit_requested(self, event: PlannerInput.SubmitRequested) -> None:
        if not self._agent_ready or self._is_processing:
            return
        await self._submit_prompt()

    async def _submit_prompt(self) -> None:
        from contextlib import suppress

        from textual.css.query import NoMatches

        planner_input = self.query_one("#planner-input", PlannerInput)
        text = planner_input.text.strip()
        if not text:
            return

        # Lock input while processing
        self._is_processing = True
        planner_input.add_class("-disabled")
        planner_input.read_only = True
        planner_input.clear()
        self._todos_displayed = False
        self._thinking_shown = False

        with suppress(NoMatches):
            self.query_one(StatusBar).update_status("thinking", "Processing...")

        self._show_output()
        output = self._get_output()

        # Add turn separator if there was previous output, then reset for new turn
        has_previous = output.phase != "idle" or len(list(output.children)) > 0
        output.reset_turn()
        if has_previous:
            await output.post_turn_separator()

        await output.post_user_input(text)

        if self._agent and self._is_running:
            self.run_worker(self._send_to_agent(text))
        else:
            self._unlock_input()
            self.notify("Planner not running", severity="warning")

    def _unlock_input(self) -> None:
        """Re-enable input after processing completes."""
        from contextlib import suppress

        from textual.css.query import NoMatches

        self._is_processing = False
        with suppress(NoMatches):
            planner_input = self.query_one("#planner-input", PlannerInput)
            planner_input.remove_class("-disabled")
            planner_input.read_only = False
            planner_input.focus()

    async def _send_to_agent(self, text: str) -> None:
        from contextlib import suppress

        from textual.css.query import NoMatches

        if not self._agent:
            self._unlock_input()
            return

        self._accumulated_response.clear()
        prompt = build_planner_prompt(text)

        try:
            await self._agent.wait_ready(timeout=AGENT_TIMEOUT)
            await self._agent.send_prompt(prompt)
            await self._try_create_tickets()
        except Exception as e:
            await self._get_output().post_note(f"Error: {e}", classes="error")
        finally:
            with suppress(NoMatches):
                self.query_one(StatusBar).update_status("ready", "Press F1 for help")
            self._unlock_input()

    # ACP Message handlers

    @on(messages.AgentUpdate)
    async def on_agent_update(self, message: messages.AgentUpdate) -> None:
        self._show_output()
        self._accumulated_response.append(message.text)
        # StreamingOutput now filters XML internally
        await self._get_output().post_response(message.text)

        # Parse and update plan display (update in-place if exists)
        if not self._todos_displayed:
            full_response = "".join(self._accumulated_response)
            todos = parse_todos(full_response)
            if todos:
                self._todos_displayed = True
                output = self._get_output()
                if output._plan_display is not None:
                    output._plan_display.update_entries(todos)
                else:
                    await output.post_plan(todos)

    @on(messages.Thinking)
    async def on_agent_thinking(self, message: messages.Thinking) -> None:
        self._show_output()
        if not self._thinking_shown:
            self._thinking_shown = True
            await self._get_output().post_thinking_indicator()
        await self._get_output().post_thought(message.text)

    @on(messages.ToolCall)
    async def on_tool_call(self, message: messages.ToolCall) -> None:
        self._show_output()
        tool_id = str(message.tool_call.get("id", "unknown"))
        title = str(message.tool_call.get("title", "Tool call"))
        kind = str(message.tool_call.get("kind", ""))
        await self._get_output().post_tool_call(tool_id, title, kind)

    @on(messages.ToolCallUpdate)
    async def on_tool_call_update(self, message: messages.ToolCallUpdate) -> None:
        tool_id = str(message.update.get("id", "unknown"))
        status = str(message.update.get("status", ""))
        if status:
            self._get_output().update_tool_status(tool_id, status)

    @on(messages.AgentReady)
    async def on_agent_ready(self, message: messages.AgentReady) -> None:
        from contextlib import suppress

        from textual.css.query import NoMatches

        self._agent_ready = True

        planner_input = self.query_one("#planner-input", PlannerInput)
        planner_input.remove_class("-disabled")
        planner_input.read_only = False

        with suppress(NoMatches):
            self.query_one(StatusBar).update_status("ready", "Press F1 for help")

        planner_input.focus()

    @on(messages.AgentFail)
    async def on_agent_fail(self, message: messages.AgentFail) -> None:
        from contextlib import suppress

        from textual.css.query import NoMatches

        self._is_running = False

        with suppress(NoMatches):
            self.query_one(StatusBar).update_status("error", f"Error: {message.message}")

        planner_input = self.query_one("#planner-input", PlannerInput)
        planner_input.add_class("-disabled")
        planner_input.read_only = True

        self._show_output()
        output = self._get_output()
        await output.post_note(f"Error: {message.message}", classes="error")
        if message.details:
            await output.post_note(message.details)

    @on(messages.Plan)
    async def on_plan(self, message: messages.Plan) -> None:
        """Display plan entries from agent."""
        self._show_output()
        await self._get_output().post_plan(message.entries)

    @on(messages.SetModes)
    def on_set_modes(self, message: messages.SetModes) -> None:
        """Store available modes from agent."""
        self._current_mode = message.current_mode
        self._available_modes = message.modes

    @on(messages.ModeUpdate)
    def on_mode_update(self, message: messages.ModeUpdate) -> None:
        """Track mode changes from agent."""
        self._current_mode = message.current_mode

    @on(messages.AvailableCommandsUpdate)
    def on_commands_update(self, message: messages.AvailableCommandsUpdate) -> None:
        """Store available slash commands from agent."""
        self._available_commands = message.commands

    @on(messages.RequestPermission)
    async def on_request_permission(self, message: messages.RequestPermission) -> None:
        """Display inline permission prompt when agent requests it."""
        self._show_output()
        await self._get_output().post_permission_request(
            message.options,
            message.tool_call,
            message.result_future,
            timeout=300.0,
        )

    async def _try_create_tickets(self) -> None:
        if not self._accumulated_response:
            return

        full_response = "".join(self._accumulated_response)
        tickets = parse_plan(full_response)
        if tickets:
            self.app.push_screen(ApprovalScreen(tickets), self._on_approval_result)

    async def _on_approval_result(self, result: list[TicketCreate] | None) -> None:
        output = self._get_output()

        if result is None:
            self._accumulated_response.clear()
            await output.clear()
            await output.post_note("Plan cancelled. Describe what you want to build.")
            return

        created_count = 0
        for ticket_data in result:
            try:
                ticket = await self.kagan_app.state_manager.create_ticket(ticket_data)
                self.notify(
                    f"Created: {ticket.title[:PLANNER_TITLE_MAX_LENGTH]}", severity="information"
                )
                created_count += 1
            except Exception as e:
                self.notify(f"Failed to create ticket: {e}", severity="error")

        if created_count > 0:
            await output.post_note(f"Created {created_count} ticket(s)", classes="success")
            await self.action_to_board()

    async def action_cancel(self) -> None:
        if self._agent and self._is_running:
            await self._agent.cancel()
            self.notify("Sent cancel request")

    async def action_refine(self) -> None:
        """Refine the current prompt using dedicated ACP agent (Ctrl+E)."""
        from contextlib import suppress

        from textual.css.query import NoMatches

        # Guard against invalid states
        if not self._agent_ready or self._is_processing or self._is_refining:
            return

        planner_input = self.query_one("#planner-input", PlannerInput)
        text = planner_input.text.strip()

        if not text:
            self.notify("Nothing to enhance", severity="warning")
            return

        # Check refinement config
        config = self.kagan_app.config
        refinement_config = config.refinement

        if not refinement_config.enabled:
            self.notify("Prompt refinement is disabled", severity="warning")
            return

        # Skip refinement for short inputs
        if len(text) < refinement_config.skip_length_under:
            self.notify("Input too short to enhance", severity="warning")
            return

        # Skip refinement for command-like inputs
        if any(text.startswith(prefix) for prefix in refinement_config.skip_prefixes):
            self.notify("Commands cannot be enhanced", severity="warning")
            return

        # Enter refining state
        self._is_refining = True
        planner_input.add_class("-refining")

        with suppress(NoMatches):
            self.query_one(StatusBar).update_status("refining", "Enhancing prompt...")

        try:
            # Lazily create refiner with dedicated agent
            if not self._refiner:
                agent_config = config.get_worker_agent()
                if agent_config is None:
                    agent_config = get_fallback_agent_config()
                self._refiner = PromptRefiner(Path.cwd(), agent_config)

            # Refine the prompt
            refined = await self._refiner.refine(text)

            # Replace input content with refined prompt
            planner_input.clear()
            planner_input.insert(refined)
            self.notify("Prompt enhanced - review and press Enter")

        except TimeoutError:
            self.notify("Refinement timed out", severity="error")
        except Exception as e:
            self.notify(f"Refinement failed: {e}", severity="error")
        finally:
            self._is_refining = False
            planner_input.remove_class("-refining")
            planner_input.focus()

            with suppress(NoMatches):
                self.query_one(StatusBar).update_status("ready", "Press F1 for help")

    async def action_to_board(self) -> None:
        from kagan.ui.screens.kanban import KanbanScreen

        await self.app.switch_screen(KanbanScreen())

    async def on_unmount(self) -> None:
        if self._refiner:
            await self._refiner.stop()
        if self._agent:
            await self._agent.stop()
            self._is_running = False
