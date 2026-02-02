"""Session handling for Kanban screen ticket operations."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from kagan.agents.worktree import WorktreeError
from kagan.database.models import TicketStatus, TicketType
from kagan.sessions.tmux import TmuxError
from kagan.ui.utils import coerce_enum

if TYPE_CHECKING:
    from collections.abc import Callable

    from textual.app import App

    from kagan.app import KaganApp
    from kagan.database.models import Ticket


class SessionHandler:
    """Handles opening and managing sessions for tickets.

    Routes between AUTO (agent) and PAIR (tmux) sessions based on ticket type.
    """

    def __init__(
        self,
        kagan_app: KaganApp,
        app: App,
        notify: Callable[[str, str], None],
        push_screen: Callable,
        refresh_board: Callable,
    ) -> None:
        """Initialize session handler.

        Args:
            kagan_app: The KaganApp instance for accessing managers.
            app: The Textual App instance for screen operations.
            notify: Callback for showing notifications (message, severity).
            push_screen: Callback for pushing screens/modals.
            refresh_board: Async callback for refreshing the board.
        """
        self._kagan_app = kagan_app
        self._app = app
        self._notify = notify
        self._push_screen = push_screen
        self._refresh_board = refresh_board
        self._skip_tmux_gateway_callback: Callable[[], None] | None = None

    def set_skip_tmux_gateway_callback(self, callback: Callable[[], None]) -> None:
        """Set callback for saving tmux gateway skip preference.

        Args:
            callback: Function to call when user chooses to skip gateway in future.
        """
        self._skip_tmux_gateway_callback = callback

    async def open_session(self, ticket: Ticket) -> None:
        """Open appropriate session based on ticket type.

        For AUTO tickets, opens agent output modal or starts agent.
        For PAIR tickets, opens tmux session with worktree.

        Args:
            ticket: The ticket to open a session for.
        """
        ticket_type = coerce_enum(ticket.ticket_type, TicketType)

        if ticket_type == TicketType.AUTO:
            await self._open_auto_session(ticket)
        else:
            await self._open_pair_session(ticket)

    async def _open_auto_session(self, ticket: Ticket, manual: bool = False) -> None:
        """Open session for AUTO ticket.

        Handles starting agents and showing agent output modal.

        Args:
            ticket: The AUTO ticket.
            manual: If True, bypasses auto_start check and starts agent directly.
        """
        scheduler = self._kagan_app.scheduler
        config = self._kagan_app.config

        # Handle BACKLOG tickets - move to IN_PROGRESS first
        if ticket.status == TicketStatus.BACKLOG:
            await self._kagan_app.state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)
            # Refresh ticket to get updated status
            refreshed = await self._kagan_app.state_manager.get_ticket(ticket.id)
            if refreshed:
                ticket = refreshed
            await self._refresh_board()

            # For manual start, spawn agent directly
            if manual:
                spawned = await scheduler.spawn_for_ticket(ticket)
                if spawned:
                    self._notify(f"Started agent for: {ticket.short_id}", "information")
                else:
                    self._notify("Failed to start agent (at capacity?)", "warning")
                return

            # For auto mode, notify and let scheduler pick it up
            if config.general.auto_start:
                self._notify(f"Started AUTO ticket: {ticket.short_id}", "information")
            else:
                self._notify(
                    "Ticket moved to IN_PROGRESS. Press [a] to start agent manually.",
                    "warning",
                )
            return

        # Ticket already in IN_PROGRESS - check if agent is running
        if scheduler.is_running(ticket.id):
            from kagan.ui.modals.agent_output import AgentOutputModal

            agent = scheduler.get_running_agent(ticket.id)
            iteration = scheduler.get_iteration_count(ticket.id)
            modal = AgentOutputModal(ticket=ticket, agent=agent, iteration=iteration)
            await self._push_screen(modal)
        elif manual:
            # Manual start for IN_PROGRESS ticket that's not running
            spawned = await scheduler.spawn_for_ticket(ticket)
            if spawned:
                self._notify(f"Started agent for: {ticket.short_id}", "information")
                # Optionally open watch modal immediately
                from kagan.ui.modals.agent_output import AgentOutputModal

                agent = scheduler.get_running_agent(ticket.id)
                iteration = scheduler.get_iteration_count(ticket.id)
                modal = AgentOutputModal(ticket=ticket, agent=agent, iteration=iteration)
                await self._push_screen(modal)
            else:
                self._notify("Failed to start agent (at capacity?)", "warning")
        else:
            # Auto mode not enabled and not manually triggered
            if not config.general.auto_start:
                self._notify(
                    "Auto-start disabled. Press a to start manually.",
                    "warning",
                )
            else:
                self._notify("Agent starting on next tick (~5 seconds)", "information")

    async def start_agent_manual(self, ticket: Ticket) -> None:
        """Manually start an agent for AUTO ticket.

        Args:
            ticket: The AUTO ticket to start agent for.
        """
        await self._open_auto_session(ticket, manual=True)

    async def _open_pair_session(self, ticket: Ticket) -> None:
        """Open session for PAIR ticket with optional gateway modal.

        Args:
            ticket: The PAIR ticket.
        """
        # Check if user wants to skip the gateway modal
        if not self._kagan_app.config.ui.skip_tmux_gateway:
            from kagan.ui.modals.tmux_gateway import TmuxGatewayModal

            def on_gateway_result(result: str | None) -> None:
                if result is None:
                    # User cancelled
                    return
                if result == "skip_future":
                    # Update config to skip in future
                    self._kagan_app.config.ui.skip_tmux_gateway = True
                    if self._skip_tmux_gateway_callback:
                        self._skip_tmux_gateway_callback()
                # Proceed to open tmux session
                self._app.call_later(self._do_open_pair_session, ticket)

            self._push_screen(TmuxGatewayModal(ticket.id, ticket.title), on_gateway_result)
            return

        # Skip modal - open directly
        await self._do_open_pair_session(ticket)

    async def _do_open_pair_session(self, ticket: Ticket) -> None:
        """Actually open the tmux session after modal confirmation.

        Args:
            ticket: The PAIR ticket.
        """
        worktree = self._kagan_app.worktree_manager

        try:
            wt_path = await worktree.get_path(ticket.id)
            if wt_path is None:
                base = self._kagan_app.config.general.default_base_branch
                wt_path = await worktree.create(ticket.id, ticket.title, base)

            session_manager = self._kagan_app.session_manager
            if not await session_manager.session_exists(ticket.id):
                await session_manager.create_session(ticket, wt_path)

            if ticket.status == TicketStatus.BACKLOG:
                await self._kagan_app.state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)

            with self._app.suspend():
                attach_success = session_manager.attach_session(ticket.id)

            # Detect exit vs detach: small buffer to let tmux server state settle
            await asyncio.sleep(0.1)

            session_still_exists = await session_manager.session_exists(ticket.id)

            # Refresh ticket status from DB
            current_ticket = await self._kagan_app.state_manager.get_ticket(ticket.id)

            if attach_success and not session_still_exists and current_ticket:
                # Session terminated (user typed 'exit') - prompt for review
                if current_ticket.status == TicketStatus.IN_PROGRESS:
                    from kagan.ui.modals import ConfirmModal

                    # Store task references to prevent garbage collection
                    _background_tasks: set[asyncio.Task] = set()

                    def on_confirm(confirmed: bool | None) -> None:
                        async def move_to_review() -> None:
                            await self._kagan_app.state_manager.move_ticket(
                                ticket.id, TicketStatus.REVIEW
                            )
                            await self._refresh_board()
                            self._notify("Moved to REVIEW", "information")

                        if confirmed:
                            task = asyncio.create_task(move_to_review())
                        else:
                            task = asyncio.create_task(self._refresh_board())
                        _background_tasks.add(task)
                        task.add_done_callback(_background_tasks.discard)

                    self._push_screen(
                        ConfirmModal(
                            title="Session Ended",
                            message=f"Move '{ticket.title[:40]}' to REVIEW?",
                        ),
                        on_confirm,
                    )

            if not attach_success:
                # Session died unexpectedly (e.g., agent exited, tmux killed)
                # Clean up stale state and try to recreate
                await session_manager.kill_session(ticket.id)
                await session_manager.create_session(ticket, wt_path)

                with self._app.suspend():
                    retry_success = session_manager.attach_session(ticket.id)

                if not retry_success:
                    self._notify("Session failed to start. Try again.", "error")

            await self._refresh_board()
        except (TmuxError, WorktreeError) as exc:
            self._notify(f"Failed to open session: {exc}", "error")
