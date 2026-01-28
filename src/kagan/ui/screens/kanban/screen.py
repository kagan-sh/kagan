"""Main Kanban board screen."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from textual import getters, on
from textual.binding import Binding
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
from kagan.ui.widgets.card import TicketCard  # noqa: TC001 - used at runtime for messages
from kagan.ui.widgets.column import KanbanColumn
from kagan.ui.widgets.header import KaganHeader
from kagan.ui.widgets.search_bar import SearchBar

if TYPE_CHECKING:
    from textual import events
    from textual.app import ComposeResult
    from textual.timer import Timer

# Leader key timeout in seconds
LEADER_TIMEOUT = 0.8

SIZE_WARNING_MESSAGE = (
    f"Terminal too small\n\n"
    f"Minimum size: {MIN_SCREEN_WIDTH}x{MIN_SCREEN_HEIGHT}\n"
    f"Please resize your terminal"
)


class KanbanScreen(KaganScreen):
    """Main Kanban board screen with 4 columns."""

    BINDINGS = [
        # Visible bindings
        Binding("q", "quit", "Quit", priority=True),
        Binding("n", "new_ticket", "New"),
        Binding("v", "view_details", "View"),
        Binding("e", "edit_ticket", "Edit"),
        Binding("enter", "open_session", "Open"),
        Binding("g", "activate_leader", "Go..."),
        Binding("slash", "toggle_search", "Search", key_display="/"),
        # Hidden navigation (intuitive vim-style, no footer display but shown in palette)
        Binding("h", "focus_left", "Left", show=False),
        Binding("j", "focus_down", "Down", show=False),
        Binding("k", "focus_up", "Up", show=False),
        Binding("l", "focus_right", "Right", show=False),
        Binding("left", "focus_left", "Left", show=False),
        Binding("right", "focus_right", "Right", show=False),
        Binding("down", "focus_down", "Down", show=False),
        Binding("up", "focus_up", "Up", show=False),
        # Ticket management
        Binding("a", "start_agent", "Start agent"),
        # Hidden specialized (will move to leader key)
        Binding("w", "watch_agent", "Watch agent", show=False),
        Binding("D", "view_diff", "Diff", show=False),
        Binding("r", "open_review", "Review", show=False),
        Binding("p", "open_planner", "Plan Mode", show=True),
        # Hidden utility
        Binding("escape", "deselect", show=False),
        Binding("ctrl+c", "interrupt", show=False),
        Binding("ctrl+comma", "open_settings", "Settings", show=True),
        Binding("ctrl+d", "delete_ticket_direct", "Delete", show=True),
        Binding("ctrl+m", "merge_direct", "Merge PR", show=True),
    ]

    header = getters.query_one(KaganHeader)

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self._tickets: list[Ticket] = []
        self._filtered_tickets: list[Ticket] | None = None  # None = no filter active
        self._pending_delete_ticket: Ticket | None = None
        self._pending_merge_ticket: Ticket | None = None
        self._pending_reopen_ticket: Ticket | None = None
        self._pending_advance_ticket: Ticket | None = None
        self._editing_ticket_id: str | None = None
        # Leader key state
        self._leader_active: bool = False
        self._leader_timer: Timer | None = None

    def check_action(self, action: str, parameters: tuple[object, ...]) -> bool | None:
        card = focus.get_focused_card(self)
        ticket = card.ticket if card else None

        if action in (
            "edit_ticket",
            "delete_ticket",
            "delete_ticket_direct",
            "view_details",
        ):
            return True if ticket else None

        if action in ("move_forward", "move_backward"):
            if not ticket:
                return None
            if ticket.status == TicketStatus.IN_PROGRESS:
                ticket_type = ticket.ticket_type
                if isinstance(ticket_type, str):
                    ticket_type = TicketType(ticket_type)
                if ticket_type == TicketType.AUTO:
                    return None
            return True

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

        if action == "open_session":
            return True if ticket else None

        return True

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
            "g: h=Ticket← l=Ticket→ d=Diff r=Review w=Watch | Esc=Cancel",
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
        else:
            column.update_iterations({ticket_id: ""})

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

    def on_descendant_focus(self, event: events.DescendantFocus) -> None:
        self.refresh_bindings()

    def on_resize(self, event: events.Resize) -> None:
        self._check_screen_size()

    async def on_screen_resume(self) -> None:
        await self._refresh_board()
        self._sync_iterations()

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
        # Call the appropriate action
        action_method = getattr(self, f"action_{action_name}", None)
        if action_method:
            result = action_method()
            # Handle async actions
            if asyncio.iscoroutine(result):
                self.run_worker(result)

    def on_key(self, event: events.Key) -> None:
        """Handle key events for leader key sequences."""
        if not self._leader_active:
            return

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

        if not forward and status == TicketStatus.DONE:
            self._pending_reopen_ticket = ticket
            title = ticket.title[:NOTIFICATION_TITLE_MAX_LENGTH]
            if ticket_type == TicketType.AUTO:
                msg = f"Reopen '{title}' and move to BACKLOG?\n\nThis will reset review state."
            else:
                msg = f"Reopen '{title}' and move to BACKLOG?"
            self.app.push_screen(
                ConfirmModal(title="Reopen Ticket?", message=msg),
                callback=self._on_reopen_confirmed,
            )
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

    async def _on_reopen_confirmed(self, confirmed: bool | None) -> None:
        if confirmed and self._pending_reopen_ticket:
            ticket = self._pending_reopen_ticket
            await actions.reopen_ticket(self.kagan_app, ticket)
            await self._refresh_board()
            self.notify(f"Reopened: {ticket.title}")
            focus.focus_column(self, TicketStatus.BACKLOG)
        self._pending_reopen_ticket = None

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

        if not config.general.auto_start and not manual:
            msg = (
                "AUTO mode requires auto_start=true in .kagan/config.toml "
                "(or use 'a' to start manually)"
            )
            self.notify(msg, severity="warning")
            return

        if ticket.status == TicketStatus.BACKLOG:
            await self.kagan_app.state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)
            await self._refresh_board()
            self.notify(f"Started AUTO ticket: {ticket.short_id}")
            return

        if scheduler.is_running(ticket.id):
            from kagan.ui.modals.agent_output import AgentOutputModal

            agent = scheduler.get_running_agent(ticket.id)
            iteration = scheduler.get_iteration_count(ticket.id)
            modal = AgentOutputModal(ticket=ticket, agent=agent, iteration=iteration)
            await self.app.push_screen(modal)
        else:
            msg = "Agent not running yet. Will start on next scheduler tick."
            self.notify(msg, severity="warning")

    async def _open_pair_session(self, ticket: Ticket) -> None:
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

    # Message handlers
    def on_ticket_card_selected(self, message: TicketCard.Selected) -> None:
        self.action_view_details()
