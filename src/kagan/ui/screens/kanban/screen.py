"""Main Kanban board screen."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from textual import getters, on
from textual.binding import Binding
from textual.containers import Container, Horizontal
from textual.css.query import NoMatches
from textual.widgets import Footer, Static

from kagan.constants import (
    COLUMN_ORDER,
    MIN_SCREEN_HEIGHT,
    MIN_SCREEN_WIDTH,
    NOTIFICATION_TITLE_MAX_LENGTH,
)
from kagan.database.models import MergeReadiness, Ticket, TicketStatus, TicketType
from kagan.keybindings import (
    KANBAN_BINDINGS,
    KANBAN_LEADER_BINDINGS,
    generate_leader_hint,
)
from kagan.lifecycle.ticket_lifecycle import TicketLifecycle
from kagan.ui.modals import (
    AgentOutputModal,
    ConfirmModal,
    DiffModal,
    ModalAction,
    RejectionInputModal,
    ReviewModal,
    TicketDetailsModal,
)
from kagan.ui.modals.description_editor import DescriptionEditorModal
from kagan.ui.screens.base import KaganScreen
from kagan.ui.screens.kanban import focus
from kagan.ui.screens.kanban.hints import build_keybinding_hints
from kagan.ui.screens.planner import PlannerScreen
from kagan.ui.utils import copy_with_notification
from kagan.ui.widgets.card import TicketCard  # noqa: TC001 - needed at runtime for message handler
from kagan.ui.widgets.column import KanbanColumn
from kagan.ui.widgets.header import KaganHeader
from kagan.ui.widgets.keybinding_hint import KeybindingHint
from kagan.ui.widgets.peek_overlay import PeekOverlay
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

    BINDINGS = KANBAN_BINDINGS

    header = getters.query_one(KaganHeader)

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self._tickets: list[Ticket] = []
        self._filtered_tickets: list[Ticket] | None = None
        self._pending_delete_ticket: Ticket | None = None
        self._pending_merge_ticket: Ticket | None = None
        self._pending_close_ticket: Ticket | None = None
        self._pending_advance_ticket: Ticket | None = None
        self._pending_auto_move_ticket: Ticket | None = None
        self._pending_auto_move_status: TicketStatus | None = None
        self._editing_ticket_id: str | None = None
        self._leader_active: bool = False
        self._leader_timer: Timer | None = None
        self._merge_readiness: dict[str, str] = {}
        self._refresh_timer: Timer | None = None
        self._ticket_hashes: dict[str, int] = {}  # ticket_id -> hash for change detection
        # Lifecycle (initialized on mount)
        self._lifecycle: TicketLifecycle | None = None

    def _init_components(self) -> None:
        """Initialize lifecycle component."""
        self._lifecycle = TicketLifecycle(
            self.kagan_app.state_manager,
            self.kagan_app.worktree_manager,
            self.kagan_app.session_manager,
            self.kagan_app.scheduler,
            self.kagan_app.config,
        )

    # Actions requiring a ticket to be selected
    _TICKET_REQUIRED_ACTIONS = frozenset(
        {
            "edit_ticket",
            "delete_ticket",
            "delete_ticket_direct",
            "view_details",
            "open_session",
            "move_forward",
            "move_backward",
            "duplicate_ticket",
            "merge",
            "merge_direct",
            "view_diff",
            "open_review",
            "watch_agent",
            "start_agent",
            "stop_agent",
        }
    )

    def _validate_action(self, action: str) -> tuple[bool, str | None]:
        """Validate if an action can be performed (inlined from ActionValidator)."""
        card = focus.get_focused_card(self)
        ticket = card.ticket if card else None
        scheduler = self.kagan_app.scheduler

        # No ticket - check ticket-requiring actions
        if not ticket:
            if action in self._TICKET_REQUIRED_ACTIONS:
                return (False, "No ticket selected")
            return (True, None)

        status = ticket.status
        ticket_type = ticket.ticket_type

        # Edit validation
        if action == "edit_ticket":
            if status == TicketStatus.DONE:
                return (False, "Done tickets cannot be edited. Use [y] to duplicate.")
            return (True, None)

        # Move validation
        if action in ("move_forward", "move_backward"):
            if status == TicketStatus.DONE:
                return (False, "Done tickets cannot be moved. Use [y] to duplicate.")
            return (True, None)

        # Review validation
        if action in ("merge", "merge_direct", "view_diff", "open_review"):
            if status != TicketStatus.REVIEW:
                return (False, f"Only available for REVIEW tickets (current: {status.value})")
            return (True, None)

        # Watch agent validation
        if action == "watch_agent":
            if ticket_type != TicketType.AUTO:
                return (False, "Only available for AUTO tickets")
            is_running = scheduler.is_running(ticket.id)
            if is_running or status == TicketStatus.IN_PROGRESS:
                return (True, None)
            return (False, "No agent running for this ticket")

        # Start agent validation
        if action == "start_agent":
            if ticket_type != TicketType.AUTO:
                return (False, "Only available for AUTO tickets")
            return (True, None)

        # Stop agent validation
        if action == "stop_agent":
            if ticket_type != TicketType.AUTO:
                return (False, "Only available for AUTO tickets")
            if not scheduler.is_running(ticket.id):
                return (False, "No agent running for this ticket")
            return (True, None)

        return (True, None)

    def check_action(self, action: str, parameters: tuple[object, ...]) -> bool | None:
        is_valid, _ = self._validate_action(action)
        return True if is_valid else None

    def compose(self) -> ComposeResult:
        yield KaganHeader(ticket_count=0)
        yield SearchBar(id="search-bar")
        with Container(classes="board-container"):
            with Horizontal(classes="board"):
                for status in COLUMN_ORDER:
                    yield KanbanColumn(status=status, tickets=[])
        with Container(classes="size-warning"):
            yield Static(SIZE_WARNING_MESSAGE, classes="size-warning-text")
        yield Static("", id="review-queue-hint", classes="review-queue-hint")
        yield Static(generate_leader_hint(KANBAN_LEADER_BINDINGS), classes="leader-hint")
        yield PeekOverlay(id="peek-overlay")
        yield KeybindingHint(id="keybinding-hint", classes="keybinding-hint")
        yield Footer()

    async def on_mount(self) -> None:
        self._init_components()
        self._check_screen_size()
        await self._refresh_board()
        focus.focus_first_card(self)
        self.kagan_app.ticket_changed_signal.subscribe(self, self._on_ticket_changed)
        self.kagan_app.iteration_changed_signal.subscribe(self, self._on_iteration_changed)
        self._sync_iterations()
        self._sync_agent_states()
        from kagan.ui.widgets.header import _get_git_branch

        branch = await _get_git_branch(self.kagan_app.config_path.parent.parent)
        self.header.update_branch(branch)

    def on_unmount(self) -> None:
        """Clean up pending state on unmount."""
        self._pending_delete_ticket = None
        self._pending_merge_ticket = None
        self._pending_advance_ticket = None
        self._pending_auto_move_ticket = None
        self._pending_auto_move_status = None
        self._editing_ticket_id = None
        self._filtered_tickets = None
        if self._refresh_timer:
            self._refresh_timer.stop()
            self._refresh_timer = None

    async def _on_ticket_changed(self, _ticket_id: str) -> None:
        self._schedule_refresh()

    def _on_iteration_changed(self, data: tuple[str, int]) -> None:
        ticket_id, iteration = data
        try:
            column = self.query_one("#column-in_progress", KanbanColumn)
        except NoMatches:
            return
        max_iter = self.kagan_app.config.general.max_iterations
        if iteration > 0:
            column.update_iterations({ticket_id: f"Iter {iteration}/{max_iter}"})
            for card in column.get_cards():
                if card.ticket and card.ticket.id == ticket_id:
                    card.is_agent_active = True
        else:
            column.update_iterations({ticket_id: ""})
            for card in column.get_cards():
                if card.ticket and card.ticket.id == ticket_id:
                    card.is_agent_active = False

    def _sync_iterations(self) -> None:
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
        """Sync agent active states for all columns.

        Updates is_agent_active for all cards based on scheduler's running tickets.
        This ensures cards show correct running state even during status transitions.
        """
        scheduler = self.kagan_app.scheduler
        running_tickets = scheduler._running_tickets
        for column in self.query(KanbanColumn):
            column.update_active_states(running_tickets)

    def on_descendant_focus(self, event: events.DescendantFocus) -> None:
        """Update UI immediately on focus change (hints first for instant feedback)."""
        self._update_keybinding_hints()
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
        """Refresh board with differential updates (only changed tickets)."""
        new_tickets = await self.kagan_app.state_manager.get_all_tickets()
        display_tickets = (
            self._filtered_tickets if self._filtered_tickets is not None else new_tickets
        )

        old_status_by_id = {ticket.id: ticket.status for ticket in self._tickets}

        # Compute changed tickets
        new_hashes = {
            t.id: hash((t.status.value, t.title, t.session_active, t.total_iterations))
            for t in new_tickets
        }
        changed_ids = {tid for tid, h in new_hashes.items() if self._ticket_hashes.get(tid) != h}
        deleted_ids = set(self._ticket_hashes.keys()) - set(new_hashes.keys())

        # Only update if changes detected
        if changed_ids or deleted_ids or self._ticket_hashes == {}:
            self._tickets = new_tickets
            self._ticket_hashes = new_hashes
            self._update_merge_readiness_cache(new_tickets)

            # Determine which columns need updates
            affected_statuses = set()
            for ticket in new_tickets:
                if ticket.id in changed_ids:
                    affected_statuses.add(ticket.status)
                    old_status = old_status_by_id.get(ticket.id)
                    if old_status is not None and old_status != ticket.status:
                        affected_statuses.add(old_status)
            for _tid in deleted_ids:
                # Need full refresh on deletion to be safe
                affected_statuses = set(COLUMN_ORDER)
                break

            # Update only affected columns
            for status in affected_statuses:
                column = self.query_one(f"#column-{status.value.lower()}", KanbanColumn)
                column.update_tickets([t for t in display_tickets if t.status == status])

            self._sync_merge_readiness()
            self.header.update_count(len(self._tickets))
            active_sessions = sum(1 for ticket in self._tickets if ticket.session_active)
            self.header.update_sessions(active_sessions)
            self._update_review_queue_hint()
            self._update_keybinding_hints()
            self.refresh_bindings()

    async def _refresh_and_sync(self) -> None:
        await self._refresh_board()
        self._sync_iterations()
        self._sync_agent_states()

    def _schedule_refresh(self) -> None:
        if self._refresh_timer:
            self._refresh_timer.stop()
        self._refresh_timer = self.set_timer(0.15, self._run_refresh)

    def _run_refresh(self) -> None:
        self._refresh_timer = None
        self.run_worker(self._refresh_and_sync())

    def _update_merge_readiness_cache(self, tickets: list[Ticket]) -> None:
        for ticket in tickets:
            if ticket.status != TicketStatus.REVIEW:
                self._merge_readiness.pop(ticket.id, None)
                continue
            readiness_value = getattr(ticket, "merge_readiness", "risk")
            readiness = (
                readiness_value.value if hasattr(readiness_value, "value") else str(readiness_value)
            )
            if ticket.merge_failed:
                readiness = "blocked"
            self._merge_readiness[ticket.id] = readiness or "risk"

    def _sync_merge_readiness(self) -> None:
        for status in COLUMN_ORDER:
            try:
                column = self.query_one(f"#column-{status.value.lower()}", KanbanColumn)
            except NoMatches:
                continue
            column.update_merge_readiness(self._merge_readiness)

    def _update_review_queue_hint(self) -> None:
        try:
            hint = self.query_one("#review-queue-hint", Static)
        except NoMatches:
            return
        review_count = sum(1 for ticket in self._tickets if ticket.status == TicketStatus.REVIEW)
        if review_count > 1:
            hint.update("Hint: multiple tickets are in REVIEW. Merging in order reduces conflicts.")
            hint.add_class("visible")
        else:
            hint.update("")
            hint.remove_class("visible")

    def _update_keybinding_hints(self) -> None:
        """Update hints based on focused card context."""
        try:
            hint_widget = self.query_one("#keybinding-hint", KeybindingHint)
        except NoMatches:
            return

        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            hints = build_keybinding_hints(None, None)
        else:
            hints = build_keybinding_hints(card.ticket.status, card.ticket.ticket_type)

        hint_widget.show_hints(hints)

    # =========================================================================
    # Navigation
    # =========================================================================

    def action_focus_left(self) -> None:
        focus.focus_horizontal(self, -1)

    def action_focus_right(self) -> None:
        focus.focus_horizontal(self, 1)

    def action_focus_up(self) -> None:
        focus.focus_vertical(self, -1)

    def action_focus_down(self) -> None:
        focus.focus_vertical(self, 1)

    def action_deselect(self) -> None:
        if self._leader_active:
            self._deactivate_leader()
            return
        try:
            overlay = self.query_one("#peek-overlay", PeekOverlay)
            if overlay.has_class("visible"):
                overlay.hide()
                return
        except NoMatches:
            pass
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
        self.app.exit()

    # =========================================================================
    # Peek Overlay
    # =========================================================================

    async def action_toggle_peek(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return
        try:
            overlay = self.query_one("#peek-overlay", PeekOverlay)
        except NoMatches:
            return
        if not overlay.toggle():
            return

        ticket = card.ticket
        scheduler = self.kagan_app.scheduler
        ticket_type = ticket.ticket_type

        if ticket_type == TicketType.AUTO:
            if scheduler.is_running(ticket.id):
                iteration = scheduler.get_iteration_count(ticket.id)
                max_iter = self.kagan_app.config.general.max_iterations
                status = f"🟢 Running (Iter {iteration}/{max_iter})"
            else:
                status = "⚪ Idle"
        else:
            status = "🟢 Session Active" if ticket.session_active else "⚪ No Active Session"

        scratchpad = await self.kagan_app.state_manager.get_scratchpad(ticket.id)
        content = scratchpad if scratchpad else "(No scratchpad)"

        overlay.update_content(ticket.short_id, ticket.title, status, content)
        x_pos = min(card.region.x + card.region.width + 2, self.size.width - 55)
        y_pos = max(1, card.region.y)
        overlay.show_at(x_pos, y_pos)

    # =========================================================================
    # Leader Key
    # =========================================================================

    def action_activate_leader(self) -> None:
        if self._leader_active:
            return
        self._leader_active = True
        try:
            hint = self.query_one(".leader-hint", Static)
            hint.add_class("visible")
        except NoMatches:
            pass
        self._leader_timer = self.set_timer(LEADER_TIMEOUT, self._leader_timeout)

    def _leader_timeout(self) -> None:
        self._deactivate_leader()

    def _deactivate_leader(self) -> None:
        self._leader_active = False
        if self._leader_timer:
            self._leader_timer.stop()
            self._leader_timer = None
        try:
            hint = self.query_one(".leader-hint", Static)
            hint.remove_class("visible")
        except NoMatches:
            pass

    def _execute_leader_action(self, action_name: str) -> None:
        self._deactivate_leader()
        is_valid, reason = self._validate_action(action_name)
        if not is_valid:
            if reason:
                self.notify(reason, severity="warning")
            return
        action_method = getattr(self, f"action_{action_name}", None)
        if action_method:
            result = action_method()
            if asyncio.iscoroutine(result):
                self.run_worker(result)

    def on_key(self, event: events.Key) -> None:
        if self._leader_active:
            leader_actions = {
                b.key: b.action for b in KANBAN_LEADER_BINDINGS if isinstance(b, Binding)
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
                self._deactivate_leader()
            return

        feedback_actions = {
            "delete_ticket_direct",
            "merge_direct",
            "edit_ticket",
            "view_details",
            "open_session",
            "start_agent",
            "watch_agent",
            "stop_agent",
            "view_diff",
            "open_review",
        }
        key_action_map = {
            b.key: b.action
            for b in KANBAN_BINDINGS
            if isinstance(b, Binding) and b.action in feedback_actions
        }
        if event.key in key_action_map:
            _, reason = self._validate_action(key_action_map[event.key])
            if reason:
                self.notify(reason, severity="warning")

    # =========================================================================
    # Search
    # =========================================================================

    def action_toggle_search(self) -> None:
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
        query = event.query.strip()
        if not query:
            self._filtered_tickets = None
        else:
            self._filtered_tickets = await self.kagan_app.state_manager.search_tickets(query)
        await self._refresh_board()

    # =========================================================================
    # Ticket Operations
    # =========================================================================

    def action_new_ticket(self) -> None:
        self.app.push_screen(TicketDetailsModal(), callback=self._on_ticket_modal_result)

    def action_new_auto_ticket(self) -> None:
        self.app.push_screen(
            TicketDetailsModal(initial_type=TicketType.AUTO),
            callback=self._on_ticket_modal_result,
        )

    async def _on_ticket_modal_result(self, result: ModalAction | Ticket | dict | None) -> None:
        if isinstance(result, Ticket):
            await self.kagan_app.state_manager.create_ticket(result)
            await self._refresh_board()
            self.notify(f"Created ticket: {result.title}")
        elif isinstance(result, dict) and self._editing_ticket_id is not None:
            await self.kagan_app.state_manager.update_ticket(self._editing_ticket_id, **result)
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
                TicketDetailsModal(
                    ticket=card.ticket,
                    start_editing=True,
                    merge_readiness=self._merge_readiness.get(card.ticket.id),
                ),
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
            if self._lifecycle:
                await self._lifecycle.delete_ticket(ticket)
            await self._refresh_board()
            self.notify(f"Deleted ticket: {ticket.title}")
            focus.focus_first_card(self)
        self._pending_delete_ticket = None

    async def action_delete_ticket_direct(self) -> None:
        card = focus.get_focused_card(self)
        if card and card.ticket:
            ticket = card.ticket
            if self._lifecycle:
                await self._lifecycle.delete_ticket(ticket)
            await self._refresh_board()
            self.notify(f"Deleted: {ticket.title}")
            focus.focus_first_card(self)

    async def action_merge_direct(self) -> None:
        ticket = self._get_review_ticket(focus.get_focused_card(self))
        if not ticket:
            return
        if self._lifecycle and await self._lifecycle.has_no_changes(ticket):
            success, message = await self._lifecycle.close_exploratory(ticket)
            if success:
                await self._refresh_board()
                self.notify(f"Closed as exploratory: {ticket.title}")
            else:
                self.notify(message, severity="error")
            return
        self.notify("Merging... (this may take a few seconds)", severity="information")
        success, message = (
            await self._lifecycle.merge_ticket(ticket) if self._lifecycle else (False, "")
        )
        if success:
            await self._refresh_board()
            self.notify(f"Merged: {ticket.title}", severity="information")
        else:
            self.notify(KanbanScreen._format_merge_failure(ticket, message), severity="error")

    async def _move_ticket(self, forward: bool) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return
        ticket = card.ticket
        status = ticket.status
        ticket_type = ticket.ticket_type

        new_status = (
            TicketStatus.next_status(status) if forward else TicketStatus.prev_status(status)
        )
        if new_status:
            if status == TicketStatus.IN_PROGRESS and ticket_type == TicketType.AUTO:
                self._pending_auto_move_ticket = ticket
                self._pending_auto_move_status = new_status
                title = ticket.title[:NOTIFICATION_TITLE_MAX_LENGTH]
                destination = new_status.value.upper()
                self.app.push_screen(
                    ConfirmModal(
                        title="Stop Agent and Move Ticket?",
                        message=(
                            f"Stop agent, keep worktree/logs, and move '{title}' to {destination}?"
                        ),
                    ),
                    callback=self._on_auto_move_confirmed,
                )
                return

            if status == TicketStatus.REVIEW and new_status == TicketStatus.DONE:
                if self._lifecycle and await self._lifecycle.has_no_changes(ticket):
                    self._pending_close_ticket = ticket
                    title = ticket.title[:NOTIFICATION_TITLE_MAX_LENGTH]
                    self.app.push_screen(
                        ConfirmModal(
                            title="Close as Exploratory?",
                            message=f"Close '{title}' with no changes?",
                        ),
                        callback=self._on_close_confirmed,
                    )
                    return
                self._pending_merge_ticket = ticket
                title = ticket.title[:NOTIFICATION_TITLE_MAX_LENGTH]
                self.app.push_screen(
                    ConfirmModal(
                        title="Complete Ticket?",
                        message=f"Merge '{title}' and move to DONE?",
                    ),
                    callback=self._on_merge_confirmed,
                )
                return

            if (
                status == TicketStatus.IN_PROGRESS
                and ticket_type == TicketType.PAIR
                and new_status == TicketStatus.REVIEW
            ):
                self._pending_advance_ticket = ticket
                title = ticket.title[:NOTIFICATION_TITLE_MAX_LENGTH]
                self.app.push_screen(
                    ConfirmModal(title="Advance to Review?", message=f"Move '{title}' to REVIEW?"),
                    callback=self._on_advance_confirmed,
                )
                return

            # If moving AUTO ticket out of IN_PROGRESS, clear agent state immediately
            if (
                ticket_type == TicketType.AUTO
                and status == TicketStatus.IN_PROGRESS
                and new_status != TicketStatus.REVIEW
            ):
                # Clear agent state on UI before moving
                column = self.query_one("#column-in_progress", KanbanColumn)
                column.update_iterations({ticket.id: ""})
                for c in column.get_cards():
                    if c.ticket and c.ticket.id == ticket.id:
                        c.is_agent_active = False

            await self.kagan_app.state_manager.move_ticket(ticket.id, new_status)
            await self._refresh_board()
            self.notify(f"Moved #{ticket.id} to {new_status.value}")
            focus.focus_column(self, new_status)
        else:
            self.notify(f"Already in {'final' if forward else 'first'} status", severity="warning")

    async def _on_merge_confirmed(self, confirmed: bool | None) -> None:
        if confirmed and self._pending_merge_ticket:
            ticket = self._pending_merge_ticket
            self.notify("Merging... (this may take a few seconds)", severity="information")
            success, message = (
                await self._lifecycle.merge_ticket(ticket) if self._lifecycle else (False, "")
            )
            if success:
                await self._refresh_board()
                self.notify(f"Merged and completed: {ticket.title}", severity="information")
            else:
                self.notify(KanbanScreen._format_merge_failure(ticket, message), severity="error")
        self._pending_merge_ticket = None

    async def _on_close_confirmed(self, confirmed: bool | None) -> None:
        if confirmed and self._pending_close_ticket:
            ticket = self._pending_close_ticket
            success, message = (
                await self._lifecycle.close_exploratory(ticket) if self._lifecycle else (False, "")
            )
            if success:
                await self._refresh_board()
                self.notify(f"Closed as exploratory: {ticket.title}")
            else:
                self.notify(message, severity="error")
        self._pending_close_ticket = None

    async def _on_advance_confirmed(self, confirmed: bool | None) -> None:
        if confirmed and self._pending_advance_ticket:
            ticket = self._pending_advance_ticket
            await self.kagan_app.state_manager.update_ticket(
                ticket.id,
                status=TicketStatus.REVIEW,
                merge_failed=False,
                merge_error=None,
                merge_readiness=MergeReadiness.RISK,
            )
            await self.kagan_app.state_manager.append_ticket_event(
                ticket.id, "review", "Moved to REVIEW"
            )
            await self._refresh_board()
            self.notify(f"Moved #{ticket.id} to REVIEW")
            focus.focus_column(self, TicketStatus.REVIEW)
        self._pending_advance_ticket = None

    async def _on_auto_move_confirmed(self, confirmed: bool | None) -> None:
        ticket = self._pending_auto_move_ticket
        new_status = self._pending_auto_move_status
        self._pending_auto_move_ticket = None
        self._pending_auto_move_status = None

        if not confirmed or ticket is None or new_status is None:
            return

        scheduler = self.kagan_app.scheduler
        if scheduler.is_running(ticket.id):
            await scheduler.stop_ticket(ticket.id)

        # Clear agent state immediately on UI to prevent stale indicators
        try:
            column = self.query_one("#column-in_progress", KanbanColumn)
            column.update_iterations({ticket.id: ""})
            for card in column.get_cards():
                if card.ticket and card.ticket.id == ticket.id:
                    card.is_agent_active = False
        except Exception:
            pass  # Column might not exist yet

        await self.kagan_app.state_manager.move_ticket(ticket.id, new_status)
        await self._refresh_board()
        self.notify(f"Moved #{ticket.id} to {new_status.value} (agent stopped)")
        focus.focus_column(self, new_status)

    async def action_move_forward(self) -> None:
        await self._move_ticket(forward=True)

    async def action_move_backward(self) -> None:
        await self._move_ticket(forward=False)

    async def action_duplicate_ticket(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            self.notify("No ticket selected", severity="warning")
            return
        from kagan.ui.modals.duplicate_ticket import DuplicateTicketModal

        self.app.push_screen(
            DuplicateTicketModal(source_ticket=card.ticket),
            callback=self._on_duplicate_result,
        )

    async def _on_duplicate_result(self, result: Ticket | None) -> None:
        if result:
            ticket = await self.kagan_app.state_manager.create_ticket(result)
            await self._refresh_board()
            self.notify(f"Created duplicate: #{ticket.short_id}")
            focus.focus_column(self, TicketStatus.BACKLOG)

    def action_copy_ticket_id(self) -> None:
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
                TicketDetailsModal(
                    ticket=card.ticket,
                    merge_readiness=self._merge_readiness.get(card.ticket.id),
                ),
                callback=self._on_ticket_modal_result,
            )

    def action_expand_description(self) -> None:
        """Expand description in full-screen editor (read-only from Kanban)."""
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            self.notify("No ticket selected", severity="warning")
            return
        description = card.ticket.description or ""
        modal = DescriptionEditorModal(
            description=description, readonly=True, title="View Description"
        )
        self.app.push_screen(modal)

    # =========================================================================
    # Session Operations (inlined from SessionController)
    # =========================================================================

    async def action_open_session(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return
        ticket = card.ticket
        if ticket.status == TicketStatus.REVIEW:
            await self.action_open_review()
            return

        # Only PAIR tickets need manual session opening
        if ticket.ticket_type != TicketType.PAIR:
            return

        # Ensure worktree exists
        wt_path = await self.kagan_app.worktree_manager.get_path(ticket.id)
        if wt_path is None:
            self.notify("Creating worktree...", severity="information")
            base = self.kagan_app.config.general.default_base_branch
            wt_path = await self.kagan_app.worktree_manager.create(ticket.id, ticket.title, base)
            self.notify("Worktree created", severity="information")

        # Create session if doesn't exist
        if not await self.kagan_app.session_manager.session_exists(ticket.id):
            self.notify("Creating session...", severity="information")
            await self.kagan_app.session_manager.create_session(ticket, wt_path)

        # Show TmuxGatewayModal if not skipped
        if not self.kagan_app.config.ui.skip_tmux_gateway:
            from kagan.ui.modals.tmux_gateway import TmuxGatewayModal

            def on_gateway_result(result: str | None) -> None:
                if result is None:
                    return  # User cancelled
                if result == "skip_future":
                    self.kagan_app.config.ui.skip_tmux_gateway = True
                    cb_result = self._save_tmux_gateway_preference(skip=True)
                    if asyncio.iscoroutine(cb_result):
                        asyncio.create_task(cb_result)
                # Proceed to open tmux session
                self.app.call_later(self._do_open_pair_session, ticket)

            self.app.push_screen(TmuxGatewayModal(ticket.id, ticket.title), on_gateway_result)
            return

        # Skip modal - open directly
        await self._do_open_pair_session(ticket)

    async def _do_open_pair_session(self, ticket: Ticket) -> None:
        """Open the tmux session after modal confirmation."""
        try:
            # Move BACKLOG to IN_PROGRESS if needed
            if ticket.status == TicketStatus.BACKLOG:
                await self.kagan_app.state_manager.update_ticket(
                    ticket.id, status=TicketStatus.IN_PROGRESS
                )
                await self._refresh_board()

            # Suspend app and attach to session
            with self.app.suspend():
                self.kagan_app.session_manager.attach_session(ticket.id)

            # Check if session still exists after returning from attach
            session_still_exists = await self.kagan_app.session_manager.session_exists(ticket.id)
            if session_still_exists:
                # User detached, session is still active
                return

            # Session terminated - prompt to move to REVIEW
            from kagan.ui.modals.confirm import ConfirmModal

            def on_confirm(result: bool | None) -> None:
                if result:

                    async def move_to_review() -> None:
                        await self.kagan_app.state_manager.update_ticket(
                            ticket.id, status=TicketStatus.REVIEW
                        )
                        await self._refresh_board()

                    self.app.call_later(move_to_review)

            self.app.push_screen(
                ConfirmModal("Session Complete", "Move ticket to REVIEW?"),
                on_confirm,
            )

        except Exception as e:
            from kagan.sessions.tmux import TmuxError

            if isinstance(e, TmuxError):
                self.notify(f"Tmux error: {e}", severity="error")

    # =========================================================================
    # Agent Operations (inlined from SessionController)
    # =========================================================================

    async def action_watch_agent(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return
        ticket = card.ticket

        # AUTO tickets: Show agent output modal
        if ticket.ticket_type == TicketType.AUTO:
            # Show modal if agent is running OR ticket is IN_PROGRESS/REVIEW
            # (IN_PROGRESS: agent may be starting, REVIEW: can view historical logs)
            is_running = self.kagan_app.scheduler.is_running(ticket.id)
            if not is_running and ticket.status not in (
                TicketStatus.IN_PROGRESS,
                TicketStatus.REVIEW,
            ):
                self.notify("No agent running for this ticket", severity="warning")
                return

            agent = self.kagan_app.scheduler.get_running_agent(ticket.id)
            iteration = self.kagan_app.scheduler.get_iteration_count(ticket.id)

            if agent is None:
                # Agent not available yet - check if we have logs to show
                logs = await self.kagan_app.state_manager.get_agent_logs(
                    ticket.id, log_type="implementation"
                )
                if logs:
                    await self.app.push_screen(
                        AgentOutputModal(
                            ticket=ticket,
                            agent=None,
                            iteration=iteration,
                            historical_logs={"implementation": logs},
                        )
                    )
                else:
                    self.notify(
                        "No agent logs available yet (agent still starting)", severity="warning"
                    )
                return

            await self.app.push_screen(
                AgentOutputModal(
                    ticket=ticket,
                    agent=agent,
                    iteration=iteration,
                )
            )
        # PAIR tickets: Attach to tmux session
        else:
            if not await self.kagan_app.session_manager.session_exists(ticket.id):
                self.notify("No active session for this ticket", severity="warning")
                return

            # Attach to existing session
            with self.app.suspend():
                self.kagan_app.session_manager.attach_session(ticket.id)

    async def action_start_agent(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return
        ticket = card.ticket

        # Only AUTO tickets
        if ticket.ticket_type == TicketType.PAIR:
            return

        if self.kagan_app.scheduler.is_running(ticket.id):
            self.notify(
                "Agent already running for this ticket (press w to watch)", severity="warning"
            )
            return

        # Move BACKLOG tickets to IN_PROGRESS first
        if ticket.status == TicketStatus.BACKLOG:
            await self.kagan_app.state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)
            # Refresh ticket to get updated status
            refreshed = await self.kagan_app.state_manager.get_ticket(ticket.id)
            if refreshed:
                ticket = refreshed
            await self._refresh_board()

        # Show immediate feedback
        self.notify("Starting agent...", severity="information")

        # Delegate to scheduler
        result = self.kagan_app.scheduler.spawn_for_ticket(ticket)
        # Handle both async and sync returns for test compatibility
        if hasattr(result, "__await__"):
            spawned = await result
        else:
            spawned = result

        if spawned:
            self.notify(f"Agent started: {ticket.id[:8]}", severity="information")
        else:
            self.notify("Failed to start agent (at capacity?)", severity="warning")

    async def action_stop_agent(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return
        ticket = card.ticket

        if not self.kagan_app.scheduler.is_running(ticket.id):
            self.notify("No agent running for this ticket", severity="warning")
            return

        # Show immediate feedback
        self.notify("Stopping agent...", severity="information")

        result = self.kagan_app.scheduler.stop_ticket(ticket.id)
        # Handle both async and sync returns for test compatibility
        if hasattr(result, "__await__"):
            await result

        self.notify(f"Agent stopped: {ticket.id[:8]}", severity="information")

    # =========================================================================
    # Screen Navigation
    # =========================================================================

    def action_open_planner(self) -> None:
        self.app.push_screen(PlannerScreen(agent_factory=self.kagan_app._agent_factory))

    async def action_open_settings(self) -> None:
        from kagan.ui.modals import SettingsModal

        config = self.kagan_app.config
        config_path = self.kagan_app.config_path
        result = await self.app.push_screen(SettingsModal(config, config_path))
        if result:
            self.kagan_app.config = self.kagan_app.config.load(config_path)
            self.notify("Settings saved")

    # =========================================================================
    # Review Operations
    # =========================================================================

    def _get_review_ticket(self, card: TicketCard | None) -> Ticket | None:
        """Get ticket from card if it's in REVIEW status."""
        if not card or not card.ticket:
            return None
        if card.ticket.status != TicketStatus.REVIEW:
            self.notify("Ticket is not in REVIEW", severity="warning")
            return None
        return card.ticket

    async def action_merge(self) -> None:
        ticket = self._get_review_ticket(focus.get_focused_card(self))
        if not ticket:
            return
        if self._lifecycle and await self._lifecycle.has_no_changes(ticket):
            success, message = await self._lifecycle.close_exploratory(ticket)
            if success:
                await self._refresh_board()
                self.notify(f"Closed as exploratory: {ticket.title}")
            else:
                self.notify(message, severity="error")
            return
        self.notify("Merging... (this may take a few seconds)", severity="information")
        success, message = (
            await self._lifecycle.merge_ticket(ticket) if self._lifecycle else (False, "")
        )
        if success:
            await self._refresh_board()
            self.notify(f"Merged and completed: {ticket.title}", severity="information")
        else:
            self.notify(KanbanScreen._format_merge_failure(ticket, message), severity="error")

    async def action_view_diff(self) -> None:
        ticket = self._get_review_ticket(focus.get_focused_card(self))
        if not ticket:
            return
        worktree = self.kagan_app.worktree_manager
        base = self.kagan_app.config.general.default_base_branch
        diff_text = await worktree.get_diff(ticket.id, base_branch=base)  # type: ignore[misc]
        title = f"Diff: {ticket.short_id} {ticket.title[:NOTIFICATION_TITLE_MAX_LENGTH]}"

        await self.app.push_screen(
            DiffModal(title=title, diff_text=diff_text, ticket=ticket),
            callback=lambda result: self._on_diff_result(ticket, result),
        )

    async def _on_diff_result(self, ticket: Ticket, result: str | None) -> None:
        if result == "approve":
            if self._lifecycle and await self._lifecycle.has_no_changes(ticket):
                success, message = await self._lifecycle.close_exploratory(ticket)
                if success:
                    await self._refresh_board()
                    self.notify(f"Closed as exploratory: {ticket.title}")
                else:
                    self.notify(message, severity="error")
                return
            self.notify("Merging... (this may take a few seconds)", severity="information")
            success, message = (
                await self._lifecycle.merge_ticket(ticket) if self._lifecycle else (False, "")
            )
            if success:
                await self._refresh_board()
                self.notify(f"Merged: {ticket.title}", severity="information")
            else:
                self.notify(KanbanScreen._format_merge_failure(ticket, message), severity="error")
        elif result == "reject":
            await self._handle_reject_with_feedback(ticket)

    async def action_open_review(self) -> None:
        ticket = self._get_review_ticket(focus.get_focused_card(self))
        if not ticket:
            return

        agent_config = ticket.get_agent_config(self.kagan_app.config)
        await self.app.push_screen(
            ReviewModal(
                ticket=ticket,
                worktree_manager=self.kagan_app.worktree_manager,
                agent_config=agent_config,
                base_branch=self.kagan_app.config.general.default_base_branch,
                agent_factory=self.kagan_app._agent_factory,
            ),
            callback=self._on_review_result,
        )

    async def _on_review_result(self, result: str | None) -> None:
        ticket = self._get_review_ticket(focus.get_focused_card(self))
        if not ticket:
            return
        if result == "approve":
            self.notify("Merging... (this may take a few seconds)", severity="information")
            success, message = (
                await self._lifecycle.merge_ticket(ticket) if self._lifecycle else (False, "")
            )
            if success:
                await self._refresh_board()
                self.notify(f"Merged and completed: {ticket.title}", severity="information")
            else:
                self.notify(KanbanScreen._format_merge_failure(ticket, message), severity="error")
        elif result == "exploratory":
            success, message = (
                await self._lifecycle.close_exploratory(ticket) if self._lifecycle else (False, "")
            )
            if success:
                await self._refresh_board()
                self.notify(f"Closed as exploratory: {ticket.title}")
            else:
                self.notify(message, severity="error")
        elif result == "reject":
            await self._handle_reject_with_feedback(ticket)

    @staticmethod
    def _format_merge_failure(ticket: Ticket, message: str) -> str:
        ticket_type = ticket.ticket_type
        if ticket_type == TicketType.AUTO:
            return f"Merge failed (AUTO): {message}"
        return f"Merge failed (PAIR): {message}"

    async def _handle_reject_with_feedback(self, ticket: Ticket) -> None:
        ticket_type = ticket.ticket_type
        if ticket_type == TicketType.AUTO:
            await self.app.push_screen(
                RejectionInputModal(ticket.title),
                callback=lambda result: self._apply_rejection_result(ticket, result),
            )
        else:
            await self.kagan_app.state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)
            await self._refresh_board()
            self.notify(f"Moved back to IN_PROGRESS: {ticket.title}")

    async def _apply_rejection_result(self, ticket: Ticket, result: tuple[str, str] | None) -> None:
        if self._lifecycle is None:
            return
        if result is None:
            await self._lifecycle.apply_rejection_feedback(ticket, None, "shelve")
        else:
            feedback, action = result
            await self._lifecycle.apply_rejection_feedback(ticket, feedback, action)
        await self._refresh_board()
        if result is None:
            self.notify(f"Shelved: {ticket.title}")
        elif result[1] == "retry":
            self.notify(f"Retrying: {ticket.title}")
        else:
            self.notify(f"Staged for manual restart: {ticket.title}")

    # =========================================================================
    # Config Persistence
    # =========================================================================

    async def _save_tmux_gateway_preference(self, skip: bool = True) -> None:
        """Save tmux gateway preference to config."""
        try:
            await self.kagan_app.config.update_ui_preferences(
                self.kagan_app.config_path,
                skip_tmux_gateway=skip,
            )
        except Exception as e:
            self.notify(f"Failed to save preference: {e}", severity="error")

    # =========================================================================
    # Message Handlers
    # =========================================================================

    def on_ticket_card_selected(self, message: TicketCard.Selected) -> None:
        self.action_view_details()
