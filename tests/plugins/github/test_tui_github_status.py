"""TUI GitHub status visibility tests."""

from __future__ import annotations

import pytest
from textual.app import App, ComposeResult
from textual.widgets import Label

from kagan.tui.ui.screens.kanban.hints import build_kanban_hints
from kagan.tui.ui.widgets.header import GITHUB_ICON_PENDING, GITHUB_ICON_SYNCED, KaganHeader


class _HeaderHostApp(App[None]):
    """Minimal app hosting only the header widget."""

    def compose(self) -> ComposeResult:
        yield KaganHeader()


class TestGitHubStatusHeader:
    """Tests for rendered GitHub status behavior in the header widget."""

    @pytest.mark.asyncio
    async def test_header_hides_github_status_when_not_connected(self) -> None:
        app = _HeaderHostApp()
        async with app.run_test(size=(120, 40)) as pilot:
            header = pilot.app.query_one(KaganHeader)
            status = header.query_one("#header-github-status", Label)
            separator = header.query_one("#sep-github", Label)

            header.update_github_status(connected=False)
            await pilot.pause()

            assert status.display is False
            assert separator.display is False
            assert str(status.content) == ""

    @pytest.mark.asyncio
    async def test_header_shows_pending_icon_when_connected_not_synced(self) -> None:
        app = _HeaderHostApp()
        async with app.run_test(size=(120, 40)) as pilot:
            header = pilot.app.query_one(KaganHeader)
            status = header.query_one("#header-github-status", Label)
            separator = header.query_one("#sep-github", Label)

            header.update_github_status(connected=True, synced=False)
            await pilot.pause()

            assert status.display is True
            assert separator.display is True
            assert str(status.content) == f"{GITHUB_ICON_PENDING} GitHub"

    @pytest.mark.asyncio
    async def test_header_shows_synced_icon_when_connected_and_synced(self) -> None:
        app = _HeaderHostApp()
        async with app.run_test(size=(120, 40)) as pilot:
            header = pilot.app.query_one(KaganHeader)
            status = header.query_one("#header-github-status", Label)

            header.update_github_status(connected=True, synced=True)
            await pilot.pause()

            assert status.display is True
            assert str(status.content) == f"{GITHUB_ICON_SYNCED} GitHub"


class TestGitHubHintsVisibility:
    """Tests for GitHub sync hint visibility in keybinding hints."""

    def test_hints_show_sync_when_github_connected(self) -> None:
        hints = build_kanban_hints(None, None, github_connected=True)
        hint_actions = [label for _, label in hints.global_hints]
        assert "sync" in hint_actions

    def test_hints_hide_sync_when_not_connected(self) -> None:
        hints = build_kanban_hints(None, None, github_connected=False)
        hint_actions = [label for _, label in hints.global_hints]
        assert "sync" not in hint_actions
