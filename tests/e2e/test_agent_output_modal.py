"""Tests for AgentOutputModal."""

from __future__ import annotations

from typing import Any, cast

import pytest
from textual.app import App, ComposeResult
from textual.containers import Vertical

from kagan.acp import messages
from kagan.database.models import Ticket, TicketStatus
from kagan.ui.modals.agent_output import AgentOutputModal
from kagan.ui.widgets import StreamingOutput

pytestmark = pytest.mark.e2e


def _make_ticket() -> Ticket:
    return Ticket(id="abc12345", title="Test ticket", status=TicketStatus.IN_PROGRESS)


class ModalTestApp(App):
    def compose(self) -> ComposeResult:
        yield Vertical(id="container")


class TestAgentOutputModalComposition:
    async def test_composes_key_widgets(self):
        app = ModalTestApp()
        async with app.run_test() as pilot:
            modal = AgentOutputModal(ticket=_make_ticket(), agent=None)
            await app.push_screen(modal)
            await pilot.pause()
            assert modal.query_one("#agent-output", StreamingOutput)
            assert modal.query_one("#cancel-btn")
            assert modal.query_one("#close-btn")

    async def test_displays_ticket_info(self):
        app = ModalTestApp()
        async with app.run_test() as pilot:
            modal = AgentOutputModal(ticket=_make_ticket(), agent=None, iteration=3)
            await app.push_screen(modal)
            await pilot.pause()
            text = modal.query_one(".modal-subtitle").render()
            assert "abc1234" in str(text)
            assert "3" in str(text)


class TestAgentOutputModalMessages:
    async def test_agent_update_posts_response(self):
        app = ModalTestApp()
        async with app.run_test() as pilot:
            modal = AgentOutputModal(ticket=_make_ticket(), agent=None)
            await app.push_screen(modal)
            await pilot.pause()
            msg = messages.AgentUpdate(type="text", text="Hello agent")
            await modal.on_agent_update(msg)
            await pilot.pause()

    async def test_thinking_posts_thought(self):
        app = ModalTestApp()
        async with app.run_test() as pilot:
            modal = AgentOutputModal(ticket=_make_ticket(), agent=None)
            await app.push_screen(modal)
            await pilot.pause()
            msg = messages.Thinking(type="thinking", text="Reasoning...")
            await modal.on_agent_thinking(msg)
            await pilot.pause()

    async def test_tool_call_handler(self):
        app = ModalTestApp()
        async with app.run_test() as pilot:
            modal = AgentOutputModal(ticket=_make_ticket(), agent=None)
            await app.push_screen(modal)
            await pilot.pause()
            msg = messages.ToolCall(
                tool_call=cast("Any", {"id": "t1", "title": "read", "kind": "execute"})
            )
            await modal.on_tool_call(msg)
            await pilot.pause()

    async def test_set_modes_stores_modes(self):
        app = ModalTestApp()
        async with app.run_test():
            modal = AgentOutputModal(ticket=_make_ticket(), agent=None)
            await app.push_screen(modal)
            mode = messages.Mode(id="code", name="Code", description="Code mode")
            msg = messages.SetModes(current_mode="code", modes={"code": mode})
            modal.on_set_modes(msg)
            assert modal._current_mode == "code"
            assert "code" in modal._available_modes

    async def test_mode_update_changes_current(self):
        app = ModalTestApp()
        async with app.run_test():
            modal = AgentOutputModal(ticket=_make_ticket(), agent=None)
            await app.push_screen(modal)
            modal.on_mode_update(messages.ModeUpdate(current_mode="plan"))
            assert modal._current_mode == "plan"


class TestAgentOutputModalActions:
    async def test_action_close_dismisses(self):
        app = ModalTestApp()
        async with app.run_test() as pilot:
            modal = AgentOutputModal(ticket=_make_ticket(), agent=None)
            await app.push_screen(modal)
            await pilot.pause()
            modal.action_close()
            await pilot.pause()
            assert modal not in app.screen_stack

    async def test_cancel_with_no_agent_notifies(self, mocker):
        app = ModalTestApp()
        async with app.run_test() as pilot:
            modal = AgentOutputModal(ticket=_make_ticket(), agent=None)
            await app.push_screen(modal)
            await pilot.pause()

            # Mock scheduler with is_running returning False
            mock_scheduler = mocker.MagicMock()
            mock_scheduler.is_running = mocker.MagicMock(return_value=False)
            app.scheduler = mock_scheduler  # type: ignore[attr-defined]

            await modal.action_cancel_agent()
            await pilot.pause()

    async def test_cancel_with_agent_calls_stop_ticket(self, mocker):
        app = ModalTestApp()
        async with app.run_test() as pilot:
            mock_agent = mocker.MagicMock()
            mock_agent.set_message_target = mocker.MagicMock()
            modal = AgentOutputModal(ticket=_make_ticket(), agent=mock_agent)
            await app.push_screen(modal)
            await pilot.pause()

            # Mock the scheduler and state_manager on the app
            mock_scheduler = mocker.MagicMock()
            mock_scheduler.is_running = mocker.MagicMock(return_value=True)
            mock_scheduler.stop_ticket = mocker.AsyncMock(return_value=True)
            app.scheduler = mock_scheduler  # type: ignore[attr-defined]

            mock_state_manager = mocker.MagicMock()
            mock_state_manager.move_ticket = mocker.AsyncMock()
            app.state_manager = mock_state_manager  # type: ignore[attr-defined]

            await modal.action_cancel_agent()
            mock_scheduler.stop_ticket.assert_awaited_once()
            # Verify ticket was moved to BACKLOG
            mock_state_manager.move_ticket.assert_awaited_once()
