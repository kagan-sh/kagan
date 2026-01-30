"""Settings modal tests - open, close, and switches."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest
from textual.widgets import Switch

from tests.helpers.pages import is_on_screen, navigate_to_kanban

if TYPE_CHECKING:
    from kagan.app import KaganApp

pytestmark = pytest.mark.integration


class TestSettingsOpen:
    """Test opening settings modal."""

    async def test_comma_opens_settings(self, e2e_app: KaganApp):
        """Pressing comma opens settings modal."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            await pilot.pause()

            await pilot.press("comma")
            await pilot.pause()

            assert is_on_screen(pilot, "SettingsModal")

    async def test_escape_closes_settings(self, e2e_app: KaganApp):
        """Pressing escape closes settings modal."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            await pilot.pause()

            await pilot.press("comma")
            await pilot.pause()
            assert is_on_screen(pilot, "SettingsModal")

            await pilot.press("escape")
            await pilot.pause()
            assert is_on_screen(pilot, "KanbanScreen")


class TestSettingsSwitches:
    """Test toggling switches."""

    async def test_auto_start_switch_toggles(self, e2e_app: KaganApp):
        """Auto-start switch can be toggled."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            await pilot.pause()

            await pilot.press("comma")
            await pilot.pause()

            switch = pilot.app.screen.query_one("#auto-start-switch", Switch)
            initial_value = switch.value

            await pilot.click("#auto-start-switch")
            await pilot.pause()

            assert switch.value != initial_value

    async def test_auto_approve_switch_toggles(self, e2e_app: KaganApp):
        """Auto-approve switch can be toggled."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            await pilot.pause()

            await pilot.press("comma")
            await pilot.pause()

            switch = pilot.app.screen.query_one("#auto-approve-switch", Switch)
            initial_value = switch.value

            await pilot.click("#auto-approve-switch")
            await pilot.pause()

            assert switch.value != initial_value

    async def test_auto_merge_switch_toggles(self, e2e_app: KaganApp):
        """Auto-merge switch can be toggled."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            await pilot.pause()

            await pilot.press("comma")
            await pilot.pause()

            switch = pilot.app.screen.query_one("#auto-merge-switch", Switch)
            initial_value = switch.value

            await pilot.click("#auto-merge-switch")
            await pilot.pause()

            assert switch.value != initial_value
