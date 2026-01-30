"""Header widget for Kagan TUI."""

from __future__ import annotations

import asyncio
from contextlib import suppress
from importlib.metadata import PackageNotFoundError, version
from typing import TYPE_CHECKING

from textual.css.query import NoMatches
from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Label

from kagan.constants import KAGAN_LOGO_SMALL

if TYPE_CHECKING:
    from pathlib import Path

    from textual.app import ComposeResult


def _get_version() -> str:
    """Get package version, fallback to dev."""
    try:
        return version("kagan")
    except PackageNotFoundError:
        return "dev"


async def _get_git_branch(repo_root: Path) -> str:
    """Get current git branch name."""
    try:
        proc = await asyncio.create_subprocess_exec(
            "git",
            "rev-parse",
            "--abbrev-ref",
            "HEAD",
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd=repo_root,
        )
        stdout, _ = await proc.communicate()
        if proc.returncode == 0:
            return stdout.decode().strip()
    except (OSError, FileNotFoundError):
        pass
    return ""


class KaganHeader(Widget):
    """Header widget displaying app logo, title, version, and stats."""

    ticket_count: reactive[int] = reactive(0)
    active_sessions: reactive[int] = reactive(0)
    git_branch: reactive[str] = reactive("")

    def __init__(self, ticket_count: int = 0, **kwargs) -> None:
        super().__init__(**kwargs)
        self.ticket_count = ticket_count

    def compose(self) -> ComposeResult:
        yield Label(KAGAN_LOGO_SMALL, classes="header-logo")
        yield Label("KAGAN", classes="header-title")
        yield Label(f"v{_get_version()}", classes="header-version")
        yield Label("", classes="header-spacer")
        yield Label("", id="header-branch", classes="header-branch")
        yield Label("", id="header-sessions", classes="header-sessions")
        yield Label(f"Tickets: {self.ticket_count}", id="header-stats", classes="header-stats")

    def watch_ticket_count(self, count: int) -> None:
        with suppress(NoMatches):
            self.query_one("#header-stats", Label).update(f"Tickets: {count}")

    def watch_active_sessions(self, count: int) -> None:
        with suppress(NoMatches):
            label = self.query_one("#header-sessions", Label)
            label.update(f"Sessions: {count}")

    def watch_git_branch(self, branch: str) -> None:
        with suppress(NoMatches):
            label = self.query_one("#header-branch", Label)
            if branch:
                label.update(f"⎇ {branch}")
            else:
                label.update("")

    def update_count(self, count: int) -> None:
        self.ticket_count = count

    def update_sessions(self, active: int) -> None:
        self.active_sessions = active

    def update_branch(self, branch: str) -> None:
        self.git_branch = branch
