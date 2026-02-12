"""Header widget for Kagan TUI."""

from __future__ import annotations

import asyncio
from dataclasses import dataclass
from importlib.metadata import PackageNotFoundError, version
from typing import TYPE_CHECKING

from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Label

from kagan.core.builtin_agents import get_builtin_agent
from kagan.core.constants import KAGAN_LOGO_SMALL
from kagan.tui.ui.utils import safe_query_one

if TYPE_CHECKING:
    from pathlib import Path

    from textual.app import ComposeResult

    from kagan.core.config import KaganConfig

_AGENT_MODEL_CONFIG_KEY: dict[str, str] = {
    "claude": "default_model_claude",
    "opencode": "default_model_opencode",
    "codex": "default_model_codex",
    "gemini": "default_model_gemini",
    "kimi": "default_model_kimi",
    "copilot": "default_model_copilot",
}


HEADER_SEPARATOR = "│"


@dataclass
class _HeaderLabels:
    project: Label
    repo: Label
    branch: Label
    sep_branch: Label
    sessions: Label
    sep_sessions: Label
    agent: Label
    sep_agent: Label
    core_status: Label
    sep_core_status: Label
    stats: Label


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
    core_status: reactive[str] = reactive("DISCONNECTED")

    def __init__(self, task_count: int = 0, **kwargs) -> None:
        super().__init__(**kwargs)
        self._labels: _HeaderLabels | None = None
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
        yield Label("", id="header-core-status", classes="core-status")
        yield Label(HEADER_SEPARATOR, id="sep-core-status", classes="header-branch")
        yield Label("", id="header-stats", classes="header-stats")
        yield Label(HEADER_SEPARATOR, id="sep-stats", classes="header-branch")
        yield Label("? help", id="header-help", classes="header-branch")

    def on_mount(self) -> None:
        """Initialize display state on mount."""
        self._cache_labels()
        self._update_project_display()
        self._update_repo_display()
        self._update_branch_display()
        self._update_sessions_display()
        self._update_agent_display()
        self._update_core_status_display()
        self._update_stats_display()

    def _cache_labels(self) -> _HeaderLabels | None:
        if self._labels is not None:
            return self._labels

        project_label = safe_query_one(self, "#header-project", Label)
        repo_label = safe_query_one(self, "#header-repo", Label)
        branch_label = safe_query_one(self, "#header-branch", Label)
        sep_branch = safe_query_one(self, "#sep-branch", Label)
        sessions_label = safe_query_one(self, "#header-sessions", Label)
        sep_sessions = safe_query_one(self, "#sep-sessions", Label)
        agent_label = safe_query_one(self, "#header-agent", Label)
        sep_agent = safe_query_one(self, "#sep-agent", Label)
        core_status_label = safe_query_one(self, "#header-core-status", Label)
        sep_core_status = safe_query_one(self, "#sep-core-status", Label)
        stats_label = safe_query_one(self, "#header-stats", Label)

        all_labels = (
            project_label,
            repo_label,
            branch_label,
            sep_branch,
            sessions_label,
            sep_sessions,
            agent_label,
            sep_agent,
            core_status_label,
            sep_core_status,
            stats_label,
        )
        if any(label is None for label in all_labels):
            return None

        assert project_label is not None
        assert repo_label is not None
        assert branch_label is not None
        assert sep_branch is not None
        assert sessions_label is not None
        assert sep_sessions is not None
        assert agent_label is not None
        assert sep_agent is not None
        assert core_status_label is not None
        assert sep_core_status is not None
        assert stats_label is not None

        self._labels = _HeaderLabels(
            project=project_label,
            repo=repo_label,
            branch=branch_label,
            sep_branch=sep_branch,
            sessions=sessions_label,
            sep_sessions=sep_sessions,
            agent=agent_label,
            sep_agent=sep_agent,
            core_status=core_status_label,
            sep_core_status=sep_core_status,
            stats=stats_label,
        )
        return self._labels

    def _update_project_display(self) -> None:
        """Update project name label."""
        labels = self._cache_labels()
        if labels is None:
            return
        display_name = self.project_name if self.project_name else "KAGAN"
        labels.project.update(display_name)

    def _update_branch_display(self) -> None:
        """Update git branch label and separator visibility."""
        labels = self._cache_labels()
        if labels is None:
            return
        if self.git_branch:
            labels.branch.update(f"⎇ {self.git_branch}")
            labels.branch.display = True
            labels.sep_branch.display = True
            return
        labels.branch.update("")
        labels.branch.display = False
        labels.sep_branch.display = False

    def _update_repo_display(self) -> None:
        """Update repo name label visibility."""
        labels = self._cache_labels()
        if labels is None:
            return
        if self.repo_name:
            labels.repo.update(f" / {self.repo_name}")
            labels.repo.display = True
            return
        labels.repo.update("")
        labels.repo.display = False

    def _update_sessions_display(self) -> None:
        """Update active sessions label and separator visibility."""
        labels = self._cache_labels()
        if labels is None:
            return
        if self.active_sessions > 0:
            labels.sessions.update(f"● {self.active_sessions} active")
            labels.sessions.display = True
            labels.sep_sessions.display = True
            return
        labels.sessions.update("")
        labels.sessions.display = False
        labels.sep_sessions.display = False

    def _update_stats_display(self) -> None:
        """Update task count label."""
        labels = self._cache_labels()
        if labels is None:
            return
        labels.stats.update(f"📋 {self.task_count} tasks")

    def _update_agent_display(self) -> None:
        """Update selected global agent label and separator visibility."""
        labels = self._cache_labels()
        if labels is None:
            return
        if self.agent_display:
            labels.agent.update(self.agent_display)
            labels.agent.display = True
            labels.sep_agent.display = True
            return
        labels.agent.update("")
        labels.agent.display = False
        labels.sep_agent.display = False

    def _update_core_status_display(self) -> None:
        """Update core connection status label and separator visibility."""
        labels = self._cache_labels()
        if labels is None:
            return
        labels.core_status.update(f"CORE:{self.core_status}")
        labels.core_status.display = True
        labels.sep_core_status.display = True

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

    def watch_core_status(self, value: str) -> None:
        self._update_core_status_display()

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

    def update_core_status(self, status: str) -> None:
        """Update the core connection status display."""
        self.core_status = status

    def update_agent_from_config(self, config: KaganConfig) -> None:
        """Build and update the global agent label from config."""
        short_name = config.general.default_worker_agent.strip()
        if not short_name:
            self.update_agent("")
            return

        builtin = get_builtin_agent(short_name)
        name = builtin.config.name.removesuffix(" Code") if builtin else short_name.capitalize()

        key = _AGENT_MODEL_CONFIG_KEY.get(short_name)
        model = getattr(config.general, key, "") if key else ""
        if not isinstance(model, str):
            model = ""

        model_suffix = f" ({model})" if model else ""
        self.update_agent(f"AI: {name}{model_suffix}")
