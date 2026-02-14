"""TUI GitHub status visibility tests.

Verifies user-facing behavior for GitHub-connected repo indicators.
"""

from __future__ import annotations

from kagan.tui.ui.screens.kanban.hints import build_kanban_hints
from kagan.tui.ui.widgets.header import (
    GITHUB_ICON_PENDING,
    GITHUB_ICON_SYNCED,
    KaganHeader,
)


class TestGitHubStatusHeader:
    """Tests for GitHub status display in header widget."""

    def test_header_hides_github_status_when_not_connected(self) -> None:
        """Verify GitHub status is hidden when repo is not connected."""
        header = KaganHeader()
        header.update_github_status(connected=False)

        assert header.github_connected is False
        assert header.github_synced is False

    def test_header_shows_pending_when_connected_not_synced(self) -> None:
        """Verify pending icon shown when connected but not synced."""
        header = KaganHeader()
        header.update_github_status(connected=True, synced=False)

        assert header.github_connected is True
        assert header.github_synced is False

    def test_header_shows_synced_when_connected_and_synced(self) -> None:
        """Verify synced icon shown when connected and synced."""
        header = KaganHeader()
        header.update_github_status(connected=True, synced=True)

        assert header.github_connected is True
        assert header.github_synced is True


class TestGitHubHintsVisibility:
    """Tests for GitHub sync hint visibility in keybinding hints."""

    def test_hints_show_sync_when_github_connected(self) -> None:
        """Verify sync hint appears in global hints when GitHub is connected."""
        hints = build_kanban_hints(None, None, github_connected=True)

        hint_actions = [label for _, label in hints.global_hints]
        assert "sync" in hint_actions

    def test_hints_hide_sync_when_not_connected(self) -> None:
        """Verify sync hint is absent when GitHub is not connected."""
        hints = build_kanban_hints(None, None, github_connected=False)

        hint_actions = [label for _, label in hints.global_hints]
        assert "sync" not in hint_actions

    def test_hints_include_github_connected_flag(self) -> None:
        """Verify KanbanHints includes github_connected state."""
        hints_connected = build_kanban_hints(None, None, github_connected=True)
        hints_disconnected = build_kanban_hints(None, None, github_connected=False)

        assert hints_connected.github_connected is True
        assert hints_disconnected.github_connected is False


class TestGitHubStatusIcons:
    """Tests for GitHub status icon constants."""

    def test_icon_constants_are_distinct(self) -> None:
        """Verify status icons are visually distinct."""
        assert GITHUB_ICON_SYNCED != GITHUB_ICON_PENDING
        assert len(GITHUB_ICON_SYNCED) == 1
        assert len(GITHUB_ICON_PENDING) == 1
