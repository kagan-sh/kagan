"""Reactive scheduler for automatic ticket-to-agent assignment (AUTO mode).

Uses event-driven architecture: reacts to ticket status changes instead of polling.
"""

from __future__ import annotations

import asyncio
import contextlib
import json
import weakref
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Literal

from kagan.acp.agent import Agent
from kagan.agents.config_resolver import resolve_agent_config, resolve_model
from kagan.agents.prompt import build_prompt
from kagan.agents.prompt_loader import get_review_prompt
from kagan.agents.signals import Signal, SignalResult, parse_signal
from kagan.constants import MODAL_TITLE_MAX_LENGTH
from kagan.database.models import TicketStatus, TicketType
from kagan.debug_log import log
from kagan.git_utils import get_git_user_identity
from kagan.limits import AGENT_TIMEOUT_LONG

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from kagan.agents.worktree import WorktreeManager
    from kagan.app import KaganApp
    from kagan.config import AgentConfig, KaganConfig
    from kagan.database.manager import StateManager
    from kagan.database.models import Ticket
    from kagan.sessions.manager import SessionManager


@dataclass(slots=True)
class RunningTicketState:
    """State for a currently running ticket."""

    task: asyncio.Task[None] | None = None
    agent: Agent | None = None
    iteration: int = 0
    review_agent: Agent | None = None  # Review agent for watching
    is_reviewing: bool = False  # Currently in review phase


