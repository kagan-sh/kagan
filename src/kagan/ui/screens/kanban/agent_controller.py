"""Agent control operations for Kanban screen."""

from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.database.models import TicketStatus, TicketType
from kagan.ui.utils import coerce_enum

if TYPE_CHECKING:
    from collections.abc import Callable

    from kagan.app import KaganApp
    from kagan.database.models import Ticket


class AgentController:
    """Controls agent operations for AUTO tickets.

    Provides methods to watch, start, and stop agents.
    """

    def __init__(
        self,
        kagan_app: KaganApp,
        notify: Callable[[str, str], None],
        push_screen: Callable,
        refresh_board: Callable,
    ) -> None:
        """Initialize agent controller.

        Args:
            kagan_app: The KaganApp instance for accessing scheduler.
            notify: Callback for showing notifications (message, severity).
            push_screen: Callback for pushing screens/modals.
            refresh_board: Async callback for refreshing the board.
        """
        self._kagan_app = kagan_app
        self._notify = notify
        self._push_screen = push_screen
        self._refresh_board = refresh_board

    async def watch_agent(self, ticket: Ticket) -> None:
        """Open agent output modal to watch running agent.

        Shows appropriate message if agent is not running.

        Args:
            ticket: The AUTO ticket to watch.
        """
        ticket_type = coerce_enum(ticket.ticket_type, TicketType)
        if ticket_type != TicketType.AUTO:
            self._notify("Watch is only for AUTO tickets", "warning")
            return

        scheduler = self._kagan_app.scheduler
        if not scheduler.is_running(ticket.id):
            # Provide more helpful message based on ticket status
            status = coerce_enum(ticket.status, TicketStatus)

            if status == TicketStatus.BACKLOG:
                self._notify(
                    "Agent not started. Press [a] to start or move to IN_PROGRESS.",
                    "warning",
                )
            elif status == TicketStatus.IN_PROGRESS:
                config = self._kagan_app.config
                if config.general.auto_start:
                    self._notify(
                        "Agent starting soon... (next scheduler tick)",
                        "warning",
                    )
                else:
                    self._notify(
                        "Agent not running. Press a to start manually.",
                        "warning",
                    )
            else:
                self._notify("No agent running for this ticket", "warning")
            return

        from kagan.ui.modals.agent_output import AgentOutputModal

        agent = scheduler.get_running_agent(ticket.id)
        iteration = scheduler.get_iteration_count(ticket.id)
        modal = AgentOutputModal(ticket=ticket, agent=agent, iteration=iteration)
        await self._push_screen(modal)

    async def start_agent(self, ticket: Ticket) -> bool:
        """Start an agent for AUTO ticket.

        Args:
            ticket: The AUTO ticket to start agent for.

        Returns:
            True if agent was started, False otherwise.
        """
        ticket_type = coerce_enum(ticket.ticket_type, TicketType)
        if ticket_type != TicketType.AUTO:
            self._notify("Start agent is only for AUTO tickets", "warning")
            return False

        scheduler = self._kagan_app.scheduler

        # Handle BACKLOG tickets - move to IN_PROGRESS first
        if ticket.status == TicketStatus.BACKLOG:
            await self._kagan_app.state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)
            # Refresh ticket to get updated status
            refreshed = await self._kagan_app.state_manager.get_ticket(ticket.id)
            if refreshed:
                ticket = refreshed
            await self._refresh_board()

        # Spawn agent
        spawned = await scheduler.spawn_for_ticket(ticket)
        if spawned:
            self._notify(f"Started agent for: {ticket.short_id}", "information")
            return True
        else:
            self._notify("Failed to start agent (at capacity?)", "warning")
            return False

    async def stop_agent(self, ticket: Ticket) -> bool:
        """Stop running agent and move ticket to BACKLOG.

        Args:
            ticket: The AUTO ticket to stop agent for.

        Returns:
            True if agent was stopped, False otherwise.
        """
        ticket_type = coerce_enum(ticket.ticket_type, TicketType)
        if ticket_type != TicketType.AUTO:
            self._notify("Stop agent is only for AUTO tickets", "warning")
            return False

        scheduler = self._kagan_app.scheduler
        if not scheduler.is_running(ticket.id):
            self._notify("No agent running for this ticket", "warning")
            return False

        # Stop the agent and its task
        await scheduler.stop_ticket(ticket.id)

        # Move ticket to BACKLOG to prevent auto-restart
        await self._kagan_app.state_manager.move_ticket(ticket.id, TicketStatus.BACKLOG)
        await self._refresh_board()
        self._notify(f"Stopped agent: {ticket.short_id}, moved to BACKLOG", "information")
        return True
