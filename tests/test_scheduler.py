"""Tests for Scheduler class."""

from __future__ import annotations

import asyncio
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from kagan.agents.scheduler import Scheduler
from kagan.config import GeneralConfig, HatConfig, KaganConfig
from kagan.database.models import Ticket, TicketStatus


@pytest.fixture
def mock_state():
    m = MagicMock()
    m.get_all_tickets = AsyncMock(return_value=[])
    m.update_ticket = AsyncMock()
    m.get_scratchpad = AsyncMock(return_value="")
    m.update_scratchpad = AsyncMock()
    m.delete_scratchpad = AsyncMock()
    m.add_knowledge = AsyncMock()
    return m


@pytest.fixture
def mock_agents():
    m = MagicMock()
    m.get = MagicMock(return_value=None)
    m.list_active = MagicMock(return_value=[])
    m.spawn = AsyncMock()
    m.terminate = AsyncMock()
    return m


@pytest.fixture
def mock_worktrees():
    m = MagicMock()
    m.get_path = AsyncMock(return_value=None)
    m.create = AsyncMock(return_value=Path("/tmp/wt"))
    return m


@pytest.fixture
def config():
    return KaganConfig(
        general=GeneralConfig(auto_start=True, max_concurrent_agents=2),
        hats={"dev": HatConfig(agent_command="claude", args=["--model", "opus"])},
    )


@pytest.fixture
def review_config():
    """Config with agents for review loop tests."""
    from kagan.config import AgentConfig

    return KaganConfig(
        general=GeneralConfig(auto_start=True, max_concurrent_agents=2),
        hats={"dev": HatConfig(agent_command="claude", args=["--model", "opus"])},
        agents={
            "claude": AgentConfig(
                identity="claude.com",
                name="Claude",
                short_name="claude",
                run_command={"*": "claude"},
                active=True,
            )
        },
    )


