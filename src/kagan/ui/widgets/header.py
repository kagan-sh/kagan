"""Header widget for Kagan TUI."""

from __future__ import annotations

import asyncio
from contextlib import suppress
from importlib.metadata import version
from typing import TYPE_CHECKING

from textual.css.query import NoMatches
from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Label

if TYPE_CHECKING:
    from pathlib import Path

    from textual.app import ComposeResult

# Khagan logo using Canadian Aboriginal syllabics
KAGAN_LOGO = "ᘚᘛ"


def _get_version() -> str:
    """Get package version, fallback to dev."""
    try:
        return version("kagan")
    except Exception:
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
    except Exception:
        pass
    return ""


class KaganHeader(Widget):
    """Header widget displaying app logo, title, version, and stats."""

    ticket_count: reactive[int] = reactive(0)
    active_agents: reactive[int] = reactive(0)
    max_agents: reactive[int] = reactive(3)
    git_branch: reactive[str] = reactive("")

    def __init__(self, ticket_count: int = 0, **kwargs) -> None:
        super().__init__(**kwargs)
        self.ticket_count = ticket_count

    def compose(self) -> ComposeResult:
        yield Label(KAGAN_LOGO, classes="header-logo")
        yield Label("KAGAN", classes="header-title")
        yield Label(f"v{_get_version()}", classes="header-version")
        yield Label("", classes="header-spacer")
        yield Label("", id="header-branch", classes="header-branch")
        yield Label("", id="header-agents", classes="header-agents")
        yield Label(f"Tickets: {self.ticket_count}", id="header-stats", classes="header-stats")

    def watch_ticket_count(self, count: int) -> None:
        with suppress(NoMatches):
            self.query_one("#header-stats", Label).update(f"Tickets: {count}")

    def watch_active_agents(self, count: int) -> None:
        self._update_agents_display()

    def watch_max_agents(self, max_count: int) -> None:
        self._update_agents_display()

    def watch_git_branch(self, branch: str) -> None:
        with suppress(NoMatches):
            label = self.query_one("#header-branch", Label)
            if branch:
                label.update(f"⎇ {branch}")
            else:
                label.update("")

    def _update_agents_display(self) -> None:
        with suppress(NoMatches):
            label = self.query_one("#header-agents", Label)
            label.update(f"Agents: {self.active_agents}/{self.max_agents}")
            # Update style based on utilization
            label.remove_class("agents-idle", "agents-working", "agents-full")
            if self.active_agents == 0:
                label.add_class("agents-idle")
            elif self.active_agents >= self.max_agents:
                label.add_class("agents-full")
            else:
                label.add_class("agents-working")

    def update_count(self, count: int) -> None:
        self.ticket_count = count

    def update_agents(self, active: int, max_agents: int) -> None:
        self.active_agents = active
        self.max_agents = max_agents

    def update_branch(self, branch: str) -> None:
        self.git_branch = branch
