"""Settings modal tests - consolidated.

Covers: open/close, switch toggles, save/cancel, validation.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest
from textual.widgets import Input, Switch

from tests.helpers.pages import is_on_screen, navigate_to_kanban, open_settings_modal

if TYPE_CHECKING:
    from kagan.app import KaganApp

pytestmark = pytest.mark.integration


class TestSettingsOpenClose:
    """Test opening and closing settings modal."""

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
            async with open_settings_modal(pilot):
                assert is_on_screen(pilot, "SettingsModal")

                await pilot.press("escape")
                await pilot.pause()
                assert is_on_screen(pilot, "KanbanScreen")


class TestSettingsSwitches:
    """Test toggling switches - parametrized."""

    @pytest.mark.parametrize(
        "switch_id,setting_name",
        [
            ("#auto-start-switch", "auto_start"),
            ("#auto-approve-switch", "auto_approve"),
            ("#auto-merge-switch", "auto_merge"),
        ],
    )
    async def test_settings_switch_toggles(
        self, e2e_app: KaganApp, switch_id: str, setting_name: str
    ):
        """Settings switch can be toggled."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            async with open_settings_modal(pilot):
                switch = pilot.app.screen.query_one(switch_id, Switch)
                initial_value = switch.value

                await pilot.click(switch_id)
                await pilot.pause()

                assert switch.value != initial_value


class TestSettingsSave:
    """Test save persists config."""

    @pytest.mark.slow
    @pytest.mark.parametrize(
        "switch_id,config_key",
        [
            ("#auto-start-switch", "auto_start"),
            ("#auto-merge-switch", "auto_merge"),
        ],
    )
    async def test_save_persists_switch_changes(
        self, e2e_app: KaganApp, switch_id: str, config_key: str
    ):
        """Saving persists switch changes to config file."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            async with open_settings_modal(pilot):
                switch = pilot.app.screen.query_one(switch_id, Switch)
                initial_value = switch.value

                await pilot.click(switch_id)
                await pilot.pause()

                await pilot.press("ctrl+s")
                await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")

            config_content = e2e_app.config_path.read_text()
            expected_value = "true" if not initial_value else "false"
            assert f"{config_key} = {expected_value}" in config_content

    @pytest.mark.slow
    async def test_save_persists_input_changes(self, e2e_app: KaganApp):
        """Saving persists input field changes to config file."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            async with open_settings_modal(pilot):
                base_branch_input = pilot.app.screen.query_one("#base-branch-input", Input)
                base_branch_input.value = "develop"
                await pilot.pause()

                await pilot.press("ctrl+s")
                await pilot.pause()

            config_content = e2e_app.config_path.read_text()
            assert 'default_base_branch = "develop"' in config_content

    @pytest.mark.slow
    async def test_valid_numeric_values_save_successfully(self, e2e_app: KaganApp):
        """Valid numeric values save successfully."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            async with open_settings_modal(pilot):
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


class TestSettingsCancel:
    """Test cancel discards changes."""

    @pytest.mark.parametrize(
        "element_id,element_type,new_value",
        [
            ("#auto-start-switch", Switch, None),  # Toggle (no specific value needed)
            ("#base-branch-input", Input, "feature-branch"),
        ],
        ids=["switch", "input"],
    )
    async def test_escape_discards_changes(
        self, e2e_app: KaganApp, element_id: str, element_type: type, new_value: str | None
    ):
        """Escape discards changes for both switches and inputs."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            # First, get original value
            async with open_settings_modal(pilot):
                element = pilot.app.screen.query_one(element_id, element_type)
                original_value = element.value

                # Make a change
                if element_type == Switch:
                    await pilot.click(element_id)
                else:
                    element.value = new_value
                await pilot.pause()

                await pilot.press("escape")
                await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")

            # Re-open and verify original value preserved
            async with open_settings_modal(pilot):
                element = pilot.app.screen.query_one(element_id, element_type)
                assert element.value == original_value


class TestSettingsValidation:
    """Test invalid input handling - parametrized."""

    @pytest.mark.parametrize(
        "input_id,invalid_value",
        [
            ("#max-agents-input", "invalid"),
            ("#iteration-delay-input", "not-a-number"),
        ],
        ids=["max_agents", "iteration_delay"],
    )
    async def test_invalid_input_shows_error(
        self, e2e_app: KaganApp, input_id: str, invalid_value: str
    ):
        """Invalid input value keeps modal open (save failed)."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            async with open_settings_modal(pilot):
                input_field = pilot.app.screen.query_one(input_id, Input)
                input_field.value = invalid_value
                await pilot.pause()

                await pilot.press("ctrl+s")
                await pilot.pause()

                # Modal should still be open (save failed)
                assert is_on_screen(pilot, "SettingsModal")