class TestScheduler:
    async def test_respects_auto_start_false(self, mock_state, mock_agents, mock_worktrees):
        """No agents spawned when auto_start=False."""
        config = KaganConfig(general=GeneralConfig(auto_start=False))
        ticket = Ticket(id="t3", title="Test", status=TicketStatus.IN_PROGRESS)
        mock_state.get_all_tickets.return_value = [ticket]

        scheduler = Scheduler(mock_state, mock_agents, mock_worktrees, config)
        await scheduler.tick()

        # No tasks should be created with auto_start=False
        assert len(scheduler._running_tickets) == 0

    async def test_respects_max_concurrent(self, mock_state, mock_agents, mock_worktrees):
        """Respects max_concurrent_agents limit."""
        config = KaganConfig(general=GeneralConfig(auto_start=True, max_concurrent_agents=1))
        tickets = [
            Ticket(id="t1", title="Test1", status=TicketStatus.IN_PROGRESS),
            Ticket(id="t2", title="Test2", status=TicketStatus.IN_PROGRESS),
        ]
        mock_state.get_all_tickets.return_value = tickets

        scheduler = Scheduler(mock_state, mock_agents, mock_worktrees, config)

        # Patch _run_ticket_loop to avoid actual execution
        with patch.object(scheduler, "_run_ticket_loop", new_callable=AsyncMock):
            await scheduler.tick()
            # Only one ticket should be running due to max_concurrent=1
            assert len(scheduler._running_tickets) == 1

    async def test_starts_ticket_loop(self, mock_state, mock_agents, mock_worktrees, config):
        """Starts iterative loop for IN_PROGRESS ticket."""
        ticket = Ticket(id="t5", title="Feature", status=TicketStatus.IN_PROGRESS)
        mock_state.get_all_tickets.return_value = [ticket]

        scheduler = Scheduler(mock_state, mock_agents, mock_worktrees, config)

        with patch.object(scheduler, "_run_ticket_loop", new_callable=AsyncMock) as mock_loop:
            await scheduler.tick()
            # Give time for task to start
            await asyncio.sleep(0.01)
            mock_loop.assert_called_once_with(ticket)
            assert "t5" in scheduler._running_tickets

    async def test_get_iteration(self, mock_state, mock_agents, mock_worktrees, config):
        """Returns current iteration count for ticket."""
        scheduler = Scheduler(mock_state, mock_agents, mock_worktrees, config)
        scheduler._iteration_counts["t1"] = 3
        assert scheduler.get_iteration("t1") == 3
        assert scheduler.get_iteration("unknown") is None

    async def test_uses_assigned_hat_agent_config(self, mock_state, mock_agents, mock_worktrees):
        """Scheduler uses agent config from assigned hat when available."""
        from kagan.config import AgentConfig

        config = KaganConfig(
            general=GeneralConfig(auto_start=True),
            hats={"dev": HatConfig(agent_command="claude", args=["--model", "opus"])},
            agents={
                "dev": AgentConfig(
                    identity="dev.agent",
                    name="Dev Agent",
                    short_name="dev",
                    run_command={"*": "claude --model opus"},
                )
            },
        )
        ticket = Ticket(
            id="t6", title="Feature", status=TicketStatus.IN_PROGRESS, assigned_hat="dev"
        )
        mock_state.get_all_tickets.return_value = [ticket]

        Scheduler(mock_state, mock_agents, mock_worktrees, config)

        # Verify the config has the expected agent
        agent_config = config.get_agent("dev")
        assert agent_config is not None
        assert agent_config.short_name == "dev"
        assert agent_config.run_command["*"] == "claude --model opus"

    async def test_falls_back_to_default_agent(self, mock_state, mock_agents, mock_worktrees):
        """Scheduler falls back to default agent when no hat assigned."""
        from kagan.config import AgentConfig

        config = KaganConfig(
            general=GeneralConfig(auto_start=True),
            agents={
                "claude": AgentConfig(
                    identity="claude.com",
                    name="Claude Code",
                    short_name="claude",
                    run_command={"*": "claude"},
                    active=True,
                )
            },
        )
        ticket = Ticket(id="t7", title="Feature", status=TicketStatus.IN_PROGRESS)
        mock_state.get_all_tickets.return_value = [ticket]

        Scheduler(mock_state, mock_agents, mock_worktrees, config)

        # Verify default agent can be retrieved (first active agent)
        default = config.get_default_agent()
        assert default is not None
        name, agent_config = default
        assert name == "claude"
        assert agent_config.short_name == "claude"


