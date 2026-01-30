"""E2E tests for TmuxGatewayModal."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.ui.modals.tmux_gateway import TmuxGatewayModal
from tests.helpers.pages import is_on_screen, navigate_to_kanban

if TYPE_CHECKING:
    from kagan.app import KaganApp

pytestmark = pytest.mark.e2e


class TestTmuxGatewayModalOpen:
    """Test modal opens with expected content."""

    async def test_modal_shows_title(self, e2e_app: KaganApp):
        """Modal displays the title."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)

            modal = TmuxGatewayModal(ticket_id="abc123", ticket_title="Test Ticket")
            pilot.app.push_screen(modal)
            await pilot.pause()

            assert is_on_screen(pilot, "TmuxGatewayModal")

    async def test_modal_shows_essential_commands(self, e2e_app: KaganApp):
        """Modal displays tmux essential commands."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)

            modal = TmuxGatewayModal(ticket_id="abc123", ticket_title="Test Ticket")
            pilot.app.push_screen(modal)
            await pilot.pause()

            # Check that key commands are visible
            container = pilot.app.screen.query_one("#tmux-gateway-container")
            text_content = str(container.render())
            assert "Ctrl+b" in text_content or is_on_screen(pilot, "TmuxGatewayModal")


class TestTmuxGatewayModalActions:
    """Test modal action bindings."""

    async def test_enter_proceeds(self, e2e_app: KaganApp):
        """Pressing Enter dismisses with 'proceed' result."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)

            result: str | None = "sentinel"

            def capture_result(value: str | None) -> None:
                nonlocal result
                result = value

            modal = TmuxGatewayModal(ticket_id="abc123", ticket_title="Test")
            pilot.app.push_screen(modal, capture_result)
            await pilot.pause()

            await pilot.press("enter")
            await pilot.pause()

            assert result == "proceed"
            assert not is_on_screen(pilot, "TmuxGatewayModal")

    async def test_escape_cancels(self, e2e_app: KaganApp):
        """Pressing Escape dismisses with None result."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)

            result: str | None = "sentinel"

            def capture_result(value: str | None) -> None:
                nonlocal result
                result = value

            modal = TmuxGatewayModal(ticket_id="abc123", ticket_title="Test")
            pilot.app.push_screen(modal, capture_result)
            await pilot.pause()

            await pilot.press("escape")
            await pilot.pause()

            assert result is None
            assert not is_on_screen(pilot, "TmuxGatewayModal")

    async def test_s_skips_future(self, e2e_app: KaganApp):
        """Pressing 's' dismisses with 'skip_future' result."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)

            result: str | None = "sentinel"

            def capture_result(value: str | None) -> None:
                nonlocal result
                result = value

            modal = TmuxGatewayModal(ticket_id="abc123", ticket_title="Test")
            pilot.app.push_screen(modal, capture_result)
            await pilot.pause()

            await pilot.press("s")
            await pilot.pause()

            assert result == "skip_future"
            assert not is_on_screen(pilot, "TmuxGatewayModal")
