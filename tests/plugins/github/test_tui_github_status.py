"""TUI plugin badge visibility tests (formerly GitHub-specific)."""

from __future__ import annotations

import pytest
from textual.app import App, ComposeResult
from textual.widgets import Label

from kagan.tui.ui.screens.kanban.hints import build_kanban_hints
from kagan.tui.ui.widgets.header import KaganHeader


class _HeaderHostApp(App[None]):
    """Minimal app hosting only the header widget."""

    def compose(self) -> ComposeResult:
        yield KaganHeader()


class TestPluginBadgeHeader:
    """Tests for rendered plugin badge behavior in the header widget."""

    @pytest.mark.asyncio
    async def test_header_hides_badge_when_no_badges(self) -> None:
        app = _HeaderHostApp()
        async with app.run_test(size=(120, 40)) as pilot:
            header = pilot.app.query_one(KaganHeader)
            status = header.query_one("#header-github-status", Label)
            separator = header.query_one("#sep-github", Label)

            header.update_plugin_badges(None)
            await pilot.pause()

            assert status.display is False
            assert separator.display is False
            assert str(status.content) == ""

    @pytest.mark.asyncio
    async def test_header_shows_badge_with_ok_state(self) -> None:
        app = _HeaderHostApp()
        async with app.run_test(size=(120, 40)) as pilot:
            header = pilot.app.query_one(KaganHeader)
            status = header.query_one("#header-github-status", Label)
            separator = header.query_one("#sep-github", Label)

            header.update_plugin_badges([{"label": "GitHub", "state": "ok"}])
            await pilot.pause()

            assert status.display is True
            assert separator.display is True
            assert str(status.content) == "◉ GitHub"

    @pytest.mark.asyncio
    async def test_header_shows_badge_with_text_and_pending_state(self) -> None:
        app = _HeaderHostApp()
        async with app.run_test(size=(120, 40)) as pilot:
            header = pilot.app.query_one(KaganHeader)
            status = header.query_one("#header-github-status", Label)

            header.update_plugin_badges([{"label": "GitHub", "text": "synced", "state": "ok"}])
            await pilot.pause()

            assert status.display is True
            assert str(status.content) == "◉ GitHub synced"


class TestPluginHintsVisibility:
    """Tests for plugin-related hint visibility in keybinding hints."""

    def test_hints_do_not_contain_sync(self) -> None:
        hints = build_kanban_hints(None, None)
        hint_actions = [label for _, label in hints.global_hints]
        assert "sync" not in hint_actions
