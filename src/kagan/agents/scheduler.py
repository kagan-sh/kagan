"""Scheduler for automatic ticket-to-agent assignment."""

from __future__ import annotations

import asyncio
from pathlib import Path  # noqa: TC003 - used for wt_path at runtime
from typing import TYPE_CHECKING

from textual import log

from kagan.agents.prompt import build_prompt
from kagan.agents.reviewer import build_review_prompt, parse_review_signal
from kagan.agents.signals import Signal, parse_signal
from kagan.database.models import TicketStatus, TicketUpdate

if TYPE_CHECKING:
    from collections.abc import Callable

    from kagan.agents.manager import AgentManager
    from kagan.agents.worktree import WorktreeManager
    from kagan.config import AgentConfig, KaganConfig
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
        on_ticket_changed: Callable[[], None] | None = None,
    ) -> None:
        self._state = state_manager
        self._agents = agent_manager
        self._worktrees = worktree_manager
        self._config = config
        self._running_tickets: set[str] = set()
        self._reviewing_tickets: set[str] = set()
        self._iteration_counts: dict[str, int] = {}
        self._tasks: set[asyncio.Task[None]] = set()
        self._on_ticket_changed = on_ticket_changed

    def _notify_ticket_changed(self) -> None:
        """Notify that a ticket has changed status."""
        if self._on_ticket_changed:
            self._on_ticket_changed()

    async def tick(self) -> None:
        """Run one scheduling cycle."""
        tickets = await self._state.get_all_tickets()
        in_progress = sum(1 for t in tickets if t.status == TicketStatus.IN_PROGRESS)
        review = sum(1 for t in tickets if t.status == TicketStatus.REVIEW)
        running = len(self._running_tickets)
        log.debug(
            f"tick: {len(tickets)} tickets, wip={in_progress}, review={review}, running={running}"
        )

        await self._handle_completions(tickets)
        if self._config.general.auto_start:
            await self._spawn_pending(tickets)
            await self._spawn_reviews(tickets)

    async def _handle_completions(self, tickets: list) -> None:
        """Handle any cleanup needed for completed tickets."""
        # With ACP agents, completion is handled in _run_ticket_loop
        # This method is kept for potential future use
        pass

    async def _spawn_pending(self, tickets: list) -> None:
        """Spawn iterative loops for eligible IN_PROGRESS tickets."""
        max_agents = self._config.general.max_concurrent_agents
        active = len(self._running_tickets)
        log.debug(f"_spawn_pending: active={active}, max={max_agents}")

        for ticket in tickets:
            if active >= max_agents:
                log.debug(f"Max agents reached ({max_agents}), not spawning more")
                break
            if ticket.status != TicketStatus.IN_PROGRESS:
                continue
            if ticket.id in self._running_tickets:
                continue

            log.info(f"Spawning agent for ticket {ticket.id}: {ticket.title[:50]}")
            self._running_tickets.add(ticket.id)
            task = asyncio.create_task(self._run_ticket_loop(ticket))
            self._tasks.add(task)
            task.add_done_callback(self._tasks.discard)
            active += 1

    async def _spawn_reviews(self, tickets: list) -> None:
        """Spawn review loops for eligible REVIEW tickets."""
        max_agents = self._config.general.max_concurrent_agents
        active = len(self._running_tickets) + len(self._reviewing_tickets)

        for ticket in tickets:
            if active >= max_agents:
                break
            if ticket.status != TicketStatus.REVIEW:
                continue
            if ticket.id in self._reviewing_tickets:
                continue

            self._reviewing_tickets.add(ticket.id)
            task = asyncio.create_task(self._run_review_loop(ticket))
            self._tasks.add(task)
            task.add_done_callback(self._tasks.discard)
            active += 1

    async def _run_ticket_loop(self, ticket: Ticket) -> None:
        """Run the iterative loop for a ticket until completion."""
        log.info(f"Starting ticket loop for {ticket.id}")
        try:
            wt_path = await self._worktrees.get_path(ticket.id)
            if wt_path is None:
                log.info(f"Creating worktree for {ticket.id}")
                wt_path = await self._worktrees.create(
                    ticket.id, ticket.title, self._config.general.default_base_branch
                )
            log.info(f"Worktree path: {wt_path}")

            # Get agent config - prefer assigned, fall back to default
            agent_config: AgentConfig | None = None
            if ticket.assigned_hat:
                # Try to find agent with matching name
                agent_config = self._config.get_agent(ticket.assigned_hat)
                log.debug(f"Using assigned agent config: {ticket.assigned_hat}")

            if agent_config is None:
                default = self._config.get_default_agent()
                if default:
                    agent_config = default[1]
                    log.debug(f"Using default agent config: {default[0]}")

            log.debug(f"Agent config: {agent_config}")
            max_iterations = self._config.general.max_iterations
            log.info(f"Starting iterations for {ticket.id}, max={max_iterations}")

            for iteration in range(1, max_iterations + 1):
                self._iteration_counts[ticket.id] = iteration
                log.debug(f"Ticket {ticket.id} iteration {iteration}/{max_iterations}")

                signal = await self._run_iteration(
                    ticket, wt_path, agent_config, iteration, max_iterations
                )
                log.debug(f"Ticket {ticket.id} iteration {iteration} signal: {signal}")

                if signal == Signal.COMPLETE:
                    log.info(f"Ticket {ticket.id} completed, moving to REVIEW")
                    await self._handle_complete(ticket)
                    return
                elif signal == Signal.BLOCKED:
                    log.warning(f"Ticket {ticket.id} blocked, returning to BACKLOG")
                    await self._handle_blocked(ticket)
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
            self._iteration_counts.pop(ticket.id, None)
            log.info(f"Ticket loop ended for {ticket.id}")

    async def _run_review_loop(self, ticket: Ticket) -> None:
        """Run the review loop for a ticket."""
        log.info(f"Starting review loop for {ticket.id}")
        try:
            wt_path = await self._worktrees.get_path(ticket.id)
            if wt_path is None:
                # No worktree means nothing to review
                log.warning(f"No worktree for {ticket.id}, returning to BACKLOG")
                await self._update_ticket_status(ticket.id, TicketStatus.BACKLOG)
                return

            # Get commit log and files changed
            base_branch = self._config.general.default_base_branch
            commits = await self._worktrees.get_commit_log(ticket.id, base_branch)
            log.debug(f"Found {len(commits) if commits else 0} commits for {ticket.id}")
            if not commits:
                # No commits to review
                log.warning(f"No commits for {ticket.id}, returning to BACKLOG")
                await self._update_ticket_status(ticket.id, TicketStatus.BACKLOG)
                return

            # Get list of files changed (simple diff --name-only)
            files_changed = await self._get_changed_files(ticket.id, base_branch)
            files_summary = (
                "\n".join(f"- {f}" for f in files_changed)
                if files_changed
                else "(No files changed)"
            )

            # Build review prompt
            prompt = build_review_prompt(ticket, commits, files_summary)

            # Get agent config for review
            agent_config = self._config.get_default_agent()
            if agent_config is None:
                await self._update_ticket_status(ticket.id, TicketStatus.BACKLOG)
                return
            agent_config = agent_config[1]

            # Spawn review agent via manager with review-specific ID
            review_agent_id = f"{ticket.id}-review"
            agent = await self._agents.spawn(review_agent_id, agent_config, wt_path)

            try:
                await agent.wait_ready(timeout=60.0)
                await agent.send_prompt(prompt)
                output = agent.get_response_text()
                review_signal = parse_review_signal(output)

                if review_signal.approved:
                    # Merge to main
                    success, merge_msg = await self._worktrees.merge_to_main(ticket.id, base_branch)
                    if success:
                        # Update ticket to DONE with summary
                        current = await self._state.get_ticket(ticket.id)
                        new_desc = current.description if current else ""
                        new_desc = new_desc or ""
                        if review_signal.summary:
                            new_desc += f"\n\n---\n**Review Summary:** {review_signal.summary}"
                        await self._state.update_ticket(
                            ticket.id,
                            TicketUpdate(status=TicketStatus.DONE, description=new_desc),
                        )
                        self._notify_ticket_changed()
                        # Add knowledge after successful merge
                        scratchpad = await self._state.get_scratchpad(ticket.id)
                        if scratchpad:
                            await self._state.add_knowledge(
                                ticket.id, scratchpad[:500], tags=["completed"]
                            )
                        await self._state.delete_scratchpad(ticket.id)
                        # Delete worktree with branch
                        await self._worktrees.delete(ticket.id, delete_branch=True)
                    else:
                        # Merge failed, back to IN_PROGRESS
                        scratchpad = await self._state.get_scratchpad(ticket.id)
                        scratchpad = scratchpad or ""
                        scratchpad += f"\n\n---\n**Merge Failed:** {merge_msg}"
                        await self._state.update_scratchpad(ticket.id, scratchpad)
                        await self._update_ticket_status(ticket.id, TicketStatus.IN_PROGRESS)
                else:
                    # Rejected - back to IN_PROGRESS
                    scratchpad = await self._state.get_scratchpad(ticket.id)
                    scratchpad = scratchpad or ""
                    scratchpad += f"\n\n---\n**Review Rejected:** {review_signal.reason}"
                    await self._state.update_scratchpad(ticket.id, scratchpad)
                    await self._update_ticket_status(ticket.id, TicketStatus.IN_PROGRESS)

            except TimeoutError:
                await self._update_ticket_status(ticket.id, TicketStatus.BACKLOG)
            except Exception:
                await self._update_ticket_status(ticket.id, TicketStatus.BACKLOG)
            finally:
                await self._agents.terminate(review_agent_id)

        except Exception:
            await self._update_ticket_status(ticket.id, TicketStatus.BACKLOG)
        finally:
            self._reviewing_tickets.discard(ticket.id)

    async def _get_changed_files(self, ticket_id: str, base_branch: str) -> list[str]:
        """Get list of files changed in the worktree branch."""
        wt_path = await self._worktrees.get_path(ticket_id)
        if wt_path is None:
            return []

        try:
            stdout, _ = await self._worktrees._run_git(
                "diff", "--name-only", f"{base_branch}...HEAD", cwd=wt_path, check=False
            )
            if not stdout:
                return []
            return [line.strip() for line in stdout.split("\n") if line.strip()]
        except Exception:
            return []

    async def _run_iteration(
        self,
        ticket: Ticket,
        wt_path: Path,
        agent_config: AgentConfig | None,
        iteration: int,
        max_iterations: int,
    ) -> Signal:
        """Run a single iteration, return the signal."""
        log.info(f"_run_iteration: ticket={ticket.id}, iteration={iteration}/{max_iterations}")
        scratchpad = await self._state.get_scratchpad(ticket.id)

        # Get hat config for prompt building (backward compat)
        hat = self._config.get_hat(ticket.assigned_hat) if ticket.assigned_hat else None
        prompt = build_prompt(ticket, iteration, max_iterations, scratchpad, hat)
        log.debug(f"Built prompt for {ticket.id}, length={len(prompt)}")

        # Use the default agent config if none provided
        if agent_config is None:
            default = self._config.get_default_agent()
            if default is None:
                log.error(f"No agent config available for {ticket.id}")
                return Signal.BLOCKED
            agent_config = default[1]

        log.debug(
            f"Agent config for {ticket.id}: {agent_config.name}, cmd={agent_config.run_command}"
        )

        # Spawn ACP agent via manager (so it's trackable by UI)
        log.info(f"Spawning ACP agent for {ticket.id} in {wt_path}")
        agent = await self._agents.spawn(ticket.id, agent_config, wt_path)
        log.debug(f"Agent spawned for {ticket.id}, waiting for ready...")

        try:
            # Wait for agent to be ready
            await agent.wait_ready(timeout=60.0)
            log.debug(f"Agent ready for {ticket.id}, sending prompt")

            # Send prompt and wait for completion
            await agent.send_prompt(prompt)

            # Get response text for signal parsing
            output = agent.get_response_text()
            log.debug(f"Agent response length for {ticket.id}: {len(output)}")
            result = parse_signal(output)
            log.debug(f"Parsed signal for {ticket.id}: {result.signal}")

            # Store iteration result in scratchpad
            summary = f"\n\n---\n## Iteration {iteration}\n{output[-5000:]}"
            await self._state.update_scratchpad(ticket.id, scratchpad + summary)

            return result.signal

        except TimeoutError:
            log.error(f"Timeout waiting for agent for {ticket.id}")
            return Signal.BLOCKED
        except Exception as e:
            import traceback

            tb = traceback.format_exc()
            log.error(f"Exception in iteration for {ticket.id}: {e}")
            log.error(f"Traceback:\n{tb}")
            return Signal.BLOCKED
        finally:
            log.debug(f"Terminating agent for {ticket.id}")
            await self._agents.terminate(ticket.id)
            log.debug(f"Agent terminated for {ticket.id}")

    async def _handle_complete(self, ticket: Ticket) -> None:
        """Handle successful completion - move to REVIEW status."""
        log.debug(f"_handle_complete: updating {ticket.id} to REVIEW")
        await self._update_ticket_status(ticket.id, TicketStatus.REVIEW)
        # Note: Knowledge addition happens after successful review/merge

    async def _handle_blocked(self, ticket: Ticket) -> None:
        """Handle blocked signal - return to backlog."""
        await self._update_ticket_status(ticket.id, TicketStatus.BACKLOG)

    async def _handle_max_iterations(self, ticket: Ticket) -> None:
        """Handle max iterations reached."""
        await self._update_ticket_status(ticket.id, TicketStatus.BACKLOG)

    async def _update_ticket_status(self, ticket_id: str, status: TicketStatus) -> None:
        """Update ticket status and notify UI."""
        await self._state.update_ticket(ticket_id, TicketUpdate(status=status))
        self._notify_ticket_changed()

    def get_iteration(self, ticket_id: str) -> int | None:
        """Get current iteration for a ticket (for UI display)."""
        return self._iteration_counts.get(ticket_id)
