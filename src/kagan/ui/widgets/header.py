"""Header widget for Kagan TUI."""

from __future__ import annotations

import asyncio
from importlib.metadata import PackageNotFoundError, version
from typing import TYPE_CHECKING

from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Label

from kagan.builtin_agents import get_builtin_agent
from kagan.constants import KAGAN_LOGO_SMALL
from kagan.ui.utils import safe_query_one

if TYPE_CHECKING:
    from pathlib import Path

    from textual.app import ComposeResult

    from kagan.config import KaganConfig


HEADER_SEPARATOR = "│"


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
    """Header widget displaying logo, project, repo, git branch, and stats.

    Layout (with separators):
    ┃ ᘚᘛ  my-project / api │ ⎇ main │ ● 2 active │ 📋 12 tasks │ ? help ┃
    """

    task_count: reactive[int] = reactive(0)
    active_sessions: reactive[int] = reactive(0)
    git_branch: reactive[str] = reactive("")
    project_name: reactive[str] = reactive("")
    repo_name: reactive[str] = reactive("")
    agent_display: reactive[str] = reactive("")

    def __init__(self, task_count: int = 0, **kwargs) -> None:
        super().__init__(**kwargs)
        self.task_count = task_count

    def compose(self) -> ComposeResult:
        yield Label(KAGAN_LOGO_SMALL, classes="header-logo")
        yield Label("", id="header-project", classes="header-title")
        yield Label("", id="header-repo", classes="header-repo")

        yield Label("", classes="header-spacer")

        yield Label("", id="header-branch", classes="header-branch")
        yield Label(HEADER_SEPARATOR, id="sep-branch", classes="header-branch")
        yield Label("", id="header-sessions", classes="header-sessions")
        yield Label(HEADER_SEPARATOR, id="sep-sessions", classes="header-branch")
        yield Label("", id="header-agent", classes="header-agent")
        yield Label(HEADER_SEPARATOR, id="sep-agent", classes="header-branch")
        yield Label("", id="header-stats", classes="header-stats")
        yield Label(HEADER_SEPARATOR, id="sep-stats", classes="header-branch")
        yield Label("? help", id="header-help", classes="header-branch")

    def on_mount(self) -> None:
        """Initialize display state on mount."""
        self._update_project_display()
        self._update_repo_display()
        self._update_branch_display()
        self._update_sessions_display()
        self._update_agent_display()
        self._update_stats_display()

    def _update_project_display(self) -> None:
        """Update project name label."""
        if project_label := safe_query_one(self, "#header-project", Label):
            display_name = self.project_name if self.project_name else "KAGAN"
            project_label.update(display_name)

    def _update_branch_display(self) -> None:
        """Update git branch label and separator visibility."""
        branch_label = safe_query_one(self, "#header-branch", Label)
        sep_branch = safe_query_one(self, "#sep-branch", Label)
        if branch_label:
            if self.git_branch:
                branch_label.update(f"⎇ {self.git_branch}")
                branch_label.display = True
                if sep_branch:
                    sep_branch.display = True
            else:
                branch_label.update("")
                branch_label.display = False
                if sep_branch:
                    sep_branch.display = False

    def _update_repo_display(self) -> None:
        """Update repo name label visibility."""
        repo_label = safe_query_one(self, "#header-repo", Label)
        if repo_label:
            if self.repo_name:
                repo_label.update(f" / {self.repo_name}")
                repo_label.display = True
            else:
                repo_label.update("")
                repo_label.display = False

    def _update_sessions_display(self) -> None:
        """Update active sessions label and separator visibility."""
        sessions_label = safe_query_one(self, "#header-sessions", Label)
        sep_sessions = safe_query_one(self, "#sep-sessions", Label)
        if sessions_label:
            if self.active_sessions > 0:
                sessions_label.update(f"● {self.active_sessions} active")
                sessions_label.display = True
                if sep_sessions:
                    sep_sessions.display = True
            else:
                sessions_label.update("")
                sessions_label.display = False
                if sep_sessions:
                    sep_sessions.display = False

    def _update_stats_display(self) -> None:
        """Update task count label."""
        if stats_label := safe_query_one(self, "#header-stats", Label):
            stats_label.update(f"📋 {self.task_count} tasks")

    def _update_agent_display(self) -> None:
        """Update selected global agent label and separator visibility."""
        agent_label = safe_query_one(self, "#header-agent", Label)
        sep_agent = safe_query_one(self, "#sep-agent", Label)
        if agent_label:
            if self.agent_display:
                agent_label.update(self.agent_display)
                agent_label.display = True
                if sep_agent:
                    sep_agent.display = True
            else:
                agent_label.update("")
                agent_label.display = False
                if sep_agent:
                    sep_agent.display = False

    def watch_task_count(self, count: int) -> None:
        self._update_stats_display()

    def watch_active_sessions(self, count: int) -> None:
        self._update_sessions_display()

    def watch_git_branch(self, branch: str) -> None:
        self._update_branch_display()

    def watch_project_name(self, name: str) -> None:
        self._update_project_display()

    def watch_repo_name(self, name: str) -> None:
        self._update_repo_display()

    def watch_agent_display(self, value: str) -> None:
        self._update_agent_display()

    def update_count(self, count: int) -> None:
        self.task_count = count

    def update_sessions(self, active: int) -> None:
        self.active_sessions = active

    def update_branch(self, branch: str) -> None:
        self.git_branch = branch

    def update_project(self, name: str) -> None:
        """Update the displayed project name."""
        self.project_name = name

    def update_repo(self, name: str) -> None:
        """Update the displayed repo name."""
        self.repo_name = name

    def update_agent(self, display: str) -> None:
        """Update the displayed global agent label."""
        self.agent_display = display

    def update_agent_from_config(self, config: KaganConfig) -> None:
        """Build and update the global agent label from config."""
        short_name = config.general.default_worker_agent.strip()
        if not short_name:
            self.update_agent("")
            return

        builtin = get_builtin_agent(short_name)
        name = builtin.config.name.removesuffix(" Code") if builtin else short_name.capitalize()

        model = ""
        if short_name == "claude":
            model = config.general.default_model_claude or ""
        elif short_name == "opencode":
            model = config.general.default_model_opencode or ""

        model_suffix = f" ({model})" if model else ""
        self.update_agent(f"AI: {name}{model_suffix}")
