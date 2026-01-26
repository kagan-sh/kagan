"""Main Kanban board screen."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

from textual.binding import Binding
from textual.containers import Container, Horizontal
from textual.screen import Screen
from textual.widgets import Footer, Static

from kagan.constants import COLUMN_ORDER
from kagan.database.models import Ticket, TicketCreate, TicketStatus, TicketUpdate
from kagan.ui.modals import (
    AgentOutputModal,
    ConfirmModal,
    ModalAction,
    TicketDetailsModal,
    TicketFormModal,
)
from kagan.ui.screens.chat import ChatScreen
from kagan.ui.widgets.card import TicketCard
from kagan.ui.widgets.column import KanbanColumn
from kagan.ui.widgets.header import KaganHeader

if TYPE_CHECKING:
    from textual import events
    from textual.app import ComposeResult

    from kagan.app import KaganApp, TicketChanged

# Minimum terminal size for proper display
MIN_WIDTH = 80
MIN_HEIGHT = 20

# Warning message for small terminal
SIZE_WARNING_MESSAGE = (
    f"Terminal too small\n\nMinimum size: {MIN_WIDTH}x{MIN_HEIGHT}\nPlease resize your terminal"
)


class KanbanScreen(Screen):
    """Main Kanban board screen with 4 columns."""

    BINDINGS = [
        Binding("q", "quit", "Quit", priority=True),
        Binding("h", "focus_left", "Left column", show=False),
        Binding("l", "focus_right", "Right column", show=False),
        Binding("j", "focus_down", "Down", show=False),
        Binding("k", "focus_up", "Up", show=False),
        Binding("n", "new_ticket", "New ticket"),
        Binding("e", "edit_ticket", "Edit ticket"),
        Binding("d", "delete_ticket", "Delete ticket"),
        Binding("right_square_bracket", "move_forward", "Move->"),
        Binding("left_square_bracket", "move_backward", "Move<-"),
        Binding("enter", "view_details", "View details"),
        Binding("o", "view_agent_output", "Agent output"),
        Binding("s", "start_agent", "Start agent"),
        Binding("x", "stop_agent", "Stop agent"),
        Binding("escape", "deselect", "Deselect", show=False),
        Binding("c", "open_chat", "Chat"),
    ]

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self._tickets: list[Ticket] = []
        self._pending_delete_ticket: Ticket | None = None
        self._editing_ticket_id: str | None = None

    @property
    def kagan_app(self) -> KaganApp:
        """Get the typed KaganApp instance."""
        return cast("KaganApp", self.app)

    def compose(self) -> ComposeResult:
        """Compose the Kanban board layout."""
        yield KaganHeader(ticket_count=0)

        with Container(classes="board-container"):
            with Horizontal(classes="board"):
                for status in COLUMN_ORDER:
                    yield KanbanColumn(status=status, tickets=[])

        with Container(classes="size-warning"):
            yield Static(
                SIZE_WARNING_MESSAGE,
                classes="size-warning-text",
            )

        yield Footer()

    async def on_mount(self) -> None:
        self._check_screen_size()
        await self._refresh_board()
        self._focus_first_card()
        self.set_interval(1.0, self._update_active_cards)

    def on_resize(self, event: events.Resize) -> None:
        """Handle terminal resize."""
        self._check_screen_size()

    async def on_screen_resume(self) -> None:
        """Refresh board when returning from another screen."""
        await self._refresh_board()

    def _check_screen_size(self) -> None:
        """Check if terminal is large enough and show warning if not."""
        size = self.app.size
        if size.width < MIN_WIDTH or size.height < MIN_HEIGHT:
            self.add_class("too-small")
        else:
            self.remove_class("too-small")

    def _update_active_cards(self) -> None:
        """Update agent active state and iteration info for all cards."""
        active_ids = set(self.kagan_app.agent_manager.list_active())

        # Build iteration info dict
        iterations: dict[str, str] = {}
        if hasattr(self.kagan_app, "_scheduler") and self.kagan_app._scheduler:
            scheduler = self.kagan_app._scheduler
            max_iter = self.kagan_app.config.general.max_iterations
            for ticket_id in active_ids:
                current = scheduler.get_iteration(ticket_id)
                if current is not None:
                    iterations[ticket_id] = f"Iter {current}/{max_iter}"

        for column in self._get_columns():
            column.update_active_states(active_ids)
            column.update_iterations(iterations)

    async def _refresh_board(self) -> None:
        self._tickets = await self.kagan_app.state_manager.get_all_tickets()
        for status in COLUMN_ORDER:
            column = self.query_one(f"#column-{status.value.lower()}", KanbanColumn)
            column.update_tickets([t for t in self._tickets if t.status == status])
        self.query_one(KaganHeader).update_count(len(self._tickets))

    def _get_columns(self) -> list[KanbanColumn]:
        return [self.query_one(f"#column-{s.value.lower()}", KanbanColumn) for s in COLUMN_ORDER]

    def _get_focused_card(self) -> TicketCard | None:
        focused = self.app.focused
        return focused if isinstance(focused, TicketCard) else None

    def _focus_first_card(self) -> None:
        for col in self._get_columns():
            if col.focus_first_card():
                return

    def _focus_column(self, status: TicketStatus) -> None:
        col = self.query_one(f"#column-{status.value.lower()}", KanbanColumn)
        col.focus_first_card()

    # Navigation actions

    def _focus_horizontal(self, direction: int) -> None:
        card = self._get_focused_card()
        if not card or not card.ticket:
            return
        columns = self._get_columns()
        col_idx = next((i for i, s in enumerate(COLUMN_ORDER) if s == card.ticket.status), -1)
        target_idx = col_idx + direction
        if target_idx < 0 or target_idx >= len(COLUMN_ORDER):
            return
        card_idx = columns[col_idx].get_focused_card_index() or 0
        cards = columns[target_idx].get_cards()
        if cards:
            columns[target_idx].focus_card(min(card_idx, len(cards) - 1))

    def action_focus_left(self) -> None:
        self._focus_horizontal(-1)

    def action_focus_right(self) -> None:
        self._focus_horizontal(1)

    def _focus_vertical(self, direction: int) -> None:
        card = self._get_focused_card()
        if not card or not card.ticket:
            return
        status = card.ticket.status
        status_str = status.value if isinstance(status, TicketStatus) else status
        col = self.query_one(f"#column-{status_str.lower()}", KanbanColumn)
        idx = col.get_focused_card_index()
        cards = col.get_cards()
        if idx is not None:
            new_idx = idx + direction
            if 0 <= new_idx < len(cards):
                col.focus_card(new_idx)

    def action_focus_up(self) -> None:
        self._focus_vertical(-1)

    def action_focus_down(self) -> None:
        self._focus_vertical(1)

    def action_deselect(self) -> None:
        """Deselect current card."""
        self.app.set_focus(None)

    def action_quit(self) -> None:
        """Quit the application."""
        self.app.exit()

    # Ticket operations

    def action_new_ticket(self) -> None:
        """Open modal to create a new ticket."""
        self.app.push_screen(TicketFormModal(), callback=self._on_new_ticket_result)

    async def _on_new_ticket_result(
        self, result: Ticket | TicketCreate | TicketUpdate | None
    ) -> None:
        """Handle new ticket form result."""
        if isinstance(result, TicketCreate):
            await self.kagan_app.state_manager.create_ticket(result)
            await self._refresh_board()
            self.notify(f"Created ticket: {result.title}")

    def action_edit_ticket(self) -> None:
        """Open modal to edit the selected ticket."""
        card = self._get_focused_card()
        if card and card.ticket:
            self._editing_ticket_id = card.ticket.id
            self.app.push_screen(
                TicketFormModal(ticket=card.ticket),
                callback=self._on_edit_ticket_result,
            )

    async def _on_edit_ticket_result(
        self, result: Ticket | TicketCreate | TicketUpdate | None
    ) -> None:
        """Handle edit ticket form result."""
        if isinstance(result, TicketUpdate) and self._editing_ticket_id is not None:
            await self.kagan_app.state_manager.update_ticket(self._editing_ticket_id, result)
            await self._refresh_board()
            self.notify("Ticket updated")
            self._editing_ticket_id = None

    def action_delete_ticket(self) -> None:
        """Delete the selected ticket with confirmation."""
        card = self._get_focused_card()
        if card and card.ticket:
            self._pending_delete_ticket = card.ticket
            self.app.push_screen(
                ConfirmModal(title="Delete Ticket?", message=f'"{card.ticket.title}"'),
                callback=self._on_delete_confirmed,
            )

    async def _on_delete_confirmed(self, confirmed: bool | None) -> None:
        if confirmed and self._pending_delete_ticket:
            ticket = self._pending_delete_ticket
            await self.kagan_app.state_manager.delete_ticket(ticket.id)
            await self._refresh_board()
            self.notify(f"Deleted ticket: {ticket.title}")
            self._focus_first_card()
        self._pending_delete_ticket = None

    async def _move_ticket(self, forward: bool) -> None:
        card = self._get_focused_card()
        if not card or not card.ticket:
            return
        status = TicketStatus(card.ticket.status)
        new_status = (
            TicketStatus.next_status(status) if forward else TicketStatus.prev_status(status)
        )
        if new_status:
            await self.kagan_app.state_manager.move_ticket(card.ticket.id, new_status)
            await self._refresh_board()
            self.notify(f"Moved #{card.ticket.id} to {new_status.value}")
            self._focus_column(new_status)
        else:
            self.notify(f"Already in {'final' if forward else 'first'} status", severity="warning")

    async def action_move_forward(self) -> None:
        await self._move_ticket(forward=True)

    async def action_move_backward(self) -> None:
        await self._move_ticket(forward=False)

    def action_view_details(self) -> None:
        """View details of selected ticket."""
        card = self._get_focused_card()
        if card and card.ticket:
            self.app.push_screen(
                TicketDetailsModal(ticket=card.ticket),
                callback=self._on_details_action,
            )

    def _on_details_action(self, action: ModalAction | None) -> None:
        """Handle action from ticket details modal."""
        if action == ModalAction.EDIT:
            self.action_edit_ticket()
        elif action == ModalAction.DELETE:
            self.action_delete_ticket()

    def action_view_agent_output(self) -> None:
        """View agent output for the selected ticket."""
        card = self._get_focused_card()
        if not card or not card.ticket:
            self.notify("No ticket selected", severity="warning")
            return

        agent = self.kagan_app.agent_manager.get(card.ticket.id)
        if not agent:
            self.notify("No agent running for this ticket", severity="warning")
            return

        self.app.push_screen(
            AgentOutputModal(agent, card.ticket.id, project_root=agent.project_root)
        )

    def action_open_chat(self) -> None:
        """Open the planner chat screen."""
        self.app.push_screen(ChatScreen())

    async def action_start_agent(self) -> None:
        """Start an agent on the focused ticket."""
        card = self._get_focused_card()
        if not card or not card.ticket:
            return
        ticket = card.ticket

        if ticket.status != TicketStatus.IN_PROGRESS:
            self.notify("Move ticket to IN_PROGRESS first", severity="warning")
            return

        manager = self.kagan_app.agent_manager
        if manager.get(ticket.id):
            self.notify("Agent already running for this ticket", severity="warning")
            return

        worktree = self.kagan_app.worktree_manager
        wt_path = await worktree.get_path(ticket.id)
        if wt_path is None:
            base_branch = self.kagan_app.config.general.default_base_branch
            wt_path = await worktree.create(ticket.id, ticket.title, base_branch)

        config = self.kagan_app.config
        agent_result = config.get_default_agent()
        if agent_result is None:
            # Create a default AgentConfig for claude
            from kagan.config import AgentConfig

            agent_config = AgentConfig(
                identity="anthropic.claude",
                name="Claude",
                short_name="claude",
                run_command={"*": "claude"},
            )
        else:
            _, agent_config = agent_result

        agent = await manager.spawn(ticket.id, agent_config, wt_path)
        self.notify(f"Started agent for {ticket.id}")

        # Run the agent loop in a background worker
        self.run_worker(self._run_agent_task(ticket, agent), exclusive=False)

    async def action_stop_agent(self) -> None:
        """Stop the agent on the focused ticket."""
        card = self._get_focused_card()
        if not card or not card.ticket:
            return
        ticket = card.ticket

        manager = self.kagan_app.agent_manager
        agent = manager.get(ticket.id)
        if not agent:
            self.notify("No agent running for this ticket", severity="warning")
            return

        await manager.terminate(ticket.id)
        self.notify(f"Stopped agent for {ticket.id}")

    async def _run_agent_task(self, ticket: Ticket, agent) -> None:
        """Background task to run the agent and send prompt."""
        from kagan.agents.prompt import build_prompt
        from kagan.agents.signals import Signal, parse_signal

        try:
            # Wait for agent to be ready
            await agent.wait_ready(timeout=60.0)
            self.notify("Agent ready, sending prompt...")

            # Build and send prompt
            config = self.kagan_app.config
            hat = config.get_hat(ticket.assigned_hat) if ticket.assigned_hat else None
            prompt = build_prompt(ticket, 1, 1, "", hat)
            stop_reason = await agent.send_prompt(prompt)

            # Parse the response
            output = agent.get_response_text()
            result = parse_signal(output)

            if result.signal == Signal.COMPLETE:
                await self.kagan_app.state_manager.update_ticket(
                    ticket.id, TicketUpdate(status=TicketStatus.REVIEW)
                )
                self.notify(f"Agent completed ticket {ticket.id}")
            elif result.signal == Signal.BLOCKED:
                self.notify(f"Agent blocked on ticket {ticket.id}", severity="warning")
            else:
                self.notify(f"Agent finished: {stop_reason}")

            await self._refresh_board()

        except TimeoutError:
            self.notify("Agent failed to start (timeout)", severity="error")
        except Exception as e:
            self.notify(f"Agent error: {e}", severity="error")
        finally:
            # Clean up
            await self.kagan_app.agent_manager.terminate(ticket.id)

    # Message handlers

    def on_ticket_card_selected(self, message: TicketCard.Selected) -> None:
        self.action_view_details()

    async def on_ticket_card_move_requested(self, message: TicketCard.MoveRequested) -> None:
        """Handle ticket move request from card."""
        if message.forward:
            await self.action_move_forward()
        else:
            await self.action_move_backward()

    def on_ticket_card_edit_requested(self, message: TicketCard.EditRequested) -> None:
        """Handle ticket edit request from card."""
        self.action_edit_ticket()

    def on_ticket_card_delete_requested(self, message: TicketCard.DeleteRequested) -> None:
        """Handle ticket delete request from card."""
        self.action_delete_ticket()

    async def on_ticket_card_drag_move(self, message: TicketCard.DragMove) -> None:
        if message.target_status and message.target_status != message.ticket.status:
            await self.kagan_app.state_manager.move_ticket(message.ticket.id, message.target_status)
            await self._refresh_board()
            self.notify(f"Moved #{message.ticket.id} to {message.target_status.value}")
            self._focus_column(message.target_status)

    async def on_ticket_changed(self, message: TicketChanged) -> None:
        """Handle ticket status change from scheduler - refresh the board."""
        await self._refresh_board()
