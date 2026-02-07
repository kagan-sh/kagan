"""Modal for watching AUTO task agent progress."""

from __future__ import annotations

import inspect
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
from kagan.ui.utils.agent_exit import is_graceful_agent_termination
from kagan.ui.utils.clipboard import copy_with_notification
from kagan.ui.widgets import ChatPanel

if TYPE_CHECKING:
    from acp.schema import AvailableCommand
    from textual.app import ComposeResult

    from kagan.acp.agent import Agent
    from kagan.app import KaganApp
    from kagan.core.models.entities import Task
    from kagan.services.queued_messages import QueuedMessageService
    from kagan.ui.widgets.streaming_output import StreamingOutput


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
        execution_id: str | None = None,
        run_count: int = 0,
        review_agent: Agent | None = None,
        is_reviewing: bool = False,
        is_running: bool = True,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self._task_model = task
        self._agent = agent
        self._run_count = run_count
        self._execution_id = execution_id
        self._review_agent = review_agent
        self._is_reviewing = is_reviewing
        self._is_running = is_running
        self._tabbed = task.status == TaskStatus.REVIEW or is_reviewing
        self._current_mode: str = ""
        self._available_modes: dict[str, messages.Mode] = {}
        self._available_commands: list[AvailableCommand] = []

    def compose(self) -> ComposeResult:
        with Vertical(id="agent-output-container"):
            yield Label(
                f"AUTO: {self._task_model.title[:MODAL_TITLE_MAX_LENGTH]}",
                classes="modal-title",
            )
            run_label = f"Run {self._run_count}" if self._run_count > 0 else "Run"
            yield Label(
                f"Task #{self._task_model.short_id} | {run_label}",
                classes="modal-subtitle",
            )
            yield Rule()

            if self._tabbed:
                with TabbedContent():
                    with TabPane("Implementation", id="tab-impl"):
                        yield ChatPanel(
                            self._execution_id,
                            allow_input=False,
                            output_id="implementation-output",
                            id="impl-chat",
                        )
                    with TabPane("Review", id="tab-review"):
                        yield ChatPanel(
                            None,
                            allow_input=False,
                            output_id="review-output",
                            id="review-chat",
                        )
            else:
                yield ChatPanel(
                    self._execution_id,
                    allow_input=True,
                    output_id="agent-output",
                    id="agent-chat",
                )

            yield Rule()

            if self._tabbed:
                yield Label(
                    "Esc close │ y copy",
                    classes="modal-hint",
                )
                with Horizontal(classes="button-row"):
                    yield Button("Close", id="close-btn")
            else:
                # Show different buttons based on whether agent is running
                if self._is_running:
                    yield Label(
                        "c cancel │ Esc close (agent continues) │ y copy",
                        classes="modal-hint",
                    )
                    with Horizontal(classes="button-row"):
                        yield Button("Cancel Agent", variant="error", id="cancel-btn")
                        yield Button("Close", id="close-btn")
                else:
                    # Agent stopped but task still in progress - show start button
                    yield Label(
                        "Esc close │ y copy",
                        classes="modal-hint",
                    )
                    with Horizontal(classes="button-row"):
                        yield Button("Start Agent", variant="success", id="start-btn")
                        yield Button("Close", id="close-btn")
        yield Footer(show_command_palette=False)

    async def on_mount(self) -> None:
        """Set up agent connections and load historical logs."""
        if self._tabbed:
            await self._mount_tabbed()
        else:
            await self._mount_single()

    async def _mount_single(self) -> None:
        panel = self.query_one("#agent-chat", ChatPanel)
        output = panel.output
        if self._agent:
            # Live agent: buffer holds the full session history — replay rebuilds the view.
            # Skip load_logs(); in-flight executions have no DB logs yet.
            self._agent.set_message_target(self)
        else:
            # No live agent: load persisted logs from a completed execution.
            await panel.load_logs()
            await output.post_note("No agent currently running", classes="warning")

        panel.set_send_handler(self._send_message)
        panel.set_remove_handler(self._remove_queued_message)
        panel.set_get_queued_handler(self._get_queued_messages)
        # Load existing queued messages
        await panel.refresh_queued_messages()

    def _get_queue_service(self) -> QueuedMessageService | None:
        app = cast("KaganApp", self.app)
        service = getattr(app.ctx, "queued_message_service", None)
        if service is None:
            return None
        return cast("QueuedMessageService", service)

    async def _send_message(self, content: str) -> None:
        service = self._get_queue_service()
        if service is None:
            raise RuntimeError("Message queue unavailable")
        result = service.queue_message(self._task_model.id, content, lane="implementation")
        if inspect.isawaitable(result):
            await result

    async def _get_queued_messages(self) -> list:
        """Get all queued messages for display."""
        service = self._get_queue_service()
        if service is None:
            return []
        return await service.get_queued(self._task_model.id, lane="implementation")

    async def _remove_queued_message(self, index: int) -> bool:
        """Remove a queued message by index."""
        service = self._get_queue_service()
        if service is None:
            return False
        return await service.remove_message(self._task_model.id, index, lane="implementation")

    async def _mount_tabbed(self) -> None:
        impl_panel = self.query_one("#impl-chat", ChatPanel)
        review_panel = self.query_one("#review-chat", ChatPanel)

        if not self._execution_id:
            await impl_panel.output.post_note("No execution logs available", classes="warning")
            await review_panel.output.post_note("No review logs available", classes="warning")
            return

        app = cast("KaganApp", self.app)

        execution = await app.ctx.execution_service.get_execution(self._execution_id)
        has_review_result = False
        if execution and execution.metadata_:
            has_review_result = "review_result" in execution.metadata_

        entries = await app.ctx.execution_service.get_log_entries(self._execution_id)

        if not entries:
            await impl_panel.output.post_note("No execution logs available", classes="warning")
            await review_panel.output.post_note("No review logs available", classes="warning")
            return

        if has_review_result and len(entries) > 1:
            impl_entries = entries[:-1]
            review_entries = entries[-1:]
        else:
            impl_entries = entries
            review_entries = []

        for entry in impl_entries:
            if entry.logs:
                impl_panel.set_execution_id(None)
                for line in entry.logs.splitlines():
                    await impl_panel._render_log_line(line)

        if review_entries:
            for entry in review_entries:
                if entry.logs:
                    for line in entry.logs.splitlines():
                        await review_panel._render_log_line(line)
        elif self._is_reviewing and self._review_agent:
            self._review_agent.set_message_target(self)
            await review_panel.output.post_note("Connected to review agent stream", classes="info")
        else:
            await review_panel.output.post_note("No review logs available", classes="warning")

        if self._agent:
            self._agent.set_message_target(self)

    def on_unmount(self) -> None:
        """Remove message target when modal closes."""
        if self._agent:
            self._agent.set_message_target(None)
        if self._review_agent:
            self._review_agent.set_message_target(None)

    def _get_active_output(self) -> StreamingOutput:
        if self._tabbed:
            if self._is_reviewing and self._review_agent:
                return self.query_one("#review-chat", ChatPanel).output
            return self.query_one("#impl-chat", ChatPanel).output
        return self.query_one("#agent-chat", ChatPanel).output

    @on(messages.AgentUpdate)
    async def on_agent_update(self, message: messages.AgentUpdate) -> None:
        """Handle agent text output."""
        await self._get_active_output().post_response(message.text)

    @on(messages.Thinking)
    async def on_agent_thinking(self, message: messages.Thinking) -> None:
        """Handle agent thinking/reasoning."""
        await self._get_active_output().post_thought(message.text)

    @on(messages.ToolCall)
    async def on_tool_call(self, message: messages.ToolCall) -> None:
        """Handle tool call start."""
        await self._get_active_output().upsert_tool_call(message.tool_call)

    @on(messages.ToolCallUpdate)
    async def on_tool_call_update(self, message: messages.ToolCallUpdate) -> None:
        """Handle tool call update."""
        await self._get_active_output().apply_tool_call_update(message.update, message.tool_call)

    @on(messages.AgentReady)
    async def on_agent_ready(self, message: messages.AgentReady) -> None:
        """Handle agent ready."""
        await self._get_active_output().post_note("Agent ready", classes="success")

    @on(messages.AgentFail)
    async def on_agent_fail(self, message: messages.AgentFail) -> None:
        """Handle agent failure."""
        output = self._get_active_output()
        if is_graceful_agent_termination(message.message):
            await output.post_note(
                "Agent stream ended by cancellation (SIGTERM).",
                classes="dismissed",
            )
            return
        await output.post_note(f"Error: {message.message}", classes="error")
        if message.details:
            await output.post_note(message.details)

    @on(messages.Plan)
    async def on_plan(self, message: messages.Plan) -> None:
        """Display plan entries from agent."""
        await self._get_active_output().post_plan(message.entries)

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
        for opt in message.options:
            if opt.kind == "allow_once":
                message.result_future.set_result(Answer(opt.option_id))
                return
        for opt in message.options:
            if "allow" in opt.kind:
                message.result_future.set_result(Answer(opt.option_id))
                return

        if message.options:
            message.result_future.set_result(Answer(message.options[0].option_id))

    @on(Button.Pressed, "#cancel-btn")
    async def on_cancel_btn(self) -> None:
        """Cancel the agent."""
        await self.action_cancel_agent()

    @on(Button.Pressed, "#close-btn")
    def on_close_btn(self) -> None:
        """Close the modal."""
        self.action_close()

    @on(Button.Pressed, "#start-btn")
    async def on_start_btn(self) -> None:
        """Start the agent for this task."""
        await self.action_start_agent()

    async def action_cancel_agent(self) -> None:
        """Stop agent completely and move task to BACKLOG."""
        app = cast("KaganApp", self.app)
        automation = app.ctx.automation_service
        if automation is None:
            self.notify("Automation service unavailable", severity="error")
            return

        if automation.is_running(self._task_model.id):
            await automation.stop_task(self._task_model.id)
            output = self._get_active_output()
            await output.post_note("Agent stopped", classes="warning")

            await app.ctx.task_service.move(self._task_model.id, TaskStatus.BACKLOG)
            await output.post_note(
                "Task moved to BACKLOG (select task and press 'a' to restart)", classes="info"
            )
            self.notify("Agent stopped, task moved to BACKLOG")
        else:
            self.notify("No agent running for this task", severity="warning")

    async def action_start_agent(self) -> None:
        """Start the agent for the stopped task."""
        app = cast("KaganApp", self.app)
        automation = app.ctx.automation_service
        if automation is None:
            self.notify("Automation service unavailable", severity="error")
            return

        if automation.is_running(self._task_model.id):
            self.notify("Agent already running", severity="warning")
            return

        # Ensure workspace exists
        wt_path = await app.ctx.workspace_service.get_path(self._task_model.id)
        if wt_path is None:
            self.notify("No workspace configured for this task", severity="error")
            return

        self.notify("Starting agent...", severity="information")

        success = await automation.spawn_for_task(self._task_model)

        if success:
            # Update internal state and UI
            self._is_running = True
            self._agent = automation.get_running_agent(self._task_model.id)
            if self._agent:
                self._agent.set_message_target(self)

            output = self._get_active_output()
            await output.post_note("Agent started", classes="success")

            # Update buttons - remove start button, add cancel button
            await self._refresh_buttons()
        else:
            self.notify("Failed to start agent", severity="error")

    async def _refresh_buttons(self) -> None:
        """Refresh button row based on current running state."""
        button_row = self.query_one(".button-row", Horizontal)
        hint = self.query_one(".modal-hint", Label)

        # Clear existing buttons - must await removal to avoid duplicate IDs
        await button_row.remove_children()

        if self._is_running:
            hint.update("c cancel │ Esc close (agent continues) │ y copy")
            await button_row.mount(Button("Cancel Agent", variant="error", id="cancel-btn"))
            await button_row.mount(Button("Close", id="close-btn"))
        else:
            hint.update("Esc close │ y copy")
            await button_row.mount(Button("Start Agent", variant="success", id="start-btn"))
            await button_row.mount(Button("Close", id="close-btn"))

    def action_close(self) -> None:
        """Close the modal (agent continues running in background)."""
        self.dismiss(None)

    def action_copy(self) -> None:
        """Copy agent output content to clipboard."""
        output = self._get_active_output()
        content = output.get_text_content()
        copy_with_notification(self.app, content, "Agent output")
