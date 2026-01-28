"""Scheduler for automatic ticket-to-agent assignment (AUTO mode)."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from textual import log

from kagan.acp.agent import Agent
from kagan.agents.prompt import build_prompt
from kagan.agents.prompt_loader import PromptLoader
from kagan.agents.signals import Signal, SignalResult, parse_signal
from kagan.database.models import TicketStatus, TicketType, TicketUpdate

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from kagan.agents.worktree import WorktreeManager
    from kagan.config import AgentConfig, KaganConfig
    from kagan.database.manager import StateManager
    from kagan.database.models import Ticket


class Scheduler:
    """Coordinates automatic ticket-to-agent assignment with iterative loops.

    Only processes AUTO tickets. PAIR tickets use tmux sessions instead.
    """

    def __init__(
        self,
        state_manager: StateManager,
        worktree_manager: WorktreeManager,
        config: KaganConfig,
        on_ticket_changed: Callable[[], None] | None = None,
    ) -> None:
        self._state = state_manager
        self._worktrees = worktree_manager
        self._config = config
        self._running_tickets: set[str] = set()
        self._agents: dict[str, Agent] = {}
        self._iteration_counts: dict[str, int] = {}
        self._tasks: set[asyncio.Task[None]] = set()
        self._on_ticket_changed = on_ticket_changed
        self._prompt_loader = PromptLoader(config)

    def _notify_ticket_changed(self) -> None:
        """Notify that a ticket has changed status."""
        if self._on_ticket_changed:
            self._on_ticket_changed()

    def get_running_agent(self, ticket_id: str) -> Agent | None:
        """Get the running agent for a ticket (for watch functionality)."""
        return self._agents.get(ticket_id)

    def get_iteration_count(self, ticket_id: str) -> int:
        """Get current iteration count for a ticket."""
        return self._iteration_counts.get(ticket_id, 0)

    def is_running(self, ticket_id: str) -> bool:
        """Check if a ticket is currently being processed."""
        return ticket_id in self._running_tickets

    async def tick(self) -> None:
        """Run one scheduling cycle."""
        if not self._config.general.auto_start:
            return

        tickets = await self._state.get_all_tickets()
        auto_in_progress = sum(
            1
            for t in tickets
            if t.status == TicketStatus.IN_PROGRESS and t.ticket_type == TicketType.AUTO
        )
        running = len(self._running_tickets)
        log.debug(f"scheduler tick: auto_wip={auto_in_progress}, running={running}")

        await self._spawn_pending(tickets)

    async def _spawn_pending(self, tickets: list[Ticket]) -> None:
        """Spawn iterative loops for eligible AUTO IN_PROGRESS tickets."""
        max_agents = self._config.general.max_concurrent_agents
        active = len(self._running_tickets)
        log.debug(f"_spawn_pending: active={active}, max={max_agents}")

        for ticket in tickets:
            if active >= max_agents:
                log.debug(f"Max agents reached ({max_agents}), not spawning more")
                break

            # Only process AUTO tickets in IN_PROGRESS status
            if ticket.status != TicketStatus.IN_PROGRESS:
                continue
            if ticket.ticket_type != TicketType.AUTO:
                continue
            if ticket.id in self._running_tickets:
                continue

            log.info(f"Spawning agent for AUTO ticket {ticket.id}: {ticket.title[:50]}")
            self._running_tickets.add(ticket.id)
            task = asyncio.create_task(self._run_ticket_loop(ticket))
            self._tasks.add(task)
            task.add_done_callback(self._tasks.discard)
            active += 1

    async def _run_ticket_loop(self, ticket: Ticket) -> None:
        """Run the iterative loop for a ticket until completion."""
        log.info(f"Starting ticket loop for {ticket.id}")
        try:
            # Ensure worktree exists
            wt_path = await self._worktrees.get_path(ticket.id)
            if wt_path is None:
                log.info(f"Creating worktree for {ticket.id}")
                wt_path = await self._worktrees.create(
                    ticket.id, ticket.title, self._config.general.default_base_branch
                )
            log.info(f"Worktree path: {wt_path}")

            # Get agent config
            agent_config = self._get_agent_config(ticket)
            if agent_config is None:
                log.error(f"No agent config found for ticket {ticket.id}")
                await self._handle_blocked(ticket, "No agent configuration found")
                return

            log.debug(f"Agent config: {agent_config.name}")
            max_iterations = self._config.general.max_iterations
            log.info(f"Starting iterations for {ticket.id}, max={max_iterations}")

            for iteration in range(1, max_iterations + 1):
                self._iteration_counts[ticket.id] = iteration
                log.debug(f"Ticket {ticket.id} iteration {iteration}/{max_iterations}")

                signal = await self._run_iteration(
                    ticket, wt_path, agent_config, iteration, max_iterations
                )
                log.debug(f"Ticket {ticket.id} iteration {iteration} signal: {signal}")

                if signal.signal == Signal.COMPLETE:
                    log.info(f"Ticket {ticket.id} completed, moving to REVIEW")
                    await self._handle_complete(ticket)
                    return
                elif signal.signal == Signal.BLOCKED:
                    log.warning(f"Ticket {ticket.id} blocked: {signal.reason}")
                    await self._handle_blocked(ticket, signal.reason)
                    return

                await asyncio.sleep(self._config.general.iteration_delay_seconds)

            log.warning(f"Ticket {ticket.id} reached max iterations")
            await self._handle_max_iterations(ticket)

        except Exception as e:
            import traceback

            tb = traceback.format_exc()
            log.error(f"Exception in ticket loop for {ticket.id}: {e}")
            log.error(f"Traceback:\n{tb}")
            await self._update_ticket_status(ticket.id, TicketStatus.BACKLOG)
        finally:
            self._running_tickets.discard(ticket.id)
            self._agents.pop(ticket.id, None)
            self._iteration_counts.pop(ticket.id, None)
            log.info(f"Ticket loop ended for {ticket.id}")

    def _get_agent_config(self, ticket: Ticket) -> AgentConfig | None:
        """Get agent config for a ticket with fallback to default."""
        # Priority 1: ticket's agent_backend field
        if ticket.agent_backend:
            agent_config = self._config.get_agent(ticket.agent_backend)
            if agent_config:
                log.debug(f"Using agent_backend config: {ticket.agent_backend}")
                return agent_config

        # Priority 2: assigned_hat (backward compat)
        if ticket.assigned_hat:
            agent_config = self._config.get_agent(ticket.assigned_hat)
            if agent_config:
                log.debug(f"Using assigned_hat agent config: {ticket.assigned_hat}")
                return agent_config

        # Priority 3: default worker agent
        return self._config.get_worker_agent()

    async def _run_iteration(
        self,
        ticket: Ticket,
        wt_path: Path,
        agent_config: AgentConfig,
        iteration: int,
        max_iterations: int,
    ) -> SignalResult:
        """Run a single iteration for a ticket."""
        # Get or create agent
        agent = self._agents.get(ticket.id)
        if agent is None:
            agent = Agent(wt_path, agent_config)
            agent.set_auto_approve(True)  # AUTO mode auto-approves permissions
            agent.start()
            self._agents[ticket.id] = agent

            try:
                await agent.wait_ready(timeout=60.0)
            except TimeoutError:
                log.error(f"Agent timeout for ticket {ticket.id}")
                return parse_signal('<blocked reason="Agent failed to start"/>')

        # Build prompt with scratchpad context
        scratchpad = await self._state.get_scratchpad(ticket.id)
        prompt = build_prompt(
            ticket=ticket,
            iteration=iteration,
            max_iterations=max_iterations,
            scratchpad=scratchpad,
            prompt_loader=self._prompt_loader,
        )

        # Send prompt and get response
        log.info(f"Sending prompt to agent for ticket {ticket.id}, iteration {iteration}")
        try:
            await agent.send_prompt(prompt)
        except Exception as e:
            log.error(f"Agent prompt failed for {ticket.id}: {e}")
            return parse_signal(f'<blocked reason="Agent error: {e}"/>')

        # Get response and parse signal
        response = agent.get_response_text()
        signal_result = parse_signal(response)

        # Update scratchpad with progress
        progress_note = f"\n\n--- Iteration {iteration} ---\n{response[-2000:]}"
        await self._state.update_scratchpad(ticket.id, scratchpad + progress_note)

        return signal_result

    async def _handle_complete(self, ticket: Ticket) -> None:
        """Handle ticket completion - move to REVIEW."""
        await self._update_ticket_status(ticket.id, TicketStatus.REVIEW)
        self._notify_ticket_changed()

    async def _handle_blocked(self, ticket: Ticket, reason: str) -> None:
        """Handle blocked ticket - move back to BACKLOG with reason."""
        # Append block reason to scratchpad
        scratchpad = await self._state.get_scratchpad(ticket.id)
        block_note = f"\n\n--- BLOCKED ---\nReason: {reason}\n"
        await self._state.update_scratchpad(ticket.id, scratchpad + block_note)

        await self._update_ticket_status(ticket.id, TicketStatus.BACKLOG)
        self._notify_ticket_changed()

    async def _handle_max_iterations(self, ticket: Ticket) -> None:
        """Handle ticket that reached max iterations."""
        scratchpad = await self._state.get_scratchpad(ticket.id)
        max_iter_note = (
            f"\n\n--- MAX ITERATIONS ---\n"
            f"Reached {self._config.general.max_iterations} iterations without completion.\n"
        )
        await self._state.update_scratchpad(ticket.id, scratchpad + max_iter_note)

        await self._update_ticket_status(ticket.id, TicketStatus.BACKLOG)
        self._notify_ticket_changed()

    async def _update_ticket_status(self, ticket_id: str, status: TicketStatus) -> None:
        """Update ticket status."""
        await self._state.update_ticket(ticket_id, TicketUpdate(status=status))

    async def stop(self) -> None:
        """Stop all running agents."""
        for ticket_id, agent in list(self._agents.items()):
            log.info(f"Stopping agent for ticket {ticket_id}")
            await agent.stop()
        self._agents.clear()
        self._running_tickets.clear()
        self._iteration_counts.clear()

        # Cancel any running tasks
        for task in self._tasks:
            task.cancel()
        self._tasks.clear()
