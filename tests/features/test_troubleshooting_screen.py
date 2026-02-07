"""Tests for troubleshooting screen configuration."""

from __future__ import annotations

from pathlib import Path

from kagan.ui.screens.troubleshooting.screen import TroubleshootingApp


def test_troubleshooting_css_path_exists() -> None:
    """Troubleshooting screen CSS path resolves to the packaged stylesheet."""
    css_path = Path(TroubleshootingApp.CSS_PATH)
    assert css_path.name == "kagan.tcss"
    assert css_path.exists()
