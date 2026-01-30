"""E2E tests for PlannerScreen interactions.

Tests: screen navigation, input behavior, header display.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

import pytest

from tests.helpers.pages import is_on_screen

if TYPE_CHECKING:
    from kagan.app import KaganApp
    from kagan.ui.screens.planner import PlannerScreen

pytestmark = pytest.mark.e2e


class TestPlannerScreenNavigation:
    """Test planner screen navigation."""

    async def test_p_opens_planner(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'p' opens the planner screen."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("p")
            await pilot.pause()
            assert is_on_screen(pilot, "PlannerScreen")

    async def test_escape_from_planner_goes_to_board(self, e2e_app_with_tickets: KaganApp):
        """Pressing escape on planner should navigate to board."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("p")
            await pilot.pause()
            assert is_on_screen(pilot, "PlannerScreen")
            await pilot.press("escape")
            await pilot.pause()
            assert is_on_screen(pilot, "KanbanScreen")


class TestPlannerScreenWidgets:
    """Test planner screen widget behavior."""

    async def test_planner_has_header(self, e2e_app_with_tickets: KaganApp):
        """Planner screen should display the header widget."""
        from kagan.ui.widgets.header import KaganHeader

        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("p")
            await pilot.pause()
            assert is_on_screen(pilot, "PlannerScreen")
            headers = list(pilot.app.screen.query(KaganHeader))
            assert len(headers) == 1, "Planner screen should have KaganHeader"

    async def test_planner_input_is_focused(self, e2e_app_with_tickets: KaganApp):
        """Planner input should be focused after agent is ready."""
        from kagan.acp import messages
        from kagan.ui.screens.planner import PlannerInput

        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("p")
            await pilot.pause()
            assert is_on_screen(pilot, "PlannerScreen")

            screen = cast("PlannerScreen", pilot.app.screen)
            await screen.on_agent_ready(messages.AgentReady())
            await pilot.pause()

            planner_input = screen.query_one("#planner-input", PlannerInput)
            assert not planner_input.read_only, "Input should not be read-only"
            focused = pilot.app.focused
            assert isinstance(focused, PlannerInput), "PlannerInput should be focused"
            assert focused.id == "planner-input"


class TestDeselect:
    """Test deselection behavior."""

    async def test_escape_deselects_card(self, e2e_app_with_tickets: KaganApp):
        """Pressing escape deselects the current card."""
        from tests.helpers.pages import focus_first_ticket

        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            assert pilot.app.focused is not None
            await pilot.press("escape")
            await pilot.pause()