class TestSchedulerStatusTransitions:
    """Tests for Scheduler status transition methods."""

    async def test_handle_complete_updates_to_review(
        self, mock_state, mock_agents, mock_worktrees, config
    ):
        """_handle_complete updates ticket status to REVIEW in database."""
        from kagan.database.models import TicketUpdate

        ticket = Ticket(id="t10", title="Complete me", status=TicketStatus.IN_PROGRESS)
        scheduler = Scheduler(mock_state, mock_agents, mock_worktrees, config)

        await scheduler._handle_complete(ticket)

        # Verify update_ticket was called with REVIEW status
        mock_state.update_ticket.assert_called_once()
        call_args = mock_state.update_ticket.call_args
        assert call_args[0][0] == "t10"  # ticket_id
        update_arg = call_args[0][1]
        assert isinstance(update_arg, TicketUpdate)
        assert update_arg.status == TicketStatus.REVIEW

    async def test_handle_blocked_updates_to_backlog(
        self, mock_state, mock_agents, mock_worktrees, config
    ):
        """_handle_blocked updates ticket status to BACKLOG."""
        from kagan.database.models import TicketUpdate

        ticket = Ticket(id="t11", title="Block me", status=TicketStatus.IN_PROGRESS)
        scheduler = Scheduler(mock_state, mock_agents, mock_worktrees, config)

        await scheduler._handle_blocked(ticket)

        # Verify update_ticket was called with BACKLOG status
        mock_state.update_ticket.assert_called_once()
        call_args = mock_state.update_ticket.call_args
        assert call_args[0][0] == "t11"  # ticket_id
        update_arg = call_args[0][1]
        assert isinstance(update_arg, TicketUpdate)
        assert update_arg.status == TicketStatus.BACKLOG

    async def test_handle_max_iterations_updates_to_backlog(
        self, mock_state, mock_agents, mock_worktrees, config
    ):
        """_handle_max_iterations updates ticket status to BACKLOG."""
        from kagan.database.models import TicketUpdate

        ticket = Ticket(id="t12", title="Too many iterations", status=TicketStatus.IN_PROGRESS)
        scheduler = Scheduler(mock_state, mock_agents, mock_worktrees, config)

        await scheduler._handle_max_iterations(ticket)

        # Verify update_ticket was called with BACKLOG status
        mock_state.update_ticket.assert_called_once()
        call_args = mock_state.update_ticket.call_args
        assert call_args[0][0] == "t12"  # ticket_id
        update_arg = call_args[0][1]
        assert isinstance(update_arg, TicketUpdate)
        assert update_arg.status == TicketStatus.BACKLOG

    async def test_update_ticket_status_calls_notification_callback(
        self, mock_state, mock_agents, mock_worktrees, config
    ):
        """_update_ticket_status calls the notification callback after DB update."""
        notification_called = []

        def on_ticket_changed():
            notification_called.append(True)

        scheduler = Scheduler(
            mock_state, mock_agents, mock_worktrees, config, on_ticket_changed=on_ticket_changed
        )

        await scheduler._update_ticket_status("t13", TicketStatus.REVIEW)

        # Verify DB was updated
        mock_state.update_ticket.assert_called_once()

        # Verify notification callback was called
        assert len(notification_called) == 1

    async def test_update_ticket_status_without_callback(
        self, mock_state, mock_agents, mock_worktrees, config
    ):
        """_update_ticket_status works when no notification callback is set."""
        scheduler = Scheduler(mock_state, mock_agents, mock_worktrees, config)

        # Should not raise even without callback
        await scheduler._update_ticket_status("t14", TicketStatus.DONE)

        # Verify DB was still updated
        mock_state.update_ticket.assert_called_once()

    async def test_handle_complete_triggers_notification(
        self, mock_state, mock_agents, mock_worktrees, config
    ):
        """_handle_complete triggers the notification callback."""
        notification_count = []

        def on_ticket_changed():
            notification_count.append(1)

        ticket = Ticket(id="t15", title="Notify on complete", status=TicketStatus.IN_PROGRESS)
        scheduler = Scheduler(
            mock_state, mock_agents, mock_worktrees, config, on_ticket_changed=on_ticket_changed
        )

        await scheduler._handle_complete(ticket)

        # Verify notification was triggered
        assert len(notification_count) == 1