class Scheduler:
    """Reactive scheduler for AUTO ticket processing.

    Instead of polling, reacts to ticket status changes via a queue.
    Single worker loop processes all spawn/stop requests sequentially,
    eliminating race conditions.
    """

    def __init__(
        self,
        state_manager: StateManager,
        worktree_manager: WorktreeManager,
        config: KaganConfig,
        session_manager: SessionManager | None = None,
        on_ticket_changed: Callable[[], None] | None = None,
        on_iteration_changed: Callable[[str, int], None] | None = None,
        on_error: Callable[[str, str], None] | None = None,
        app: KaganApp | None = None,
    ) -> None:
        self._state = state_manager
        self._worktrees = worktree_manager
        self._config = config
        self._sessions = session_manager
        self._running: dict[str, RunningTicketState] = {}
        self._on_ticket_changed = on_ticket_changed
        self._on_iteration_changed = on_iteration_changed
        self._on_error = on_error
        self._app = app

        # Event queue for reactive processing
        self._event_queue: asyncio.Queue[tuple[str, TicketStatus | None, TicketStatus | None]] = (
            asyncio.Queue()
        )
        self._worker_task: asyncio.Task[None] | None = None
        self._started = False

        # Lock to serialize merge operations (prevents race conditions when
        # multiple tickets complete around the same time)
        self._merge_lock = asyncio.Lock()

    def start(self) -> None:
        """Start the scheduler's event processing loop."""
        if self._started:
            return
        self._started = True
        self._worker_task = asyncio.create_task(self._worker_loop())
        log.info("Scheduler started (reactive mode)")

    async def initialize_existing_tickets(self) -> None:
        """Spawn agents for existing IN_PROGRESS AUTO tickets.

        Called on startup to handle tickets that were already in progress
        before the scheduler started listening for changes.
        Only runs if auto_start is enabled in config.
        """
        if not self._config.general.auto_start:
            log.info("auto_start disabled, skipping initialization of existing tickets")
            return

        tickets = await self._state.get_tickets_by_status(TicketStatus.IN_PROGRESS)
        for ticket in tickets:
            if ticket.ticket_type == TicketType.AUTO:
                log.info(f"Queueing existing IN_PROGRESS ticket: {ticket.id}")
                await self._event_queue.put((ticket.id, None, TicketStatus.IN_PROGRESS))

    async def handle_status_change(
        self, ticket_id: str, old_status: TicketStatus | None, new_status: TicketStatus | None
    ) -> None:
        """Handle a ticket status change event.

        Called by StateManager when ticket status changes.
        Queues the event for processing by the worker loop.
        """
        await self._event_queue.put((ticket_id, old_status, new_status))
        log.debug(f"Queued status change: {ticket_id} {old_status} -> {new_status}")

    async def _worker_loop(self) -> None:
        """Single worker that processes all events sequentially.

        This eliminates race conditions because all spawn/stop decisions
        happen in one place, one at a time.
        """
        log.info("Scheduler worker loop started")
        while True:
            try:
                ticket_id, old_status, new_status = await self._event_queue.get()
                await self._process_event(ticket_id, old_status, new_status)
            except asyncio.CancelledError:
                log.info("Scheduler worker loop cancelled")
                break
            except Exception as e:
                log.error(f"Error in scheduler worker: {e}")

    async def _process_event(
        self, ticket_id: str, old_status: TicketStatus | None, new_status: TicketStatus | None
    ) -> None:
        """Process a single status change event."""
        # Ticket deleted
        if new_status is None:
            await self._stop_if_running(ticket_id)
            return

        # Get full ticket to check type
        ticket = await self._state.get_ticket(ticket_id)
        if ticket is None:
            await self._stop_if_running(ticket_id)
            return

        # Only handle AUTO tickets
        if ticket.ticket_type != TicketType.AUTO:
            return

        # React to status
        if new_status == TicketStatus.IN_PROGRESS:
            await self._ensure_running(ticket)
        elif old_status == TicketStatus.IN_PROGRESS and new_status != TicketStatus.REVIEW:
            # Moved OUT of IN_PROGRESS to non-REVIEW status - stop if running
            # Don't stop for REVIEW transitions as that's part of normal completion flow
            await self._stop_if_running(ticket_id)

    async def _ensure_running(self, ticket: Ticket) -> None:
        """Ensure an agent is running for this ticket."""
        if ticket.id in self._running:
            log.debug(f"Ticket {ticket.id} already running")
            return

        max_agents = self._config.general.max_concurrent_agents
        if len(self._running) >= max_agents:
            log.info(f"At capacity ({max_agents}), queueing {ticket.id} for retry")
            # Re-queue for later attempt
            await asyncio.sleep(1)
            await self._event_queue.put((ticket.id, None, TicketStatus.IN_PROGRESS))
            return

        await self._spawn(ticket)

    async def _spawn(self, ticket: Ticket) -> None:
        """Spawn an agent for a ticket. Called only from worker loop."""
        title = ticket.title[:MODAL_TITLE_MAX_LENGTH]
        log.info(f"Spawning agent for AUTO ticket {ticket.id}: {title}")

        # Reset review state from previous attempts
        await self._state.update_ticket(
            ticket.id,
            checks_passed=None,
            review_summary=None,
            merge_failed=False,
            merge_error=None,
        )

        # Clear previous agent logs for fresh retry
        await self._state.clear_agent_logs(ticket.id)

        # Add to _running BEFORE creating task to avoid race condition
        # where task checks _running before we've added the entry
        state = RunningTicketState()
        self._running[ticket.id] = state

        task = asyncio.create_task(self._run_ticket_loop(ticket))
        state.task = task

        task.add_done_callback(self._make_done_callback(ticket.id))

    async def _stop_if_running(self, ticket_id: str) -> None:
        """Stop agent if running. Called only from worker loop."""
        state = self._running.get(ticket_id)
        if state is None:
            return

        log.info(f"Stopping agent for ticket {ticket_id}")

        if state.agent is not None:
            await state.agent.stop()

        if state.task is not None and not state.task.done():
            state.task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await state.task

        self._running.pop(ticket_id, None)

        if self._on_iteration_changed:
            self._on_iteration_changed(ticket_id, 0)

    def _handle_task_done(self, ticket_id: str, task: asyncio.Task[None]) -> None:
        """Handle agent task completion."""
        self._running.pop(ticket_id, None)
        if self._on_iteration_changed:
            self._on_iteration_changed(ticket_id, 0)

    def _make_done_callback(self, ticket_id: str) -> Callable[[asyncio.Task[None]], None]:
        """Create task done callback with weak self reference."""
        weak_self = weakref.ref(self)

        def on_done(task: asyncio.Task[None]) -> None:
            scheduler = weak_self()
            if scheduler is not None:
                scheduler._handle_task_done(ticket_id, task)

        return on_done

    # --- Public API (thread-safe via queue) ---

    @property
    def _running_tickets(self) -> set[str]:
        """Get set of currently running ticket IDs (for UI compatibility)."""
        return set(self._running.keys())

    def is_running(self, ticket_id: str) -> bool:
        """Check if a ticket is currently being processed."""
        return ticket_id in self._running

    def get_running_agent(self, ticket_id: str) -> Agent | None:
        """Get the running agent for a ticket (for watch functionality)."""
        state = self._running.get(ticket_id)
        return state.agent if state else None

    def get_iteration_count(self, ticket_id: str) -> int:
        """Get current iteration count for a ticket."""
        state = self._running.get(ticket_id)
        return state.iteration if state else 0

    def reset_iterations(self, ticket_id: str) -> None:
        """Reset the session iteration counter for a ticket.

        This resets the in-memory "leash" counter used for the current session,
        not the lifetime total_iterations stored in the database.
        Called when a ticket is rejected and retried.
        """
        state = self._running.get(ticket_id)
        if state is not None:
            log.info(f"Resetting session iteration counter for ticket {ticket_id}")
            state.iteration = 0
            if self._on_iteration_changed:
                self._on_iteration_changed(ticket_id, 0)
        else:
            log.debug(f"Cannot reset iterations for {ticket_id}: not running")

    def is_reviewing(self, ticket_id: str) -> bool:
        """Check if ticket is currently in review phase."""
        state = self._running.get(ticket_id)
        return state.is_reviewing if state else False

    def get_review_agent(self, ticket_id: str) -> Agent | None:
        """Get the running review agent for a ticket (for watch functionality)."""
        state = self._running.get(ticket_id)
        return state.review_agent if state else None

    async def stop_ticket(self, ticket_id: str) -> bool:
        """Request to stop a ticket. Returns True if was running."""
        if ticket_id not in self._running:
            return False
        # Queue a "moved out of IN_PROGRESS" event
        await self._event_queue.put((ticket_id, TicketStatus.IN_PROGRESS, TicketStatus.BACKLOG))
        return True

    async def spawn_for_ticket(self, ticket: Ticket) -> bool:
        """Manually request to spawn an agent for a ticket.

        Used by UI for manual agent starts. Returns True if spawn was queued.
        """
        if ticket.id in self._running:
            return False  # Already running
        if ticket.ticket_type != TicketType.AUTO:
            return False  # Only AUTO tickets

        # Queue a spawn event
        await self._event_queue.put((ticket.id, None, TicketStatus.IN_PROGRESS))
        return True

    async def stop(self) -> None:
        """Stop the scheduler and all running agents."""
        log.info("Stopping scheduler")

        # Stop worker loop
        if self._worker_task and not self._worker_task.done():
            self._worker_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._worker_task

        # Stop all agents
        for ticket_id, state in list(self._running.items()):
            log.info(f"Stopping agent for ticket {ticket_id}")
            if state.agent is not None:
                await state.agent.stop()
            if state.task is not None and not state.task.done():
                state.task.cancel()

        self._running.clear()
        self._started = False

    # --- Internal: ticket processing loop ---

    def _notify_ticket_changed(self) -> None:
        """Notify that a ticket has changed status."""
        if self._on_ticket_changed:
            self._on_ticket_changed()

    def _notify_error(self, ticket_id: str, message: str) -> None:
        """Notify that an error occurred for a ticket."""
        if self._on_error:
            self._on_error(ticket_id, message)

    async def _run_ticket_loop(self, ticket: Ticket) -> None:
        """Run the iterative loop for a ticket until completion."""
        log.info(f"Starting ticket loop for {ticket.id}")
        self._notify_error(ticket.id, "Agent starting...")

        try:
            # Ensure worktree exists
            wt_path = await self._worktrees.get_path(ticket.id)
            if wt_path is None:
                log.info(f"Creating worktree for {ticket.id}")
                wt_path = await self._worktrees.create(
                    ticket.id, ticket.title, self._config.general.default_base_branch
                )
            log.info(f"Worktree path: {wt_path}")

            # Get git user identity for Co-authored-by attribution in commits
            user_name, user_email = await get_git_user_identity()
            log.debug(f"Git user identity: {user_name} <{user_email}>")

            # Get agent config
            agent_config = self._get_agent_config(ticket)
            log.debug(f"Agent config: {agent_config.name}")
            max_iterations = self._config.general.max_iterations
            log.info(f"Starting iterations for {ticket.id}, max={max_iterations}")

            for iteration in range(1, max_iterations + 1):
                # Check if we're still supposed to be running
                if ticket.id not in self._running:
                    log.info(f"Ticket {ticket.id} stopped, exiting loop")
                    return

                state = self._running[ticket.id]
                state.iteration = iteration

                # Increment lifetime total_iterations in database (the "odometer")
                await self._state.increment_total_iterations(ticket.id)

                if self._on_iteration_changed:
                    self._on_iteration_changed(ticket.id, iteration)
                log.debug(f"Ticket {ticket.id} iteration {iteration}/{max_iterations}")

                signal = await self._run_iteration(
                    ticket,
                    wt_path,
                    agent_config,
                    iteration,
                    max_iterations,
                    user_name=user_name,
                    user_email=user_email,
                )
                log.debug(f"Ticket {ticket.id} iteration {iteration} signal: {signal}")

                if signal.signal == Signal.COMPLETE:
                    log.info(f"Ticket {ticket.id} completed, moving to REVIEW")
                    await self._handle_complete(ticket)
                    return
                elif signal.signal == Signal.BLOCKED:
                    log.warning(f"Ticket {ticket.id} blocked: {signal.reason}")
                    self._notify_error(ticket.id, f"Blocked: {signal.reason}")
                    await self._handle_blocked(ticket, signal.reason)
                    return

                await asyncio.sleep(self._config.general.iteration_delay_seconds)

            log.warning(f"Ticket {ticket.id} reached max iterations")
            self._notify_error(ticket.id, "Reached max iterations without completing")
            await self._handle_max_iterations(ticket)

        except asyncio.CancelledError:
            log.info(f"Ticket {ticket.id} cancelled")
            raise
        except Exception as e:
            import traceback

            tb = traceback.format_exc()
            log.error(f"Exception in ticket loop for {ticket.id}: {e}")
            log.error(f"Traceback:\n{tb}")
            self._notify_error(ticket.id, f"Agent failed: {e}")
            await self._update_ticket_status(ticket.id, TicketStatus.BACKLOG)
        finally:
            log.info(f"Ticket loop ended for {ticket.id}")

    def _get_agent_config(self, ticket: Ticket) -> AgentConfig:
        """Get agent config for a ticket using unified resolver."""
        return resolve_agent_config(ticket, self._config)

    def _notify_user(
        self, message: str, title: str, severity: Literal["information", "warning", "error"]
    ) -> None:
        """Send a notification to the user via the app if available.

        Args:
            message: The notification message.
            title: The notification title.
            severity: The severity level (information, warning, error).
        """
        if self._app is not None:
            self._app.notify(message, title=title, severity=severity)

    def _apply_model_override(self, agent: Agent, agent_config: AgentConfig, context: str) -> None:
        """Apply model override to agent if configured.

        Args:
            agent: The agent to configure.
            agent_config: The agent configuration.
            context: Context string for logging (e.g., "ticket ABC-123" or "review").
        """
        model = resolve_model(self._config, agent_config.identity)
        if model:
            agent.set_model_override(model)
            log.info(f"Applied model override for {context}: {model}")

    def _serialize_agent_output(self, agent: Agent) -> str:
        """Serialize agent output including tool calls, thinking, and response to JSON."""
        from kagan.acp import messages as msg_types

        serialized_messages: list[dict[str, Any]] = []
        for message in agent._buffers.messages:
            if isinstance(message, msg_types.AgentUpdate):
                serialized_messages.append({"type": "response", "content": message.text})
            elif isinstance(message, msg_types.Thinking):
                serialized_messages.append({"type": "thinking", "content": message.text})
            elif isinstance(message, msg_types.ToolCall):
                serialized_messages.append(
                    {
                        "type": "tool_call",
                        "id": str(message.tool_call.get("id", "")),
                        "title": str(message.tool_call.get("title", "")),
                        "kind": str(message.tool_call.get("kind", "")),
                    }
                )
            elif isinstance(message, msg_types.ToolCallUpdate):
                serialized_messages.append(
                    {
                        "type": "tool_call_update",
                        "id": str(message.update.get("id", "")),
                        "status": str(message.update.get("status", "")),
                    }
                )
            elif isinstance(message, msg_types.Plan):
                serialized_messages.append(
                    {
                        "type": "plan",
                        "entries": [dict(e) for e in message.entries] if message.entries else [],
                    }
                )
            elif isinstance(message, msg_types.AgentReady):
                serialized_messages.append({"type": "agent_ready"})
            elif isinstance(message, msg_types.AgentFail):
                serialized_messages.append(
                    {
                        "type": "agent_fail",
                        "message": message.message,
                        "details": message.details,
                    }
                )

        return json.dumps(
            {
                "messages": serialized_messages,
                "response_text": agent.get_response_text(),
            }
        )

    async def _run_iteration(
        self,
        ticket: Ticket,
        wt_path: Path,
        agent_config: AgentConfig,
        iteration: int,
        max_iterations: int,
        user_name: str = "Developer",
        user_email: str = "developer@localhost",
    ) -> SignalResult:
        """Run a single iteration for a ticket.

        Args:
            ticket: The ticket being worked on.
            wt_path: Path to the worktree.
            agent_config: Agent configuration.
            iteration: Current iteration number.
            max_iterations: Maximum allowed iterations.
            user_name: Git user name for Co-authored-by attribution.
            user_email: Git user email for Co-authored-by attribution.

        Returns:
            Signal result from the agent.
        """
        # Get or create agent
        state = self._running.get(ticket.id)
        agent = state.agent if state else None

        if agent is None:
            agent = Agent(wt_path, agent_config)
            agent.set_auto_approve(self._config.general.auto_approve)

            # Apply model override if configured
            self._apply_model_override(agent, agent_config, f"ticket {ticket.id}")

            agent.start()
            if state:
                state.agent = agent

            try:
                await agent.wait_ready(timeout=AGENT_TIMEOUT_LONG)
            except TimeoutError:
                log.error(f"Agent timeout for ticket {ticket.id}")
                return parse_signal('<blocked reason="Agent failed to start"/>')
        else:
            # Re-sync auto_approve from config in case it changed
            agent.set_auto_approve(self._config.general.auto_approve)

        # Build prompt with scratchpad context
        scratchpad = await self._state.get_scratchpad(ticket.id)
        prompt = build_prompt(
            ticket=ticket,
            iteration=iteration,
            max_iterations=max_iterations,
            scratchpad=scratchpad,
            user_name=user_name,
            user_email=user_email,
        )

        # Send prompt and get response
        log.info(f"Sending prompt to agent for ticket {ticket.id}, iteration {iteration}")
        try:
            await agent.send_prompt(prompt)
        except Exception as e:
            log.error(f"Agent prompt failed for {ticket.id}: {e}")
            return parse_signal(f'<blocked reason="Agent error: {e}"/>')
        finally:
            # Clear tool calls to prevent memory accumulation across iterations
            agent.clear_tool_calls()

        # Get response and parse signal
        response = agent.get_response_text()
        signal_result = parse_signal(response)

        # Persist FULL agent output (including tool calls, thinking, etc.) as JSON
        serialized_output = self._serialize_agent_output(agent)
        await self._state.append_agent_log(
            ticket.id, "implementation", iteration, serialized_output
        )

        # Update scratchpad with progress (truncated for prompt context)
        progress_note = f"\n\n--- Iteration {iteration} ---\n{response[-2000:]}"
        await self._state.update_scratchpad(ticket.id, scratchpad + progress_note)

        return signal_result

    async def _handle_complete(self, ticket: Ticket) -> None:
        """Handle ticket completion - move to REVIEW immediately, then run review."""
        # 1. Move to REVIEW status IMMEDIATELY (before review agent runs)
        await self._state.update_ticket(ticket.id, status=TicketStatus.REVIEW)
        self._notify_ticket_changed()

        wt_path = await self._worktrees.get_path(ticket.id)
        checks_passed = False
        review_summary = ""

        if wt_path is not None:
            # Mark as reviewing and run review agent
            state = self._running.get(ticket.id)
            if state:
                state.is_reviewing = True

            try:
                checks_passed, review_summary = await self._run_review(ticket, wt_path)
            finally:
                if state:
                    state.is_reviewing = False
                    state.review_agent = None

            status = "approved" if checks_passed else "rejected"
            log.info(f"Ticket {ticket.id} review: {status}")

            # Emit toast notification for review result
            if checks_passed:
                self._notify_user(
                    f"✓ Review passed: {ticket.title[:30]}",
                    title="Review Complete",
                    severity="information",
                )
            else:
                self._notify_user(
                    f"✗ Review failed: {review_summary[:50]}",
                    title="Review Complete",
                    severity="warning",
                )

        # 2. Update ticket with review results (status already REVIEW)
        await self._state.update_ticket(
            ticket.id,
            checks_passed=checks_passed,
            review_summary=review_summary,
        )
        self._notify_ticket_changed()

        # Auto-merge if enabled and review passed
        if self._config.general.auto_merge and checks_passed:
            log.info(f"Auto-merging ticket {ticket.id}")
            await self._auto_merge(ticket)

    async def _run_review(self, ticket: Ticket, wt_path: Path) -> tuple[bool, str]:
        """Run agent-based review and return (passed, summary)."""
        state = self._running.get(ticket.id)
        agent_config = self._get_agent_config(ticket)
        prompt = await self._build_review_prompt(ticket)

        agent = Agent(wt_path, agent_config, read_only=True)
        agent.set_auto_approve(True)

        # Track the review agent for watch functionality
        if state:
            state.review_agent = agent

        # Apply model override for review (same as work iterations)
        self._apply_model_override(agent, agent_config, f"review of ticket {ticket.id}")

        agent.start()

        try:
            await agent.wait_ready(timeout=AGENT_TIMEOUT_LONG)
            await agent.send_prompt(prompt)
            response = agent.get_response_text()

            # Persist review logs (including tool calls, thinking, etc.) as JSON
            serialized_output = self._serialize_agent_output(agent)
            await self._state.append_agent_log(ticket.id, "review", 1, serialized_output)

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
            if state:
                state.review_agent = None

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
        """Auto-merge ticket to main and move to DONE.

        Only called when auto_merge config is enabled.
        If merge fails due to conflict and auto_retry_on_merge_conflict is also enabled,
        attempts to rebase the branch and retry the ticket from IN_PROGRESS.

        Uses a lock to serialize merge operations, preventing race conditions when
        multiple tickets complete around the same time.
        """
        async with self._merge_lock:
            log.info(f"Acquired merge lock for ticket {ticket.id}")
            base = self._config.general.default_base_branch
            success, message = await self._worktrees.merge_to_main(ticket.id, base_branch=base)

            if success:
                await self._worktrees.delete(ticket.id, delete_branch=True)
                if self._sessions is not None:
                    await self._sessions.kill_session(ticket.id)
                await self._update_ticket_status(ticket.id, TicketStatus.DONE)
                log.info(f"Auto-merged ticket {ticket.id}: {ticket.title}")
            else:
                # Check if this is a merge conflict and auto-retry is enabled
                is_conflict = "conflict" in message.lower()
                should_retry = is_conflict and self._config.general.auto_retry_on_merge_conflict

                if should_retry:
                    log.info(f"Merge conflict for {ticket.id}, attempting rebase and retry")
                    await self._handle_merge_conflict_retry(ticket, base, message)
                else:
                    # Standard failure handling - stay in REVIEW with error
                    log.warning(f"Auto-merge failed for {ticket.id}: {message}")
                    await self._state.update_ticket(
                        ticket.id,
                        merge_failed=True,
                        merge_error=message[:500] if message else "Unknown error",
                    )
                    self._notify_user(
                        f"⚠ Merge failed: {message[:50]}",
                        title="Merge Error",
                        severity="error",
                    )

            self._notify_ticket_changed()
            log.info(f"Released merge lock for ticket {ticket.id}")

    async def _handle_merge_conflict_retry(
        self, ticket: Ticket, base_branch: str, original_error: str
    ) -> None:
        """Handle merge conflict by rebasing and sending ticket back to IN_PROGRESS.

        This gives the agent a chance to resolve conflicts after rebasing onto
        the latest base branch.
        """
        wt_path = await self._worktrees.get_path(ticket.id)
        if wt_path is None:
            log.error(f"Cannot retry {ticket.id}: worktree not found")
            await self._state.update_ticket(
                ticket.id,
                merge_failed=True,
                merge_error="Worktree not found for conflict retry",
            )
            return

        # Get info about what changed on base branch (for context)
        files_on_base = await self._worktrees.get_files_changed_on_base(ticket.id, base_branch)

        # Attempt to rebase onto latest base branch
        rebase_success, rebase_msg, conflict_files = await self._worktrees.rebase_onto_base(
            ticket.id, base_branch
        )

        if not rebase_success and conflict_files:
            # Rebase had conflicts - this is expected, the agent needs to fix them
            # The rebase was aborted, so we'll let the agent handle it manually
            log.info(f"Rebase conflict for {ticket.id}, agent will resolve: {conflict_files}")

        # Build detailed context for the scratchpad
        scratchpad = await self._state.get_scratchpad(ticket.id)
        conflict_note = self._build_merge_conflict_note(
            original_error=original_error,
            rebase_success=rebase_success,
            rebase_msg=rebase_msg,
            conflict_files=conflict_files,
            files_on_base=files_on_base,
            base_branch=base_branch,
        )
        await self._state.update_scratchpad(ticket.id, scratchpad + conflict_note)

        # Clear the review state since we're retrying
        await self._state.update_ticket(
            ticket.id,
            status=TicketStatus.IN_PROGRESS,
            checks_passed=None,
            review_summary=None,
            merge_failed=False,
            merge_error=None,
        )

        # Notify user about the retry
        self._notify_user(
            f"🔄 Merge conflict - retrying: {ticket.title[:30]}",
            title="Auto-Retry",
            severity="warning",
        )

        log.info(f"Ticket {ticket.id} sent back to IN_PROGRESS for merge conflict resolution")

        # Queue the ticket for processing (it's now IN_PROGRESS again)
        await self._event_queue.put((ticket.id, TicketStatus.REVIEW, TicketStatus.IN_PROGRESS))

    def _build_merge_conflict_note(
        self,
        original_error: str,
        rebase_success: bool,
        rebase_msg: str,
        conflict_files: list[str],
        files_on_base: list[str],
        base_branch: str,
    ) -> str:
        """Build a detailed scratchpad note about merge conflict for agent context."""
        lines = [
            "\n\n--- MERGE CONFLICT - AUTO RETRY ---",
            f"Original merge error: {original_error}",
            "",
        ]

        if rebase_success:
            lines.append(f"✓ Successfully rebased onto origin/{base_branch}")
            lines.append("The branch is now up to date. Please verify changes and signal COMPLETE.")
        else:
            lines.append(f"⚠ Rebase onto origin/{base_branch} had conflicts: {rebase_msg}")
            lines.append("")
            lines.append("ACTION REQUIRED: You need to manually resolve the conflicts.")
            lines.append("")
            lines.append("Steps to resolve:")
            lines.append(f"1. Run: git fetch origin {base_branch}")
            lines.append(f"2. Run: git rebase origin/{base_branch}")
            lines.append("3. For each conflict, edit the file to resolve, then: git add <file>")
            lines.append("4. Run: git rebase --continue")
            lines.append("5. Once resolved, signal COMPLETE to retry the merge")

        if conflict_files:
            lines.append("")
            lines.append("Files with conflicts:")
            for f in conflict_files[:10]:  # Limit to first 10
                lines.append(f"  - {f}")
            if len(conflict_files) > 10:
                lines.append(f"  ... and {len(conflict_files) - 10} more")

        if files_on_base:
            lines.append("")
            lines.append(f"Files recently changed on {base_branch} (potential conflict sources):")
            for f in files_on_base[:10]:  # Limit to first 10
                lines.append(f"  - {f}")
            if len(files_on_base) > 10:
                lines.append(f"  ... and {len(files_on_base) - 10} more")

        lines.append("")
        lines.append("---")
        lines.append("")

        return "\n".join(lines)

    async def _handle_blocked(self, ticket: Ticket, reason: str) -> None:
        """Handle blocked ticket - move back to BACKLOG with reason."""
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
        await self._state.update_ticket(ticket_id, status=status)
