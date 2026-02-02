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
from kagan.database.models import Ticket, TicketStatus, TicketType
from kagan.keybindings import (
    KANBAN_BINDINGS,
    KANBAN_LEADER_BINDINGS,
    generate_leader_hint,
)
from kagan.ui.modals import (
    ConfirmModal,
    DiffModal,
    ModalAction,
    RejectionInputModal,
    ReviewModal,
    TicketDetailsModal,
)
from kagan.ui.modals.description_editor import DescriptionEditorModal
from kagan.ui.screens.base import KaganScreen
from kagan.ui.screens.kanban import actions, focus
from kagan.ui.screens.kanban.agent_controller import AgentController
from kagan.ui.screens.kanban.session_handler import SessionHandler
from kagan.ui.screens.kanban.validation import ActionValidator
from kagan.ui.screens.planner import PlannerScreen
from kagan.ui.utils import coerce_enum, copy_with_notification
from kagan.ui.widgets.card import TicketCard  # noqa: TC001 - needed at runtime for message handler
from kagan.ui.widgets.column import KanbanColumn
from kagan.ui.widgets.header import KaganHeader
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
        self._pending_advance_ticket: Ticket | None = None
        self._editing_ticket_id: str | None = None
        self._leader_active: bool = False
        self._leader_timer: Timer | None = None
        # Extracted components (initialized on mount)
        self._validator: ActionValidator | None = None
        self._session_handler: SessionHandler | None = None
        self._agent_controller: AgentController | None = None

    def _init_components(self) -> None:
        """Initialize extracted component classes."""
        from textual.widget import Widget

        SeverityLevel = Widget.notify.__annotations__["severity"]

        def notify_adapter(msg: str, severity: SeverityLevel = "information") -> None:
            self.notify(msg, severity=severity)

        self._validator = ActionValidator(self.kagan_app.scheduler)
        self._session_handler = SessionHandler(
            kagan_app=self.kagan_app,
            app=self.app,
            notify=notify_adapter,
            push_screen=self.app.push_screen,
            refresh_board=self._refresh_board,
        )
        self._session_handler.set_skip_tmux_gateway_callback(self._save_tmux_gateway_preference)
        self._agent_controller = AgentController(
            kagan_app=self.kagan_app,
            notify=notify_adapter,
            push_screen=self.app.push_screen,
            refresh_board=self._refresh_board,
        )

    def _validate_action(self, action: str) -> tuple[bool, str | None]:
        """Validate if an action can be performed."""
        card = focus.get_focused_card(self)
        ticket = card.ticket if card else None
        if self._validator is None:
            return (True, None)
        return self._validator.validate(action, ticket)

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
        yield Static(generate_leader_hint(KANBAN_LEADER_BINDINGS), classes="leader-hint")
        yield PeekOverlay(id="peek-overlay")
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
        self._editing_ticket_id = None
        self._filtered_tickets = None

    async def _on_ticket_changed(self, _ticket_id: str) -> None:
        await self._refresh_board()
        self._sync_iterations()
        self._sync_agent_states()

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
        ticket_type = coerce_enum(ticket.ticket_type, TicketType)

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
        card = focus.get_focused_card(self)
        if card and card.ticket:
            ticket = card.ticket
            await actions.delete_ticket(self.kagan_app, ticket)
            await self._refresh_board()
            self.notify(f"Deleted: {ticket.title}")
            focus.focus_first_card(self)

    async def action_merge_direct(self) -> None:
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
        status = coerce_enum(ticket.status, TicketStatus)
        ticket_type = coerce_enum(ticket.ticket_type, TicketType)

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
                TicketDetailsModal(ticket=card.ticket),
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
    # Session Operations (delegated to SessionHandler)
    # =========================================================================

    async def action_open_session(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return
        ticket = card.ticket
        if ticket.status == TicketStatus.REVIEW:
            await self.action_open_review()
            return
        if self._session_handler:
            await self._session_handler.open_session(ticket)

    # =========================================================================
    # Agent Operations (delegated to AgentController)
    # =========================================================================

    async def action_watch_agent(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return
        if self._agent_controller:
            await self._agent_controller.watch_agent(card.ticket)

    async def action_start_agent(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return
        if self._session_handler:
            await self._session_handler.start_agent_manual(card.ticket)

    async def action_stop_agent(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.ticket:
            return
        if self._agent_controller:
            await self._agent_controller.stop_agent(card.ticket)

    # =========================================================================
    # Screen Navigation
    # =========================================================================

    def action_open_planner(self) -> None:
        self.app.push_screen(PlannerScreen())

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

        await self.app.push_screen(
            DiffModal(title=title, diff_text=diff_text, ticket=ticket),
            callback=lambda result: self._on_diff_result(ticket, result),
        )

    async def _on_diff_result(self, ticket: Ticket, result: str | None) -> None:
        if result == "approve":
            success, message = await actions.merge_ticket(self.kagan_app, ticket)
            if success:
                await self._refresh_board()
                self.notify(f"Merged: {ticket.title}")
            else:
                self.notify(message, severity="error")
        elif result == "reject":
            await self._handle_reject_with_feedback(ticket)

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
        ticket_type = coerce_enum(ticket.ticket_type, TicketType)
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
        if result is None:
            await actions.apply_rejection_feedback(self.kagan_app, ticket, None, "shelve")
        else:
            feedback, action = result
            await actions.apply_rejection_feedback(self.kagan_app, ticket, feedback, action)
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

    def _save_tmux_gateway_preference(self) -> None:
        import re

        config_path = self.kagan_app.config_path
        if not config_path.exists():
            config_path.parent.mkdir(exist_ok=True)
            config_path.write_text("[ui]\nskip_tmux_gateway = true\n")
            return
        content = config_path.read_text()
        if "[ui]" not in content:
            if not content.endswith("\n"):
                content += "\n"
            content += "\n[ui]\nskip_tmux_gateway = true\n"
        elif "skip_tmux_gateway" in content:
            content = re.sub(
                r"skip_tmux_gateway\s*=\s*(true|false)", "skip_tmux_gateway = true", content
            )
        else:
            content = re.sub(r"(\[ui\])", r"\1\nskip_tmux_gateway = true", content)
        config_path.write_text(content)

    # =========================================================================
    # Message Handlers
    # =========================================================================

    def on_ticket_card_selected(self, message: TicketCard.Selected) -> None:
        self.action_view_details()
