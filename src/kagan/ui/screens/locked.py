"""Locked instance screen shown when another Kagan is running."""

from __future__ import annotations

from pathlib import Path

from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Center, Container, Middle
from textual.widgets import Footer, Static

from kagan.theme import KAGAN_THEME
from kagan.ui.widgets.header import KaganHeader


class InstanceLockedApp(App):
    """Minimal app shown when another Kagan instance is already running."""

    TITLE = "KAGAN"
    CSS_PATH = str(Path(__file__).resolve().parents[2] / "styles" / "kagan.tcss")

    BINDINGS = [
        Binding("q", "quit", "Quit"),
        Binding("escape", "quit", "Quit"),
        Binding("enter", "quit", "Quit"),
    ]

    def __init__(self) -> None:
        super().__init__()
        self.register_theme(KAGAN_THEME)
        self.theme = "kagan"

    def compose(self) -> ComposeResult:
        yield KaganHeader()
        with Container(id="lock-container"):
            with Middle():
                with Center():
                    with Static(id="lock-card"):
                        yield Static("Another Instance Running", id="lock-title")
                        yield Static(
                            "Another Kagan instance is already running\n"
                            "in this folder.\n\n"
                            "Please return to that window or close it\n"
                            "and restart this one.",
                            id="lock-message",
                        )
                        yield Static("Press q to exit", id="lock-hint")
        yield Footer()
