"""Main Kanban board screen."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from textual import getters, on
from textual.containers import Container, Horizontal
from textual.css.query import NoMatches
from textual.widgets import Footer, Static

from kagan.agents.worktree import WorktreeError
from kagan.constants import (
    COLUMN_ORDER,
    MIN_SCREEN_HEIGHT,
    MIN_SCREEN_WIDTH,
    NOTIFICATION_TITLE_MAX_LENGTH,
)
from kagan.database.models import Ticket, TicketCreate, TicketStatus, TicketType, TicketUpdate
from kagan.keybindings import (
    KANBAN_BINDINGS,
    KANBAN_LEADER_BINDINGS,
    generate_leader_hint,
    to_textual_bindings,
)
from kagan.sessions.tmux import TmuxError
from kagan.ui.modals import (
    ConfirmModal,
    DiffModal,
    ModalAction,
    RejectionInputModal,
    ReviewModal,
    TicketDetailsModal,
)
from kagan.ui.screens.base import KaganScreen
from kagan.ui.screens.kanban import actions, focus
from kagan.ui.screens.planner import PlannerScreen
from kagan.ui.utils.clipboard import copy_with_notification
from kagan.ui.widgets.card import TicketCard  # noqa: TC001 - used at runtime for messages
from kagan.ui.widgets.column import KanbanColumn
from kagan.ui.widgets.header import KaganHeader
from kagan.ui.widgets.search_bar import SearchBar

if TYPE_CHECKING:
    from textual import events
    from textual.app import ComposeResult
    from textual.timer import Timer

# Leader key timeout in seconds
LEADER_TIMEOUT = 2.0

SIZE_WARNING_MESSAGE = (
    f"Terminal too small\n\n"
    f"Minimum size: {MIN_SCREEN_WIDTH}x{MIN_SCREEN_HEIGHT}\n"
    f"Please resize your terminal"
)


class KanbanScreen(KaganScreen):
    """Main Kanban board screen with 4 columns."""

    BINDINGS = to_textual_bindings(KANBAN_BINDINGS)

    header = getters.query_one(KaganHeader)

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self._tickets: list[Ticket] = []
        self._filtered_tickets: list[Ticket] | None = None  # None = no filter active
        self._pending_delete_ticket: Ticket | None = None
        self._pending_merge_ticket: Ticket | None = None
        self._pending_advance_ticket: Ticket | None = None
        self._editing_ticket_id: str | None = None
        # Leader key state
        self._leader_active: bool = False
        self._leader_timer: Timer | None = None

    def check_action(self, action: str, parameters: tuple[object, ...]) -> bool | None:
        card = focus.get_focused_card(self)
        ticket = card.ticket if card else None

        # Normalize status for Done ticket checks
        status = None
        if ticket:
            status = ticket.status
            if isinstance(status, str):
                status = TicketStatus(status)

        # Done tickets: block edit and move actions
        if action == "edit_ticket":
            if not ticket:
                return None
            if status == TicketStatus.DONE:
                return None
            return True

        if action in ("move_forward", "move_backward"):
            if not ticket:
                return None
            # Block all movement for Done tickets
            if status == TicketStatus.DONE:
                return None
            if ticket.status == TicketStatus.IN_PROGRESS:
                ticket_type = ticket.ticket_type
                if isinstance(ticket_type, str):
                    ticket_type = TicketType(ticket_type)
                if ticket_type == TicketType.AUTO:
                    return None
            return True

        # Actions allowed for Done tickets
        if action in (
            "delete_ticket",
            "delete_ticket_direct",
            "view_details",
        ):
            return True if ticket else None

        if action == "duplicate_ticket":
            return True if ticket else None

        if action in ("merge", "merge_direct", "view_diff", "open_review"):
            if not ticket:
                return None
            return True if ticket.status == TicketStatus.REVIEW else None

        if action == "watch_agent":
            if not ticket:
                return None
            ticket_type = ticket.ticket_type
            if isinstance(ticket_type, str):
                ticket_type = TicketType(ticket_type)
            return True if ticket_type == TicketType.AUTO else None

        if action == "start_agent":
            if not ticket:
                return None
            ticket_type = ticket.ticket_type
            if isinstance(ticket_type, str):
                ticket_type = TicketType(ticket_type)
            return True if ticket_type == TicketType.AUTO else None

        if action == "stop_agent":
            if not ticket:
                return None
            ticket_type = ticket.ticket_type
            if isinstance(ticket_type, str):
                ticket_type = TicketType(ticket_type)
            if ticket_type != TicketType.AUTO:
                return None
            # Only available if agent is running
            if not self.kagan_app.scheduler.is_running(ticket.id):
                return None
            return True

        if action == "open_session":
            return True if ticket else None

        return True

    def _get_disabled_action_reason(self, action: str) -> str | None:
        """Return a user-friendly message explaining why an action is disabled."""
        card = focus.get_focused_card(self)
        ticket = card.ticket if card else None

        # No ticket selected
        if not ticket:
            if action in (
                "edit_ticket",
                "delete_ticket",
                "delete_ticket_direct",
                "view_details",
                "open_session",
                "move_forward",
                "move_backward",
                "duplicate_ticket",
            ):
                return "No ticket selected"
            if action in ("merge", "merge_direct", "view_diff", "open_review"):
                return "No ticket selected"
            if action in ("watch_agent", "start_agent"):
                return "No ticket selected"
            return None

        ticket_type = ticket.ticket_type
        if isinstance(ticket_type, str):
            ticket_type = TicketType(ticket_type)

        # Normalize status to enum for consistent comparison
        status = ticket.status
        if isinstance(status, str):
            status = TicketStatus(status)

        # Done ticket restrictions
        if status == TicketStatus.DONE:
            if action == "edit_ticket":
                return "Done tickets cannot be edited. Use [y] to duplicate."
            if action in ("move_forward", "move_backward"):
                return "Done tickets cannot be moved. Use [y] to duplicate."

        # Status-based restrictions
        if action in ("merge", "merge_direct", "view_diff", "open_review"):
            if status != TicketStatus.REVIEW:
                return f"Only available for REVIEW tickets (current: {status.value})"

        # Type-based restrictions
        if action in ("watch_agent", "start_agent", "stop_agent"):
            if ticket_type != TicketType.AUTO:
                return "Only available for AUTO tickets"

        if action == "stop_agent":
            if not self.kagan_app.scheduler.is_running(ticket.id):
                return "No agent running for this ticket"

        # Movement restrictions for AUTO in progress
        if action in ("move_forward", "move_backward"):
            if status == TicketStatus.IN_PROGRESS and ticket_type == TicketType.AUTO:
                return "Agent controls AUTO ticket movement"

        return None

    def _notify_disabled_action(self, action: str) -> None:
        """Show notification for why an action is disabled."""
        reason = self._get_disabled_action_reason(action)
        if reason:
            self.notify(reason, severity="warning")

    def compose(self) -> ComposeResult:
        yield KaganHeader(ticket_count=0)
        yield SearchBar(id="search-bar")
        with Container(classes="board-container"):
            with Horizontal(classes="board"):
                for status in COLUMN_ORDER:
                    yield KanbanColumn(status=status, tickets=[])
        with Container(classes="size-warning"):
            yield Static(SIZE_WARNING_MESSAGE, classes="size-warning-text")
        yield Static(
            generate_leader_hint(KANBAN_LEADER_BINDINGS),
            classes="leader-hint",
        )
        yield Footer()

    async def on_mount(self) -> None:
        self._check_screen_size()
        await self._refresh_board()
        focus.focus_first_card(self)
        self.kagan_app.ticket_changed_signal.subscribe(self, self._on_ticket_changed)
        self.kagan_app.iteration_changed_signal.subscribe(self, self._on_iteration_changed)
        self._sync_iterations()
        self._sync_agent_states()
        from kagan.ui.widgets.header import _get_git_branch

        config_path = self.kagan_app.config_path
        repo_root = config_path.parent.parent
        branch = await _get_git_branch(repo_root)
        self.header.update_branch(branch)

    async def _on_ticket_changed(self, _ticket_id: str) -> None:
        await self._refresh_board()

    def _on_iteration_changed(self, data: tuple[str, int]) -> None:
        """Handle iteration count updates from scheduler."""
        ticket_id, iteration = data
        try:
            column = self.query_one("#column-in_progress", KanbanColumn)
        except NoMatches:
            return
        max_iter = self.kagan_app.config.general.max_iterations
        if iteration > 0:
            column.update_iterations({ticket_id: f"Iter {iteration}/{max_iter}"})
            # Also update agent running state
            for card in column.get_cards():
                if card.ticket and card.ticket.id == ticket_id:
                    card.is_agent_active = True
        else:
            column.update_iterations({ticket_id: ""})
            # Agent stopped
            for card in column.get_cards():
                if card.ticket and card.ticket.id == ticket_id:
                    card.is_agent_active = False

    def _sync_iterations(self) -> None:
        """Sync iteration display for any already-running tickets."""
        scheduler = self.kagan_app.scheduler
        try:
            column = self.query_one("#column-in_progress", KanbanColumn)
        except NoMatches:
            return
        max_iter = self.kagan_app.config.general.max_iterations
        iterations = {}
        for card in column.get_cards():
            if card.ticket:
                count = scheduler.get_iteration_count(card.ticket.id)
                if count > 0:
                    iterations[card.ticket.id] = f"Iter {count}/{max_iter}"
        if iterations:
            column.update_iterations(iterations)

    def _sync_agent_states(self) -> None:
        """Sync agent running state display for all tickets."""
        scheduler = self.kagan_app.scheduler
        running_tickets = scheduler._running_tickets
        try:
            column = self.query_one("#column-in_progress", KanbanColumn)
            column.update_active_states(running_tickets)
        except NoMatches:
            pass

    def on_descendant_focus(self, event: events.DescendantFocus) -> None:
        self.refresh_bindings()

    def on_resize(self, event: events.Resize) -> None:
        self._check_screen_size()

    async def on_screen_resume(self) -> None:
        await self._refresh_board()
        self._sync_iterations()
        self._sync_agent_states()

    def _check_screen_size(self) -> None:
        size = self.app.size
        if size.width < MIN_SCREEN_WIDTH or size.height < MIN_SCREEN_HEIGHT:
            self.add_class("too-small")
        else:
            self.remove_class("too-small")

    async def _refresh_board(self) -> None:
        self._tickets = await self.kagan_app.state_manager.get_all_tickets()
        # Use filtered tickets if search is active, otherwise all tickets
        display_tickets = (
            self._filtered_tickets if self._filtered_tickets is not None else self._tickets
        )
        for status in COLUMN_ORDER:
            column = self.query_one(f"#column-{status.value.lower()}", KanbanColumn)
            column.update_tickets([t for t in display_tickets if t.status == status])
        self.header.update_count(len(self._tickets))
        active_sessions = sum(1 for ticket in self._tickets if ticket.session_active)
        self.header.update_sessions(active_sessions)
        self.refresh_bindings()

    # Navigation actions
    def action_focus_left(self) -> None:
        focus.focus_horizontal(self, -1)

    def action_focus_right(self) -> None:
        focus.focus_horizontal(self, 1)

    def action_focus_up(self) -> None:
        focus.focus_vertical(self, -1)

    def action_focus_down(self) -> None:
        focus.focus_vertical(self, 1)

    def action_focus_next_column(self) -> None:
        """Tab navigation: move to next column."""
        focus.focus_horizontal(self, 1)

    def action_focus_prev_column(self) -> None:
        """Shift+Tab navigation: move to previous column."""
        focus.focus_horizontal(self, -1)

    def action_deselect(self) -> None:
        """Handle Escape - deselect or cancel leader mode."""
        if self._leader_active:
            self._deactivate_leader()
            return
        # Check if search is active and hide it
        try:
            search_bar = self.query_one("#search-bar", SearchBar)
            if search_bar.is_visible:
                search_bar.hide()
                self._filtered_tickets = None
                self.run_worker(self._refresh_board())
                return
        except NoMatches:
            pass
        self.app.set_focus(None)

    def action_quit(self) -> None:
        self.app.exit()

    def action_interrupt(self) -> None:
        """Handle Ctrl+C - exit app."""
        self.app.exit()

    # =========================================================================
    # Leader Key Infrastructure
    # =========================================================================

    def action_activate_leader(self) -> None:
        """Activate leader key mode with timeout."""
        if self._leader_active:
            return
        self._leader_active = True
        # Show leader hint
        try:
            hint = self.query_one(".leader-hint", Static)
            hint.add_class("visible")
        except NoMatches:
            pass
        # Start timeout timer
        self._leader_timer = self.set_timer(LEADER_TIMEOUT, self._leader_timeout)

    def _leader_timeout(self) -> None:
        """Called when leader key times out."""
        self._deactivate_leader()

    def _deactivate_leader(self) -> None:
        """Deactivate leader key mode."""
        self._leader_active = False
        if self._leader_timer:
            self._leader_timer.stop()
            self._leader_timer = None
        # Hide leader hint
        try:
            hint = self.query_one(".leader-hint", Static)
            hint.remove_class("visible")
        except NoMatches:
            pass

    def _execute_leader_action(self, action_name: str) -> None:
        """Execute a leader action and deactivate leader mode."""
        self._deactivate_leader()
        # Check if action is disabled and notify user
        if self.check_action(action_name, ()) is None:
            self._notify_disabled_action(action_name)
            return
        # Call the appropriate action
        action_method = getattr(self, f"action_{action_name}", None)
        if action_method:
            result = action_method()
            # Handle async actions
            if asyncio.iscoroutine(result):
                self.run_worker(result)

    def on_key(self, event: events.Key) -> None:
        """Handle key events for leader key sequences and disabled action feedback."""
        # Handle leader key mode
        if self._leader_active:
            # Map leader key sequences
            leader_actions = {
                "d": "view_diff",
                "h": "move_backward",
                "l": "move_forward",
                "r": "open_review",
                "w": "watch_agent",
            }

            if event.key in leader_actions:
                event.prevent_default()
                event.stop()
                self._execute_leader_action(leader_actions[event.key])
            elif event.key == "escape":
                event.prevent_default()
                event.stop()
                self._deactivate_leader()
            else:
                # Invalid key - cancel leader mode
                self._deactivate_leader()
            return

        # Check for disabled actions on direct key presses and show feedback
        # Map keys to their action names for feedback
        key_action_map = {
            "x": "delete_ticket_direct",
            "m": "merge_direct",
            "e": "edit_ticket",
            "v": "view_details",
            "enter": "open_session",
            "a": "start_agent",
            "w": "watch_agent",
            "s": "stop_agent",
            "D": "view_diff",
            "r": "open_review",
        }

        if event.key in key_action_map:
            action = key_action_map[event.key]
            if self.check_action(action, ()) is None:
                self._notify_disabled_action(action)

    # =========================================================================
    # Search Infrastructure
    # =========================================================================

    def action_toggle_search(self) -> None:
        """Toggle search bar visibility."""
        try:
            search_bar = self.query_one("#search-bar", SearchBar)
            if search_bar.is_visible:
                search_bar.hide()
                self._filtered_tickets = None
                self.run_worker(self._refresh_board())
            else:
                search_bar.show()
        except NoMatches:
            pass

    @on(SearchBar.QueryChanged)
    async def on_search_query_changed(self, event: SearchBar.QueryChanged) -> None:
        """Handle search query changes."""
        query = event.query.strip()
        if not query:
            self._filtered_tickets = None
        else:
            self._filtered_tickets = await self.kagan_app.state_manager.search_tickets(query)
        await self._refresh_board()

    # Ticket operations
    def action_new_ticket(self) -> None:
        self.app.push_screen(TicketDetailsModal(), callback=self._on_ticket_modal_result)

    async def _on_ticket_modal_result(
        self, result: ModalAction | TicketCreate | TicketUpdate | None
    ) -> None:
        if isinstance(result, TicketCreate):
            await self.kagan_app.state_manager.create_ticket(result)
            await self._refresh_board()
            self.notify(f"Created ticket: {result.title}")
        elif isinstance(result, TicketUpdate) and self._editing_ticket_id is not None:
            await self.kagan_app.state_manager.update_ticket(self._editing_ticket_id, result)
            await self._refresh_board()
            self.notify("Ticket updated")
            self._editing_ticket_id = None
        elif result == ModalAction.DELETE:
            self.action_delete_ticket()

    def action_edit_ticket(self) -> None:
        card = focus.get_focused_card(self)
        if card and card.ticket:
            self._editing_ticket_id = card.ticket.id
            self.app.push_screen(
                TicketDetailsModal(ticket=card.ticket, start_editing=True),
                callback=self._on_ticket_modal_result,
            )

    def action_delete_ticket(self) -> None:
        card = focus.get_focused_card(self)
        if card and card.ticket:
            self._pending_delete_ticket = card.ticket
            self.app.push_screen(
                ConfirmModal(title="Delete Ticket?", message=f'"{card.ticket.title}"'),
                callback=self._on_delete_confirmed,
            )

    async def _on_delete_confirmed(self, confirmed: bool | None) -> None:
        if confirmed and self._pending_delete_ticket:
            ticket = self._pending_delete_ticket
            await actions.delete_ticket(self.kagan_app, ticket)
            await self._refresh_board()
            self.notify(f"Deleted ticket: {ticket.title}")
            focus.focus_first_card(self)
        self._pending_delete_ticket = None

    async def action_delete_ticket_direct(self) -> None:
        """Delete ticket directly without confirm modal."""
        card = focus.get_focused_card(self)
        if card and card.ticket:
            ticket = card.ticket
            await actions.delete_ticket(self.kagan_app, ticket)
            await self._refresh_board()
            self.notify(f"Deleted: {ticket.title}")
            focus.focus_first_card(self)

    async def action_merge_direct(self) -> None:
        """Merge ticket directly without confirm modal."""
        ticket = actions.get_review_ticket(self, focus.get_focused_card(self))
        if not ticket:
            return
        success, message = await actions.merge_ticket(self.kagan_app, ticket)
        if success:
            await self._refresh_board()
            self.notify(f"Merged: {ticket.title}")
        else:
            self.notify(message, severity="error")

    async def _move_ticket(self, forward: bool) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return

        ticket = card.ticket
        status = TicketStatus(ticket.status)
        ticket_type = ticket.ticket_type
        if isinstance(ticket_type, str):
            ticket_type = TicketType(ticket_type)

        if status == TicketStatus.IN_PROGRESS and ticket_type == TicketType.AUTO:
            self.notify("Agent controls this ticket's movement", severity="warning")
            return

        new_status = (
            TicketStatus.next_status(status) if forward else TicketStatus.prev_status(status)
        )
        if new_status:
            if status == TicketStatus.REVIEW and new_status == TicketStatus.DONE:
                self._pending_merge_ticket = ticket
                title = ticket.title[:NOTIFICATION_TITLE_MAX_LENGTH]
                msg = f"Merge '{title}' and move to DONE?\n\nCleanup worktree and session."
                self.app.push_screen(
                    ConfirmModal(title="Complete Ticket?", message=msg),
                    callback=self._on_merge_confirmed,
                )
                return

            is_pair = ticket_type == TicketType.PAIR
            is_pair_in_progress = status == TicketStatus.IN_PROGRESS and is_pair
            if is_pair_in_progress and new_status == TicketStatus.REVIEW:
                self._pending_advance_ticket = ticket
                title = ticket.title[:NOTIFICATION_TITLE_MAX_LENGTH]
                msg = f"Move '{title}' to REVIEW?\n\nMake sure your changes are ready."
                self.app.push_screen(
                    ConfirmModal(title="Advance to Review?", message=msg),
                    callback=self._on_advance_confirmed,
                )
                return

            await self.kagan_app.state_manager.move_ticket(ticket.id, new_status)
            await self._refresh_board()
            self.notify(f"Moved #{ticket.id} to {new_status.value}")
            focus.focus_column(self, new_status)
        else:
            self.notify(f"Already in {'final' if forward else 'first'} status", severity="warning")

    async def _on_merge_confirmed(self, confirmed: bool | None) -> None:
        if confirmed and self._pending_merge_ticket:
            ticket = self._pending_merge_ticket
            success, message = await actions.merge_ticket(self.kagan_app, ticket)
            if success:
                await self._refresh_board()
                self.notify(f"Merged and completed: {ticket.title}")
            else:
                self.notify(message, severity="error")
        self._pending_merge_ticket = None

    async def _on_advance_confirmed(self, confirmed: bool | None) -> None:
        if confirmed and self._pending_advance_ticket:
            ticket = self._pending_advance_ticket
            await self.kagan_app.state_manager.move_ticket(ticket.id, TicketStatus.REVIEW)
            await self._refresh_board()
            self.notify(f"Moved #{ticket.id} to REVIEW")
            focus.focus_column(self, TicketStatus.REVIEW)
        self._pending_advance_ticket = None

    async def action_move_forward(self) -> None:
        await self._move_ticket(forward=True)

    async def action_move_backward(self) -> None:
        await self._move_ticket(forward=False)

    async def action_duplicate_ticket(self) -> None:
        """Open duplicate modal for the focused ticket."""
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            self.notify("No ticket selected", severity="warning")
            return

        from kagan.ui.modals.duplicate_ticket import DuplicateTicketModal

        self.app.push_screen(
            DuplicateTicketModal(source_ticket=card.ticket),
            callback=self._on_duplicate_result,
        )

    async def _on_duplicate_result(self, result: TicketCreate | None) -> None:
        """Handle the result from the duplicate ticket modal."""
        if result:
            ticket = await self.kagan_app.state_manager.create_ticket(result)
            await self._refresh_board()
            self.notify(f"Created duplicate: #{ticket.short_id}")
            focus.focus_column(self, TicketStatus.BACKLOG)

    def action_copy_ticket_id(self) -> None:
        """Copy the focused ticket's ID to clipboard."""
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            self.notify("No ticket selected", severity="warning")
            return
        copy_with_notification(self.app, f"#{card.ticket.short_id}", "Ticket ID")

    def action_view_details(self) -> None:
        card = focus.get_focused_card(self)
        if card and card.ticket:
            self._editing_ticket_id = card.ticket.id
            self.app.push_screen(
                TicketDetailsModal(ticket=card.ticket),
                callback=self._on_ticket_modal_result,
            )

    async def action_open_session(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return

        ticket = card.ticket
        if ticket.status == TicketStatus.REVIEW:
            await self.action_open_review()
            return

        raw_type = ticket.ticket_type
        ticket_type = TicketType(raw_type) if isinstance(raw_type, str) else raw_type

        if ticket_type == TicketType.AUTO:
            await self._open_auto_session(ticket)
        else:
            await self._open_pair_session(ticket)

    async def _open_auto_session(self, ticket: Ticket, manual: bool = False) -> None:
        scheduler = self.kagan_app.scheduler
        config = self.kagan_app.config

        # Handle BACKLOG tickets - move to IN_PROGRESS first
        if ticket.status == TicketStatus.BACKLOG:
            await self.kagan_app.state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)
            # Refresh ticket to get updated status
            refreshed = await self.kagan_app.state_manager.get_ticket(ticket.id)
            if refreshed:
                ticket = refreshed
            await self._refresh_board()

            # For manual start, spawn agent directly
            if manual:
                spawned = await scheduler.spawn_for_ticket(ticket)
                if spawned:
                    self.notify(f"Started agent for: {ticket.short_id}")
                else:
                    self.notify("Failed to start agent (at capacity?)", severity="warning")
                return

            # For auto mode, notify and let scheduler pick it up
            if config.general.auto_start:
                self.notify(f"Started AUTO ticket: {ticket.short_id}")
            else:
                self.notify(
                    "Ticket moved to IN_PROGRESS. Press [a] to start agent manually.",
                    severity="warning",
                )
            return

        # Ticket already in IN_PROGRESS - check if agent is running
        if scheduler.is_running(ticket.id):
            from kagan.ui.modals.agent_output import AgentOutputModal

            agent = scheduler.get_running_agent(ticket.id)
            iteration = scheduler.get_iteration_count(ticket.id)
            modal = AgentOutputModal(ticket=ticket, agent=agent, iteration=iteration)
            await self.app.push_screen(modal)
        elif manual:
            # Manual start for IN_PROGRESS ticket that's not running
            spawned = await scheduler.spawn_for_ticket(ticket)
            if spawned:
                self.notify(f"Started agent for: {ticket.short_id}")
                # Optionally open watch modal immediately
                from kagan.ui.modals.agent_output import AgentOutputModal

                agent = scheduler.get_running_agent(ticket.id)
                iteration = scheduler.get_iteration_count(ticket.id)
                modal = AgentOutputModal(ticket=ticket, agent=agent, iteration=iteration)
                await self.app.push_screen(modal)
            else:
                self.notify("Failed to start agent (at capacity?)", severity="warning")
        else:
            # Auto mode not enabled and not manually triggered
            if not config.general.auto_start:
                self.notify(
                    "Auto-start disabled. Press a to start manually.",
                    severity="warning",
                )
            else:
                self.notify("Agent starting on next tick (~5 seconds)")

    async def _open_pair_session(self, ticket: Ticket) -> None:
        # Check if user wants to skip the gateway modal
        if not self.kagan_app.config.ui.skip_tmux_gateway:
            from kagan.ui.modals.tmux_gateway import TmuxGatewayModal

            def on_gateway_result(result: str | None) -> None:
                if result is None:
                    # User cancelled
                    return
                if result == "skip_future":
                    # Update config to skip in future
                    self.kagan_app.config.ui.skip_tmux_gateway = True
                    self._save_tmux_gateway_preference()
                # Proceed to open tmux session
                self.app.call_later(self._do_open_pair_session, ticket)

            self.app.push_screen(TmuxGatewayModal(ticket.id, ticket.title), on_gateway_result)
            return

        # Skip modal - open directly
        await self._do_open_pair_session(ticket)

    async def _do_open_pair_session(self, ticket: Ticket) -> None:
        """Actually open the tmux session after modal confirmation."""
        worktree = self.kagan_app.worktree_manager

        try:
            wt_path = await worktree.get_path(ticket.id)
            if wt_path is None:
                base = self.kagan_app.config.general.default_base_branch
                wt_path = await worktree.create(ticket.id, ticket.title, base)

            session_manager = self.kagan_app.session_manager
            if not await session_manager.session_exists(ticket.id):
                await session_manager.create_session(ticket, wt_path)

            if ticket.status == TicketStatus.BACKLOG:
                await self.kagan_app.state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)

            with self.app.suspend():
                attach_success = session_manager.attach_session(ticket.id)

            if not attach_success:
                # Session died unexpectedly (e.g., agent exited, tmux killed)
                # Clean up stale state and try to recreate
                await session_manager.kill_session(ticket.id)
                await session_manager.create_session(ticket, wt_path)

                with self.app.suspend():
                    retry_success = session_manager.attach_session(ticket.id)

                if not retry_success:
                    self.notify("Session failed to start. Try again.", severity="error")

            await self._refresh_board()
        except (TmuxError, WorktreeError) as exc:
            self.notify(f"Failed to open session: {exc}", severity="error")

    def action_open_planner(self) -> None:
        self.app.push_screen(PlannerScreen())

    async def action_open_settings(self) -> None:
        """Open settings modal."""
        from kagan.ui.modals import SettingsModal

        config = self.kagan_app.config
        config_path = self.kagan_app.config_path
        result = await self.app.push_screen(SettingsModal(config, config_path))
        if result:
            # Reload config after save
            self.kagan_app.config = self.kagan_app.config.load(config_path)
            self.notify("Settings saved")

    async def action_watch_agent(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return

        ticket = card.ticket
        ticket_type = ticket.ticket_type
        if isinstance(ticket_type, str):
            ticket_type = TicketType(ticket_type)
        if ticket_type != TicketType.AUTO:
            self.notify("Watch is only for AUTO tickets", severity="warning")
            return

        scheduler = self.kagan_app.scheduler
        if not scheduler.is_running(ticket.id):
            # Provide more helpful message based on ticket status
            status = ticket.status
            if isinstance(status, str):
                status = TicketStatus(status)

            if status == TicketStatus.BACKLOG:
                self.notify(
                    "Agent not started. Press [a] to start or move to IN_PROGRESS.",
                    severity="warning",
                )
            elif status == TicketStatus.IN_PROGRESS:
                config = self.kagan_app.config
                if config.general.auto_start:
                    self.notify(
                        "Agent starting soon... (next scheduler tick)",
                        severity="warning",
                    )
                else:
                    self.notify(
                        "Agent not running. Press a to start manually.",
                        severity="warning",
                    )
            else:
                self.notify("No agent running for this ticket", severity="warning")
            return

        from kagan.ui.modals.agent_output import AgentOutputModal

        agent = scheduler.get_running_agent(ticket.id)
        iteration = scheduler.get_iteration_count(ticket.id)
        modal = AgentOutputModal(ticket=ticket, agent=agent, iteration=iteration)
        await self.app.push_screen(modal)

    async def action_start_agent(self) -> None:
        """Manually start an AUTO agent (bypasses auto mode check)."""
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return

        ticket = card.ticket
        ticket_type = ticket.ticket_type
        if isinstance(ticket_type, str):
            ticket_type = TicketType(ticket_type)
        if ticket_type != TicketType.AUTO:
            self.notify("Start agent is only for AUTO tickets", severity="warning")
            return

        await self._open_auto_session(ticket, manual=True)

    async def action_stop_agent(self) -> None:
        """Stop running agent and move ticket to BACKLOG."""
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return

        ticket = card.ticket
        ticket_type = ticket.ticket_type
        if isinstance(ticket_type, str):
            ticket_type = TicketType(ticket_type)
        if ticket_type != TicketType.AUTO:
            self.notify("Stop agent is only for AUTO tickets", severity="warning")
            return

        scheduler = self.kagan_app.scheduler
        if not scheduler.is_running(ticket.id):
            self.notify("No agent running for this ticket", severity="warning")
            return

        # Stop the agent and its task
        await scheduler.stop_ticket(ticket.id)

        # Move ticket to BACKLOG to prevent auto-restart
        await self.kagan_app.state_manager.move_ticket(ticket.id, TicketStatus.BACKLOG)
        await self._refresh_board()
        self.notify(f"Stopped agent: {ticket.short_id}, moved to BACKLOG")

    async def action_merge(self) -> None:
        ticket = actions.get_review_ticket(self, focus.get_focused_card(self))
        if not ticket:
            return

        success, message = await actions.merge_ticket(self.kagan_app, ticket)
        if success:
            await self._refresh_board()
            self.notify(f"Merged and completed: {ticket.title}")
        else:
            self.notify(message, severity="error")

    async def action_view_diff(self) -> None:
        ticket = actions.get_review_ticket(self, focus.get_focused_card(self))
        if not ticket:
            return

        worktree = self.kagan_app.worktree_manager
        base = self.kagan_app.config.general.default_base_branch
        diff_text = await worktree.get_diff(ticket.id, base_branch=base)
        title = f"Diff: {ticket.short_id} {ticket.title[:NOTIFICATION_TITLE_MAX_LENGTH]}"
        await self.app.push_screen(DiffModal(title=title, diff_text=diff_text))

    async def action_open_review(self) -> None:
        ticket = actions.get_review_ticket(self, focus.get_focused_card(self))
        if not ticket:
            return

        from kagan.agents.config_resolver import resolve_agent_config

        agent_config = resolve_agent_config(ticket, self.kagan_app.config)

        await self.app.push_screen(
            ReviewModal(
                ticket=ticket,
                worktree_manager=self.kagan_app.worktree_manager,
                agent_config=agent_config,
                base_branch=self.kagan_app.config.general.default_base_branch,
            ),
            callback=self._on_review_result,
        )

    async def _on_review_result(self, result: str | None) -> None:
        ticket = actions.get_review_ticket(self, focus.get_focused_card(self))
        if not ticket:
            return

        if result == "approve":
            success, message = await actions.merge_ticket(self.kagan_app, ticket)
            if success:
                await self._refresh_board()
                self.notify(f"Merged and completed: {ticket.title}")
            else:
                self.notify(message, severity="error")
        elif result == "reject":
            await self._handle_reject_with_feedback(ticket)

    async def _handle_reject_with_feedback(self, ticket: Ticket) -> None:
        ticket_type = ticket.ticket_type
        if isinstance(ticket_type, str):
            ticket_type = TicketType(ticket_type)

        if ticket_type == TicketType.AUTO:
            await self.app.push_screen(
                RejectionInputModal(ticket.title),
                callback=lambda feedback: self._apply_rejection_feedback(ticket, feedback),
            )
        else:
            await self.kagan_app.state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)
            await self._refresh_board()
            self.notify(f"Moved back to IN_PROGRESS: {ticket.title}")

    async def _apply_rejection_feedback(self, ticket: Ticket, feedback: str | None) -> None:
        await actions.apply_rejection_feedback(self.kagan_app, ticket, feedback)
        await self._refresh_board()
        self.notify(f"Rejected: {ticket.title}")

    def _save_tmux_gateway_preference(self) -> None:
        """Save tmux gateway skip preference to config file."""
        import re

        config_path = self.kagan_app.config_path
        if not config_path.exists():
            # Config file doesn't exist, create with UI section
            config_path.parent.mkdir(exist_ok=True)
            config_path.write_text("[ui]\nskip_tmux_gateway = true\n")
            return

        content = config_path.read_text()
        if "[ui]" not in content:
            # Add UI section at the end
            if not content.endswith("\n"):
                content += "\n"
            content += "\n[ui]\nskip_tmux_gateway = true\n"
        else:
            # Update existing value or add to section
            if "skip_tmux_gateway" in content:
                content = re.sub(
                    r"skip_tmux_gateway\s*=\s*(true|false)",
                    "skip_tmux_gateway = true",
                    content,
                )
            else:
                # Add after [ui] section header
                content = re.sub(
                    r"(\[ui\])",
                    r"\1\nskip_tmux_gateway = true",
                    content,
                )
        config_path.write_text(content)

    # Message handlers
    def on_ticket_card_selected(self, message: TicketCard.Selected) -> None:
        self.action_view_details()