class TestSchedulerReviewLoop:
    """Tests for the _run_review_loop method."""

    async def test_review_no_worktree_returns_to_backlog(
        self, mock_state, mock_agents, mock_worktrees, config
    ):
        """Returns to BACKLOG when no worktree exists."""
        ticket = Ticket(id="r1", title="No worktree", status=TicketStatus.REVIEW)
        mock_worktrees.get_path.return_value = None
        notification_called = []

        scheduler = Scheduler(
            mock_state,
            mock_agents,
            mock_worktrees,
            config,
            on_ticket_changed=lambda: notification_called.append(True),
        )

        await scheduler._run_review_loop(ticket)

        # Verify ticket updated to BACKLOG
        mock_state.update_ticket.assert_called_once()
        call_args = mock_state.update_ticket.call_args
        assert call_args[0][0] == "r1"
        assert call_args[0][1].status == TicketStatus.BACKLOG
        assert len(notification_called) == 1

    async def test_review_no_commits_returns_to_backlog(
        self, mock_state, mock_agents, mock_worktrees, config
    ):
        """Returns to BACKLOG when no commits exist."""
        ticket = Ticket(id="r2", title="No commits", status=TicketStatus.REVIEW)
        mock_worktrees.get_path.return_value = Path("/tmp/wt")
        mock_worktrees.get_commit_log = AsyncMock(return_value=[])
        notification_called = []

        scheduler = Scheduler(
            mock_state,
            mock_agents,
            mock_worktrees,
            config,
            on_ticket_changed=lambda: notification_called.append(True),
        )

        await scheduler._run_review_loop(ticket)

        # Verify ticket updated to BACKLOG
        mock_state.update_ticket.assert_called_once()
        call_args = mock_state.update_ticket.call_args
        assert call_args[0][0] == "r2"
        assert call_args[0][1].status == TicketStatus.BACKLOG
        assert len(notification_called) == 1

    async def test_review_approved_merges_and_updates_to_done(
        self, mock_state, mock_agents, mock_worktrees, review_config
    ):
        """Approved review merges to main and updates ticket to DONE."""
        ticket = Ticket(id="r3", title="Approved", status=TicketStatus.REVIEW)
        mock_worktrees.get_path.return_value = Path("/tmp/wt")
        mock_worktrees.get_commit_log = AsyncMock(return_value=["abc123 Initial commit"])
        mock_worktrees.merge_to_main = AsyncMock(return_value=(True, "merged"))
        mock_worktrees.delete = AsyncMock()

        # Mock agent
        mock_agent = MagicMock()
        mock_agent.wait_ready = AsyncMock()
        mock_agent.send_prompt = AsyncMock()
        mock_agent.get_response_text = MagicMock(return_value='<approve summary="Done"/>')
        mock_agents.spawn.return_value = mock_agent

        # Mock get_ticket for review summary update
        mock_state.get_ticket = AsyncMock(return_value=ticket)

        notification_called = []

        scheduler = Scheduler(
            mock_state,
            mock_agents,
            mock_worktrees,
            review_config,
            on_ticket_changed=lambda: notification_called.append(True),
        )

        # Mock _get_changed_files
        with patch.object(scheduler, "_get_changed_files", new_callable=AsyncMock) as mock_files:
            mock_files.return_value = ["src/main.py", "tests/test_main.py"]
            await scheduler._run_review_loop(ticket)

        # Verify ticket updated to DONE
        assert mock_state.update_ticket.called
        # Find the call with DONE status
        done_call = None
        for call in mock_state.update_ticket.call_args_list:
            if call[0][1].status == TicketStatus.DONE:
                done_call = call
                break
        assert done_call is not None
        assert done_call[0][0] == "r3"

        # Verify worktree deleted with branch
        mock_worktrees.delete.assert_called_once_with("r3", delete_branch=True)
        assert len(notification_called) >= 1

    async def test_review_approved_merge_fails_returns_to_in_progress(
        self, mock_state, mock_agents, mock_worktrees, review_config
    ):
        """Approved review with merge failure returns to IN_PROGRESS."""
        ticket = Ticket(id="r4", title="Merge fail", status=TicketStatus.REVIEW)
        mock_worktrees.get_path.return_value = Path("/tmp/wt")
        mock_worktrees.get_commit_log = AsyncMock(return_value=["abc123 Commit"])
        mock_worktrees.merge_to_main = AsyncMock(return_value=(False, "conflict in main.py"))

        # Mock agent
        mock_agent = MagicMock()
        mock_agent.wait_ready = AsyncMock()
        mock_agent.send_prompt = AsyncMock()
        mock_agent.get_response_text = MagicMock(return_value="<approve/>")
        mock_agents.spawn.return_value = mock_agent

        notification_called = []

        scheduler = Scheduler(
            mock_state,
            mock_agents,
            mock_worktrees,
            review_config,
            on_ticket_changed=lambda: notification_called.append(True),
        )

        with patch.object(scheduler, "_get_changed_files", new_callable=AsyncMock) as mock_files:
            mock_files.return_value = ["src/main.py"]
            await scheduler._run_review_loop(ticket)

        # Verify ticket updated to IN_PROGRESS
        assert mock_state.update_ticket.called
        in_progress_call = None
        for call in mock_state.update_ticket.call_args_list:
            if call[0][1].status == TicketStatus.IN_PROGRESS:
                in_progress_call = call
                break
        assert in_progress_call is not None
        assert in_progress_call[0][0] == "r4"

        # Verify scratchpad updated with merge failure
        mock_state.update_scratchpad.assert_called()
        scratchpad_call = mock_state.update_scratchpad.call_args
        assert "Merge Failed" in scratchpad_call[0][1]
        assert "conflict" in scratchpad_call[0][1]

    async def test_review_rejected_returns_to_in_progress(
        self, mock_state, mock_agents, mock_worktrees, review_config
    ):
        """Rejected review returns to IN_PROGRESS with reason in scratchpad."""
        ticket = Ticket(id="r5", title="Rejected", status=TicketStatus.REVIEW)
        mock_worktrees.get_path.return_value = Path("/tmp/wt")
        mock_worktrees.get_commit_log = AsyncMock(return_value=["abc123 Commit"])

        # Mock agent
        mock_agent = MagicMock()
        mock_agent.wait_ready = AsyncMock()
        mock_agent.send_prompt = AsyncMock()
        mock_agent.get_response_text = MagicMock(return_value='<reject reason="needs tests"/>')
        mock_agents.spawn.return_value = mock_agent

        notification_called = []

        scheduler = Scheduler(
            mock_state,
            mock_agents,
            mock_worktrees,
            review_config,
            on_ticket_changed=lambda: notification_called.append(True),
        )

        with patch.object(scheduler, "_get_changed_files", new_callable=AsyncMock) as mock_files:
            mock_files.return_value = ["src/main.py"]
            await scheduler._run_review_loop(ticket)

        # Verify ticket updated to IN_PROGRESS
        assert mock_state.update_ticket.called
        in_progress_call = None
        for call in mock_state.update_ticket.call_args_list:
            if call[0][1].status == TicketStatus.IN_PROGRESS:
                in_progress_call = call
                break
        assert in_progress_call is not None
        assert in_progress_call[0][0] == "r5"

        # Verify scratchpad updated with rejection reason
        mock_state.update_scratchpad.assert_called()
        scratchpad_call = mock_state.update_scratchpad.call_args
        assert "Review Rejected" in scratchpad_call[0][1]
        assert "needs tests" in scratchpad_call[0][1]

    async def test_review_timeout_returns_to_backlog(
        self, mock_state, mock_agents, mock_worktrees, review_config
    ):
        """Timeout during review returns to BACKLOG."""
        ticket = Ticket(id="r6", title="Timeout", status=TicketStatus.REVIEW)
        mock_worktrees.get_path.return_value = Path("/tmp/wt")
        mock_worktrees.get_commit_log = AsyncMock(return_value=["abc123 Commit"])

        # Mock agent that times out
        mock_agent = MagicMock()
        mock_agent.wait_ready = AsyncMock(side_effect=TimeoutError("Agent timed out"))
        mock_agents.spawn.return_value = mock_agent

        notification_called = []

        scheduler = Scheduler(
            mock_state,
            mock_agents,
            mock_worktrees,
            review_config,
            on_ticket_changed=lambda: notification_called.append(True),
        )

        with patch.object(scheduler, "_get_changed_files", new_callable=AsyncMock) as mock_files:
            mock_files.return_value = ["src/main.py"]
            await scheduler._run_review_loop(ticket)

        # Verify ticket updated to BACKLOG
        assert mock_state.update_ticket.called
        backlog_call = None
        for call in mock_state.update_ticket.call_args_list:
            if call[0][1].status == TicketStatus.BACKLOG:
                backlog_call = call
                break
        assert backlog_call is not None
        assert backlog_call[0][0] == "r6"

        # Verify agent was terminated
        mock_agents.terminate.assert_called_with("r6-review")

    async def test_handle_max_iterations_triggers_notification(
        self, mock_state, mock_agents, mock_worktrees, config
    ):
        """_handle_max_iterations triggers the notification callback."""
        notification_count = []

        def on_ticket_changed():
            notification_count.append(1)

        ticket = Ticket(id="t17", title="Notify on max iter", status=TicketStatus.IN_PROGRESS)
        scheduler = Scheduler(
            mock_state, mock_agents, mock_worktrees, config, on_ticket_changed=on_ticket_changed
        )

        await scheduler._handle_max_iterations(ticket)

        # Verify notification was triggered
        assert len(notification_count) == 1
