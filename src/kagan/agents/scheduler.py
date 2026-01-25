"""Scheduler for automatic ticket-to-agent assignment."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from kagan.agents.process import AgentProcess, AgentState
from kagan.agents.prompt import build_prompt
from kagan.agents.signals import Signal, parse_signal
from kagan.database.models import TicketStatus, TicketUpdate

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.agents.manager import AgentManager
    from kagan.agents.worktree import WorktreeManager
    from kagan.config import HatConfig, KaganConfig
    from kagan.database.manager import StateManager
    from kagan.database.models import Ticket


class Scheduler:
    """Coordinates automatic ticket-to-agent assignment with iterative loops."""

    def __init__(
        self,
        state_manager: StateManager,
        agent_manager: AgentManager,
        worktree_manager: WorktreeManager,
        config: KaganConfig,
    ) -> None:
        self._state = state_manager
        self._agents = agent_manager
        self._worktrees = worktree_manager
        self._config = config
        self._running_tickets: set[str] = set()
        self._iteration_counts: dict[str, int] = {}
        self._tasks: set[asyncio.Task[None]] = set()

    async def tick(self) -> None:
        """Run one scheduling cycle."""
        tickets = await self._state.get_all_tickets()
        await self._handle_completions(tickets)
        if self._config.general.auto_start:
            await self._spawn_pending(tickets)

    async def _handle_completions(self, tickets: list) -> None:
        """Move tickets based on agent exit status (legacy support)."""
        status_map = {
            AgentState.FINISHED: TicketStatus.REVIEW,
            AgentState.FAILED: TicketStatus.BACKLOG,
        }
        for ticket in tickets:
            agent = self._agents.get(ticket.id)
            if agent and agent.state in status_map:
                await self._state.update_ticket(
                    ticket.id, TicketUpdate(status=status_map[agent.state])
                )
                await self._agents.terminate(ticket.id)

    async def _spawn_pending(self, tickets: list) -> None:
        """Spawn iterative loops for eligible IN_PROGRESS tickets."""
        max_agents = self._config.general.max_concurrent_agents
        active = len(self._running_tickets)

        for ticket in tickets:
            if active >= max_agents:
                break
            if ticket.status != TicketStatus.IN_PROGRESS:
                continue
            if ticket.id in self._running_tickets:
                continue

            self._running_tickets.add(ticket.id)
            task = asyncio.create_task(self._run_ticket_loop(ticket))
            self._tasks.add(task)
            task.add_done_callback(self._tasks.discard)
            active += 1

    async def _run_ticket_loop(self, ticket: Ticket) -> None:
        """Run the iterative loop for a ticket until completion."""
        try:
            wt_path = await self._worktrees.get_path(ticket.id)
            if wt_path is None:
                wt_path = await self._worktrees.create(
                    ticket.id, ticket.title, self._config.general.default_base_branch
                )

            hat = self._config.get_hat(ticket.assigned_hat) if ticket.assigned_hat else None
            max_iterations = self._config.general.max_iterations

            for iteration in range(1, max_iterations + 1):
                self._iteration_counts[ticket.id] = iteration

                signal = await self._run_iteration(ticket, wt_path, hat, iteration, max_iterations)

                if signal == Signal.COMPLETE:
                    await self._handle_complete(ticket)
                    return
                elif signal == Signal.BLOCKED:
                    await self._handle_blocked(ticket)
                    return

                await asyncio.sleep(self._config.general.iteration_delay_seconds)

            await self._handle_max_iterations(ticket)

        except Exception:
            await self._state.update_ticket(ticket.id, TicketUpdate(status=TicketStatus.BACKLOG))
        finally:
            self._running_tickets.discard(ticket.id)
            self._iteration_counts.pop(ticket.id, None)

    async def _run_iteration(
        self,
        ticket: Ticket,
        wt_path: Path,
        hat: HatConfig | None,
        iteration: int,
        max_iterations: int,
    ) -> Signal:
        """Run a single iteration, return the signal."""
        scratchpad = await self._state.get_scratchpad(ticket.id)
        prompt = build_prompt(ticket, iteration, max_iterations, scratchpad, hat)

        cmd = self._get_command(ticket.assigned_hat)
        process = AgentProcess(f"{ticket.id}-iter-{iteration}")
        await process.start(cmd, wt_path)
        await process.send_input(prompt + "\n")
        await process.wait_for_exit()

        output, _ = process.get_output()
        result = parse_signal(output)

        summary = f"\n\n---\n## Iteration {iteration}\n{output[-5000:]}"
        await self._state.update_scratchpad(ticket.id, scratchpad + summary)

        return result.signal

    async def _handle_complete(self, ticket: Ticket) -> None:
        """Handle successful completion."""
        await self._state.update_ticket(ticket.id, TicketUpdate(status=TicketStatus.REVIEW))
        scratchpad = await self._state.get_scratchpad(ticket.id)
        if scratchpad:
            summary = scratchpad[:500]
            await self._state.add_knowledge(ticket.id, summary, tags=["completed"])
        await self._state.delete_scratchpad(ticket.id)

    async def _handle_blocked(self, ticket: Ticket) -> None:
        """Handle blocked signal - return to backlog."""
        await self._state.update_ticket(ticket.id, TicketUpdate(status=TicketStatus.BACKLOG))

    async def _handle_max_iterations(self, ticket: Ticket) -> None:
        """Handle max iterations reached."""
        await self._state.update_ticket(ticket.id, TicketUpdate(status=TicketStatus.BACKLOG))

    def _get_command(self, assigned_hat: str | None) -> str:
        """Get agent command from hat config or defaults."""
        if assigned_hat and (hat := self._config.get_hat(assigned_hat)):
            return self._build_command(hat.agent_command, hat.args)
        if default := self._config.get_default_hat():
            return self._build_command(default[1].agent_command, default[1].args)
        return "claude"

    def _build_command(self, cmd: str, args: list[str]) -> str:
        """Build command string."""
        return f"{cmd} {' '.join(args)}" if args else cmd

    def get_iteration(self, ticket_id: str) -> int | None:
        """Get current iteration for a ticket (for UI display)."""
        return self._iteration_counts.get(ticket_id)
