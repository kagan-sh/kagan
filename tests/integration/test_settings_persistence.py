"""Settings modal tests - save, cancel, and validation."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest
from textual.widgets import Input, Switch

from tests.helpers.pages import is_on_screen, navigate_to_kanban

if TYPE_CHECKING:
    from kagan.app import KaganApp

pytestmark = pytest.mark.integration


class TestSettingsSave:
    """Test save persists config."""

    @pytest.mark.slow
    async def test_save_persists_switch_changes(self, e2e_app: KaganApp):
        """Saving persists switch changes to config file."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            await pilot.pause()

            await pilot.press("comma")
            await pilot.pause()

            switch = pilot.app.screen.query_one("#auto-start-switch", Switch)
            initial_value = switch.value

            await pilot.click("#auto-start-switch")
            await pilot.pause()

            await pilot.press("ctrl+s")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")

            config_content = e2e_app.config_path.read_text()
            expected_value = "true" if not initial_value else "false"
            assert f"auto_start = {expected_value}" in config_content

    @pytest.mark.slow
    async def test_save_persists_input_changes(self, e2e_app: KaganApp):
        """Saving persists input field changes to config file."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            await pilot.pause()

            await pilot.press("comma")
            await pilot.pause()

            base_branch_input = pilot.app.screen.query_one("#base-branch-input", Input)
            base_branch_input.value = "develop"
            await pilot.pause()

            await pilot.press("ctrl+s")
            await pilot.pause()

            config_content = e2e_app.config_path.read_text()
            assert 'default_base_branch = "develop"' in config_content

    @pytest.mark.slow
    async def test_ctrl_s_saves_settings(self, e2e_app: KaganApp):
        """Ctrl+S saves settings and closes modal."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            await pilot.pause()

            await pilot.press("comma")
            await pilot.pause()

            await pilot.click("#auto-merge-switch")
            await pilot.pause()

            await pilot.press("ctrl+s")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")


class TestSettingsCancel:
    """Test cancel discards changes."""

    async def test_cancel_discards_switch_changes(self, e2e_app: KaganApp):
        """Escape discards switch changes."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            await pilot.pause()

            await pilot.press("comma")
            await pilot.pause()

            switch = pilot.app.screen.query_one("#auto-start-switch", Switch)
            initial_value = switch.value

            await pilot.click("#auto-start-switch")
            await pilot.pause()

            await pilot.press("escape")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")

            await pilot.press("comma")
            await pilot.pause()

            switch = pilot.app.screen.query_one("#auto-start-switch", Switch)
            assert switch.value == initial_value

    async def test_escape_discards_changes(self, e2e_app: KaganApp):
        """Escape key discards changes."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            await pilot.pause()

            await pilot.press("comma")
            await pilot.pause()

            base_branch_input = pilot.app.screen.query_one("#base-branch-input", Input)
            original_value = base_branch_input.value
            base_branch_input.value = "feature-branch"
            await pilot.pause()

            await pilot.press("escape")
            await pilot.pause()

            await pilot.press("comma")
            await pilot.pause()

            base_branch_input = pilot.app.screen.query_one("#base-branch-input", Input)
            assert base_branch_input.value == original_value


class TestSettingsValidation:
    """Test invalid input handling."""

    async def test_invalid_max_agents_shows_error(self, e2e_app: KaganApp):
        """Invalid max_agents value shows error notification."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            await pilot.pause()

            await pilot.press("comma")
            await pilot.pause()

            max_agents_input = pilot.app.screen.query_one("#max-agents-input", Input)
            max_agents_input.value = "invalid"
            await pilot.pause()

            await pilot.press("ctrl+s")
            await pilot.pause()

            # Modal should still be open (save failed)
            assert is_on_screen(pilot, "SettingsModal")

    async def test_invalid_iteration_delay_shows_error(self, e2e_app: KaganApp):
        """Invalid iteration_delay value shows error notification."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            await pilot.pause()

            await pilot.press("comma")
            await pilot.pause()

            delay_input = pilot.app.screen.query_one("#iteration-delay-input", Input)
            delay_input.value = "not-a-number"
            await pilot.pause()

            await pilot.press("ctrl+s")
            await pilot.pause()

            assert is_on_screen(pilot, "SettingsModal")

    async def test_valid_numeric_values_save_successfully(self, e2e_app: KaganApp):
        """Valid numeric values save successfully."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            await pilot.pause()

            await pilot.press("comma")
            await pilot.pause()

            max_agents_input = pilot.app.screen.query_one("#max-agents-input", Input)
            max_agents_input.value = "5"
            await pilot.pause()

            max_iter_input = pilot.app.screen.query_one("#max-iterations-input", Input)
            max_iter_input.value = "20"
            await pilot.pause()

            delay_input = pilot.app.screen.query_one("#iteration-delay-input", Input)
            delay_input.value = "3.5"
            await pilot.pause()

            await pilot.press("ctrl+s")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")

            config_content = e2e_app.config_path.read_text()
            assert "max_concurrent_agents = 5" in config_content
            assert "max_iterations = 20" in config_content
            assert "iteration_delay_seconds = 3.5" in config_content
