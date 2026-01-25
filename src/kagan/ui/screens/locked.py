"""Locked instance screen shown when another Kagan is running."""

from __future__ import annotations

from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Center, Container, Middle
from textual.widgets import Footer, Static

from kagan.ui.widgets.header import KaganHeader


class InstanceLockedApp(App):
    """Minimal app shown when another Kagan instance is already running."""

    TITLE = "KAGAN"

    CSS = """
    Screen {
        layout: vertical;
    }

    KaganHeader {
        width: 100%;
        height: 3;
        background: $primary;
        color: $text;
        padding: 1;
    }

    KaganHeader .header-title {
        text-style: bold;
        width: auto;
    }

    KaganHeader .header-stats {
        dock: right;
        width: auto;
        padding-right: 2;
    }

    KaganHeader .header-help {
        dock: right;
        width: auto;
        padding-right: 1;
        color: $text-muted;
    }

    #lock-container {
        width: 100%;
        height: 1fr;
        align: center middle;
    }

    #lock-card {
        width: 60;
        height: auto;
        border: thick $error;
        background: $surface;
        padding: 2 4;
    }

    #lock-title {
        text-align: center;
        text-style: bold;
        width: 100%;
        padding-bottom: 1;
    }

    #lock-message {
        text-align: center;
        width: 100%;
        color: $text-muted;
    }

    #lock-hint {
        text-align: center;
        width: 100%;
        padding-top: 1;
        color: $text-muted;
    }
    """

    BINDINGS = [
        Binding("q", "quit", "Quit"),
        Binding("escape", "quit", "Quit"),
        Binding("enter", "quit", "Quit"),
    ]

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
