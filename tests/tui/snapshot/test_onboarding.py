"""Snapshot tests for first-boot onboarding flow.

These tests cover the OnboardingScreen appearance on first boot.

Note: Tests are synchronous because pytest-textual-snapshot's snap_compare
internally calls asyncio.run(), which conflicts with async test functions.
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, Any

from kagan.tui.ui.screens.onboarding import OnboardingScreen

if TYPE_CHECKING:
    from textual.pilot import Pilot


class TestOnboardingFlow:
    def test_onboarding_initial_screen(
        self,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """OnboardingScreen appears correctly on first boot."""
        from textual.app import App

        class OnboardingTestApp(App):
            """Test app that pushes OnboardingScreen on mount."""

            _app_file = __import__("kagan.tui.app", fromlist=["app"]).__file__
            assert _app_file is not None
            CSS_PATH = str(Path(_app_file).parent / "styles" / "kagan.tcss")

            async def on_mount(self) -> None:
                """Push OnboardingScreen to the screen stack."""
                await self.push_screen(OnboardingScreen())

        app = OnboardingTestApp()

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()

            assert isinstance(pilot.app.screen, OnboardingScreen)
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)
