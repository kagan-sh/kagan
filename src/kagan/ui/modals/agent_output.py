"""Modal for watching AUTO task agent progress."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, cast

from textual import on
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Label, Rule, TabbedContent, TabPane

from kagan.acp import messages
from kagan.acp.messages import Answer
from kagan.constants import MODAL_TITLE_MAX_LENGTH
from kagan.core.models.enums import TaskStatus
from kagan.keybindings import AGENT_OUTPUT_BINDINGS
from kagan.ui.utils.clipboard import copy_with_notification
from kagan.ui.widgets import StreamingOutput

if TYPE_CHECKING:
    from acp.schema import AvailableCommand
    from textual.app import ComposeResult

    from kagan.acp.agent import Agent
    from kagan.adapters.db.schema import AgentTurn
    from kagan.app import KaganApp
    from kagan.core.models.entities import Task


class AgentOutputModal(ModalScreen[None]):
    """Modal for watching an AUTO task's agent progress in real-time.

    Supports two modes:
    - IN_PROGRESS: Single streaming output showing live agent work
    - REVIEW: Tabbed interface showing Implementation logs + Review logs
    """

    BINDINGS = AGENT_OUTPUT_BINDINGS

    def __init__(
        self,
        task: Task,
        agent: Agent | None,
        iteration: int = 0,
        review_agent: Agent | None = None,
        is_reviewing: bool = False,
        historical_logs: dict[str, list[AgentTurn]] | None = None,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self._task_model = task
        self._agent = agent
        self._iteration = iteration
        self._review_agent = review_agent
        self._is_reviewing = is_reviewing
        self._historical_logs = historical_logs or {}
        self._show_tabs = task.status == TaskStatus.REVIEW
        self._current_mode: str = ""
        self._available_modes: dict[str, messages.Mode] = {}
        self._available_commands: list[AvailableCommand] = []
        self._review_loaded: bool = False

    def compose(self) -> ComposeResult:
        with Vertical(id="agent-output-container"):
            yield Label(
                f"AUTO: {self._task_model.title[:MODAL_TITLE_MAX_LENGTH]}",
                classes="modal-title",
            )

            if self._show_tabs:
                # REVIEW status: Tabbed interface
                status_text = "Reviewing..." if self._is_reviewing else "Review Complete"
                yield Label(
                    f"Task #{self._task_model.short_id} | {status_text}",
                    classes="modal-subtitle",
                )
                yield Rule()
                initial_tab = (
                    "review-tab"
                    if (self._review_agent or self._is_reviewing)
                    else "implementation-tab"
                )
                with TabbedContent(id="output-tabs", initial=initial_tab):
                    with TabPane("Implementation", id="implementation-tab"):
                        yield StreamingOutput(id="implementation-output")
                    with TabPane("Review", id="review-tab"):
                        yield StreamingOutput(id="review-output")
            else:
                # IN_PROGRESS status: Single output
                yield Label(
                    f"Task #{self._task_model.short_id} | Iteration {self._iteration}",
                    classes="modal-subtitle",
                )
                yield Rule()
                yield StreamingOutput(id="agent-output")

            yield Rule()

            # Different hint based on mode - use abbreviated forms to prevent truncation
            if self._is_reviewing:
                yield Label(
                    "Esc close (review continues)",
                    classes="modal-hint",
                )
                with Horizontal(classes="button-row"):
                    yield Button("Close", id="close-btn")
            else:
                yield Label(
                    "c cancel │ Esc close (agent continues) │ y copy",
                    classes="modal-hint",
                )
                with Horizontal(classes="button-row"):
                    yield Button("Cancel Agent", variant="error", id="cancel-btn")
                    yield Button("Close", id="close-btn")
        yield Footer(show_command_palette=False)

    async def on_mount(self) -> None:
        """Set up agent connections and load historical logs."""
        if self._show_tabs:
            await self._setup_tabbed_mode()
        else:
            await self._setup_single_mode()

    async def _setup_tabbed_mode(self) -> None:
        """Set up tabbed mode for REVIEW status."""
        # Load historical implementation logs
        impl_output = self.query_one("#implementation-output", StreamingOutput)
        impl_logs = self._historical_logs.get("implementation", [])

        if impl_logs:
            for log_entry in impl_logs:
                await self._load_historical_log(impl_output, log_entry)
        else:
            await impl_output.post_note("No implementation logs available", classes="warning")

        # Set up review tab
        review_output = self.query_one("#review-output", StreamingOutput)

        if self._review_agent:
            self._review_loaded = True
            # Live review agent - connect for streaming
            self._review_agent.set_message_target(self)
            await review_output.post_note("Connected to review agent stream", classes="info")
            return

        if self._is_reviewing:
            self._review_loaded = True
            await review_output.post_note("Review in progress...", classes="info")
            return

        self._review_loaded = False
        await review_output.post_note(
            "Review output is available on demand. Switch to this tab to load.",
            classes="info",
        )

    @on(TabbedContent.TabActivated, "#output-tabs")
    async def _on_output_tab_activated(self, event: TabbedContent.TabActivated) -> None:
        if event.tab.id == "review-tab":
            await self._ensure_review_loaded()

    async def _ensure_review_loaded(self) -> None:
        if self._review_loaded:
            return
        review_output = self.query_one("#review-output", StreamingOutput)
        review_logs = self._historical_logs.get("review", [])
        if review_logs:
            for log_entry in review_logs:
                await self._load_historical_log(
                    review_output, log_entry, show_iteration_header=False
                )
        else:
            if self._task_model.checks_passed is not None:
                status = "✓ Passed" if self._task_model.checks_passed else "✗ Failed"
                summary = self._task_model.review_summary or "No details"
                await review_output.post_note(f"Review {status}", classes="info")
                await review_output.post_response(summary)
            else:
                await review_output.post_note("Review not yet completed", classes="warning")
        self._review_loaded = True

    async def _setup_single_mode(self) -> None:
        """Set up single output mode for IN_PROGRESS status."""
        output = self._get_output()
        if self._agent:
            self._agent.set_message_target(self)
            await output.post_note("Connected to agent stream", classes="info")
        else:
            await output.post_note("No agent currently running", classes="warning")

    def on_unmount(self) -> None:
        """Remove message target when modal closes."""
        if self._agent:
            self._agent.set_message_target(None)
        if self._review_agent:
            self._review_agent.set_message_target(None)

    def _get_output(self) -> StreamingOutput:
        """Get the appropriate streaming output widget."""
        if self._show_tabs:
            # In tabbed mode, stream to review output (for live review)
            return self.query_one("#review-output", StreamingOutput)
        return self.query_one("#agent-output", StreamingOutput)

    async def _load_historical_log(
        self, output: StreamingOutput, log_entry: AgentTurn, show_iteration_header: bool = True
    ) -> None:
        """Load a historical log entry into a StreamingOutput widget."""
        if show_iteration_header:
            await output.post_note(f"--- Iteration {log_entry.sequence} ---", classes="info")

        try:
            data = json.loads(log_entry.content)
            messages = data.get("messages", [])

            for msg in messages:
                msg_type = msg.get("type", "")
                if msg_type == "response":
                    content = msg.get("content", "")
                    if content:
                        await output.post_response(content)
                elif msg_type == "thinking":
                    content = msg.get("content", "")
                    if content:
                        await output.post_thought(content)
                elif msg_type == "tool_call":
                    tool_id = msg.get("id", "unknown")
                    title = msg.get("title", "Tool call")
                    kind = msg.get("kind", "")
                    await output.post_tool_call(tool_id, title, kind)
                elif msg_type == "tool_call_update":
                    tool_id = msg.get("id", "unknown")
                    status = msg.get("status", "")
                    if status:
                        output.update_tool_status(tool_id, status)
                elif msg_type == "plan":
                    entries = msg.get("entries", [])
                    if entries:
                        await output.post_plan(entries)
                elif msg_type == "agent_ready":
                    await output.post_note("Agent ready", classes="success")
                elif msg_type == "agent_fail":
                    error_msg = msg.get("message", "Unknown error")
                    await output.post_note(f"Error: {error_msg}", classes="error")
                    details = msg.get("details")
                    if details:
                        await output.post_note(details)

            # If no messages were serialized, fall back to response_text
            if not messages:
                response_text = data.get("response_text", "")
                if response_text:
                    await output.post_response(response_text)

        except json.JSONDecodeError:
            await output.post_note(
                "Unsupported log format (expected JSON).",
                classes="warning",
            )

    # ACP Message handlers

    @on(messages.AgentUpdate)
    async def on_agent_update(self, message: messages.AgentUpdate) -> None:
        """Handle agent text output."""
        await self._get_output().post_response(message.text)

    @on(messages.Thinking)
    async def on_agent_thinking(self, message: messages.Thinking) -> None:
        """Handle agent thinking/reasoning."""
        await self._get_output().post_thought(message.text)

    @on(messages.ToolCall)
    async def on_tool_call(self, message: messages.ToolCall) -> None:
        """Handle tool call start."""
        tool_id = message.tool_call.tool_call_id
        title = message.tool_call.title
        kind = message.tool_call.kind or ""
        await self._get_output().post_tool_call(tool_id, title, kind)

    @on(messages.ToolCallUpdate)
    async def on_tool_call_update(self, message: messages.ToolCallUpdate) -> None:
        """Handle tool call update."""
        tool_id = message.update.tool_call_id
        status = message.update.status or ""
        if status:
            self._get_output().update_tool_status(tool_id, status)

    @on(messages.AgentReady)
    async def on_agent_ready(self, message: messages.AgentReady) -> None:
        """Handle agent ready."""
        await self._get_output().post_note("Agent ready", classes="success")

    @on(messages.AgentFail)
    async def on_agent_fail(self, message: messages.AgentFail) -> None:
        """Handle agent failure."""
        output = self._get_output()
        await output.post_note(f"Error: {message.message}", classes="error")
        if message.details:
            await output.post_note(message.details)

    @on(messages.Plan)
    async def on_plan(self, message: messages.Plan) -> None:
        """Display plan entries from agent."""
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
    def on_request_permission(self, message: messages.RequestPermission) -> None:
        """Auto-approve permissions in watch mode (passive observation)."""
        # Find allow_once option (prefer) or allow_always
        for opt in message.options:
            if opt.kind == "allow_once":
                message.result_future.set_result(Answer(opt.option_id))
                return
        for opt in message.options:
            if "allow" in opt.kind:
                message.result_future.set_result(Answer(opt.option_id))
                return
        # Fallback to first option if no allow options exist
        if message.options:
            message.result_future.set_result(Answer(message.options[0].option_id))

    # Button handlers

    @on(Button.Pressed, "#cancel-btn")
    async def on_cancel_btn(self) -> None:
        """Cancel the agent."""
        await self.action_cancel_agent()

    @on(Button.Pressed, "#close-btn")
    def on_close_btn(self) -> None:
        """Close the modal."""
        self.action_close()

    # Actions

    async def action_cancel_agent(self) -> None:
        """Stop agent completely and move task to BACKLOG."""
        # Prevent cancel during review
        if self._is_reviewing:
            self.notify("Cannot cancel during review", severity="warning")
            return

        app = cast("KaganApp", self.app)
        automation = app.ctx.automation_service
        if automation is None:
            self.notify("Automation service unavailable", severity="error")
            return

        if automation.is_running(self._task_model.id):
            # Stop both the agent process and the task loop task
            await automation.stop_task(self._task_model.id)
            await self._get_output().post_note("Agent stopped", classes="warning")

            # Move task to BACKLOG to prevent auto-restart
            await app.ctx.task_service.move(self._task_model.id, TaskStatus.BACKLOG)
            await self._get_output().post_note(
                "Task moved to BACKLOG (agent won't auto-restart)", classes="info"
            )
            self.notify("Agent stopped, task moved to BACKLOG")
        else:
            self.notify("No agent running for this task", severity="warning")

    def action_close(self) -> None:
        """Close the modal (agent continues running in background)."""
        self.dismiss(None)

    def action_copy(self) -> None:
        """Copy agent output content to clipboard."""
        output = self._get_output()
        content = output.get_text_content()
        copy_with_notification(self.app, content, "Agent output")
