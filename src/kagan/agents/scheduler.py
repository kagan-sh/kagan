"""Scheduler for automatic ticket-to-agent assignment (AUTO mode)."""

from __future__ import annotations

import asyncio
import contextlib
from typing import TYPE_CHECKING

from textual import log

from kagan.acp.agent import Agent
from kagan.agents.config_resolver import resolve_agent_config
from kagan.agents.prompt import build_prompt
from kagan.agents.prompt_loader import get_review_prompt
from kagan.agents.signals import Signal, SignalResult, parse_signal
from kagan.constants import MODAL_TITLE_MAX_LENGTH
from kagan.database.models import TicketStatus, TicketType, TicketUpdate
from kagan.limits import AGENT_TIMEOUT_LONG

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from kagan.agents.worktree import WorktreeManager
    from kagan.config import AgentConfig, KaganConfig
    from kagan.database.manager import StateManager
    from kagan.database.models import Ticket
    from kagan.sessions.manager import SessionManager


class Scheduler:
    """Coordinates automatic ticket-to-agent assignment with iterative loops.

    Only processes AUTO tickets. PAIR tickets use tmux sessions instead.
    """

    def __init__(
        self,
        state_manager: StateManager,
        worktree_manager: WorktreeManager,
        config: KaganConfig,
        session_manager: SessionManager | None = None,
        on_ticket_changed: Callable[[], None] | None = None,
        on_iteration_changed: Callable[[str, int], None] | None = None,
    ) -> None:
        self._state = state_manager
        self._worktrees = worktree_manager
        self._config = config
        self._sessions = session_manager
        self._running_tickets: set[str] = set()
        self._agents: dict[str, Agent] = {}
        self._iteration_counts: dict[str, int] = {}
        self._ticket_tasks: dict[str, asyncio.Task[None]] = {}
        self._on_ticket_changed = on_ticket_changed
        self._on_iteration_changed = on_iteration_changed

    def _notify_ticket_changed(self) -> None:
        """Notify that a ticket has changed status."""
        if self._on_ticket_changed:
            self._on_ticket_changed()

    def _make_task_done_callback(self, ticket_id: str) -> Callable[[asyncio.Task[None]], None]:
        """Create a done callback that removes the ticket task from tracking."""

        def callback(_task: asyncio.Task[None]) -> None:
            self._ticket_tasks.pop(ticket_id, None)

        return callback

    def get_running_agent(self, ticket_id: str) -> Agent | None:
        """Get the running agent for a ticket (for watch functionality)."""
        return self._agents.get(ticket_id)

    def get_iteration_count(self, ticket_id: str) -> int:
        """Get current iteration count for a ticket."""
        return self._iteration_counts.get(ticket_id, 0)

    def is_running(self, ticket_id: str) -> bool:
        """Check if a ticket is currently being processed."""
        return ticket_id in self._running_tickets

    async def stop_ticket(self, ticket_id: str) -> bool:
        """Stop a running ticket and its agent. Returns True if stopped."""
        if ticket_id not in self._running_tickets:
            return False

        # Stop the agent process
        agent = self._agents.get(ticket_id)
        if agent:
            await agent.stop()

        # Cancel the ticket loop task
        task = self._ticket_tasks.get(ticket_id)
        if task and not task.done():
            task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await task

        # Cleanup is handled in finally block of _run_ticket_loop,
        # but ensure immediate removal for responsiveness
        self._running_tickets.discard(ticket_id)
        self._agents.pop(ticket_id, None)
        self._ticket_tasks.pop(ticket_id, None)
        self._iteration_counts.pop(ticket_id, None)

        if self._on_iteration_changed:
            self._on_iteration_changed(ticket_id, 0)

        log.info(f"Stopped ticket {ticket_id}")
        return True

    async def spawn_for_ticket(self, ticket: Ticket) -> bool:
        """Manually spawn an agent for a ticket (bypasses auto_start check).

        Returns True if agent was spawned, False if already running or ineligible.
        """
        if ticket.id in self._running_tickets:
            return False  # Already running

        if ticket.status != TicketStatus.IN_PROGRESS:
            return False

        if ticket.ticket_type != TicketType.AUTO:
            return False

        max_agents = self._config.general.max_concurrent_agents
        if len(self._running_tickets) >= max_agents:
            return False  # At capacity

        title = ticket.title[:MODAL_TITLE_MAX_LENGTH]
        log.info(f"Manually spawning agent for AUTO ticket {ticket.id}: {title}")
        self._running_tickets.add(ticket.id)
        task = asyncio.create_task(self._run_ticket_loop(ticket))
        self._ticket_tasks[ticket.id] = task
        task.add_done_callback(self._make_task_done_callback(ticket.id))
        return True

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

            title = ticket.title[:MODAL_TITLE_MAX_LENGTH]
            log.info(f"Spawning agent for AUTO ticket {ticket.id}: {title}")
            self._running_tickets.add(ticket.id)
            task = asyncio.create_task(self._run_ticket_loop(ticket))
            self._ticket_tasks[ticket.id] = task
            task.add_done_callback(self._make_task_done_callback(ticket.id))
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
            log.debug(f"Agent config: {agent_config.name}")
            max_iterations = self._config.general.max_iterations
            log.info(f"Starting iterations for {ticket.id}, max={max_iterations}")

            for iteration in range(1, max_iterations + 1):
                self._iteration_counts[ticket.id] = iteration
                if self._on_iteration_changed:
                    self._on_iteration_changed(ticket.id, iteration)
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
            if self._on_iteration_changed:
                self._on_iteration_changed(ticket.id, 0)
            log.info(f"Ticket loop ended for {ticket.id}")

    def _get_agent_config(self, ticket: Ticket) -> AgentConfig:
        """Get agent config for a ticket using unified resolver."""
        return resolve_agent_config(ticket, self._config)

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
            agent.set_auto_approve(self._config.general.auto_approve)
            agent.start()
            self._agents[ticket.id] = agent

            try:
                await agent.wait_ready(timeout=AGENT_TIMEOUT_LONG)
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
        """Handle ticket completion - run review, optionally auto-merge."""
        wt_path = await self._worktrees.get_path(ticket.id)
        checks_passed = False
        review_summary = ""

        if wt_path is not None:
            checks_passed, review_summary = await self._run_review(ticket, wt_path)
            status = "approved" if checks_passed else "rejected"
            log.info(f"Ticket {ticket.id} review: {status}")

        # Update ticket with review results and move to REVIEW
        await self._state.update_ticket(
            ticket.id,
            TicketUpdate(
                status=TicketStatus.REVIEW,
                checks_passed=checks_passed,
                review_summary=review_summary,
            ),
        )
        self._notify_ticket_changed()

        # Auto-merge if enabled and review passed
        if self._config.general.auto_merge and checks_passed:
            log.info(f"Auto-merging ticket {ticket.id}")
            await self._auto_merge(ticket)

    async def _run_review(self, ticket: Ticket, wt_path: Path) -> tuple[bool, str]:
        """Run agent-based review and return (passed, summary).

        Spawns a review agent that analyzes the changes and outputs
        an <approve> or <reject> signal.
        """
        # Get agent config
        agent_config = self._get_agent_config(ticket)

        # Build review prompt from template
        prompt = await self._build_review_prompt(ticket)

        # Spawn agent and send prompt (read_only since review only analyzes code)
        agent = Agent(wt_path, agent_config, read_only=True)
        agent.set_auto_approve(True)  # Auto-approve for review agent
        agent.start()

        try:
            await agent.wait_ready(timeout=AGENT_TIMEOUT_LONG)
            await agent.send_prompt(prompt)
            response = agent.get_response_text()

            # Parse for approve/reject signal
            signal = parse_signal(response)
            if signal.signal == Signal.APPROVE:
                return True, signal.reason
            elif signal.signal == Signal.REJECT:
                return False, signal.reason
            else:
                return False, "No review signal found in agent response"
        except TimeoutError:
            log.error(f"Review agent timeout for ticket {ticket.id}")
            return False, "Review agent timed out"
        except Exception as e:
            log.error(f"Review agent failed for {ticket.id}: {e}")
            return False, f"Review agent error: {e}"
        finally:
            await agent.stop()

    async def _build_review_prompt(self, ticket: Ticket) -> str:
        """Build review prompt from template with commits and diff."""
        base = self._config.general.default_base_branch
        commits = await self._worktrees.get_commit_log(ticket.id, base)
        diff_summary = await self._worktrees.get_diff_stats(ticket.id, base)

        return get_review_prompt(
            title=ticket.title,
            ticket_id=ticket.id,
            description=ticket.description or "",
            commits="\n".join(f"- {c}" for c in commits) if commits else "No commits",
            diff_summary=diff_summary or "No changes",
        )

    async def _auto_merge(self, ticket: Ticket) -> None:
        """Auto-merge ticket to main and move to DONE."""
        base = self._config.general.default_base_branch
        success, message = await self._worktrees.merge_to_main(ticket.id, base_branch=base)

        if success:
            # Cleanup worktree and branch
            await self._worktrees.delete(ticket.id, delete_branch=True)
            # Kill session if session manager is available
            if self._sessions is not None:
                await self._sessions.kill_session(ticket.id)
            # Move to DONE
            await self._update_ticket_status(ticket.id, TicketStatus.DONE)
            log.info(f"Auto-merged ticket {ticket.id}: {ticket.title}")
        else:
            log.warning(f"Auto-merge failed for {ticket.id}: {message}")
            # Ticket stays in REVIEW for manual intervention

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
        for task in self._ticket_tasks.values():
            task.cancel()
        self._ticket_tasks.clear()
