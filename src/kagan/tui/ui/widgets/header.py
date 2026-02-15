"""Header widget for Kagan TUI."""

from __future__ import annotations

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
    from textual.app import ComposeResult

    from kagan.core.config import KaganConfig


# GitHub sync status display constants
GITHUB_ICON_SYNCED = "◉"  # Filled circle indicates sync complete
GITHUB_ICON_PENDING = "○"  # Empty circle indicates sync available

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
    logo: Label
    project: Label
    repo: Label
    github_status: Label
    sep_github: Label
    branch: Label
    sep_branch: Label
    sessions: Label
    sep_sessions: Label
    agent: Label
    sep_agent: Label
    stats: Label


def _get_version() -> str:
    """Get package version, fallback to dev."""
    try:
        return version("kagan")
    except PackageNotFoundError:
        return "dev"


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
    github_connected: reactive[bool] = reactive(False)
    github_synced: reactive[bool] = reactive(False)
    plugin_badges_text: reactive[str] = reactive("")

    def __init__(self, task_count: int = 0, **kwargs) -> None:
        super().__init__(**kwargs)
        self._labels: _HeaderLabels | None = None
        self.task_count = task_count

    def compose(self) -> ComposeResult:
        yield Label(KAGAN_LOGO_SMALL, id="header-logo", classes="header-logo")
        yield Label("", id="header-project", classes="header-title")
        yield Label("", id="header-repo", classes="header-repo")

        yield Label("", classes="header-spacer")

        yield Label("", id="header-github-status", classes="header-github-status")
        yield Label(HEADER_SEPARATOR, id="sep-github", classes="header-branch")
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
        self._cache_labels()
        self._update_project_display()
        self._update_repo_display()
        self._update_github_status_display()
        self._update_branch_display()
        self._update_sessions_display()
        self._update_agent_display()
        self._update_core_status_display()
        self._update_stats_display()

    def _cache_labels(self) -> _HeaderLabels | None:
        if self._labels is not None:
            return self._labels

        logo_label = safe_query_one(self, "#header-logo", Label)
        project_label = safe_query_one(self, "#header-project", Label)
        repo_label = safe_query_one(self, "#header-repo", Label)
        github_status_label = safe_query_one(self, "#header-github-status", Label)
        sep_github = safe_query_one(self, "#sep-github", Label)
        branch_label = safe_query_one(self, "#header-branch", Label)
        sep_branch = safe_query_one(self, "#sep-branch", Label)
        sessions_label = safe_query_one(self, "#header-sessions", Label)
        sep_sessions = safe_query_one(self, "#sep-sessions", Label)
        agent_label = safe_query_one(self, "#header-agent", Label)
        sep_agent = safe_query_one(self, "#sep-agent", Label)
        stats_label = safe_query_one(self, "#header-stats", Label)

        all_labels = (
            logo_label,
            project_label,
            repo_label,
            github_status_label,
            sep_github,
            branch_label,
            sep_branch,
            sessions_label,
            sep_sessions,
            agent_label,
            sep_agent,
            stats_label,
        )
        if any(label is None for label in all_labels):
            return None

        assert logo_label is not None
        assert project_label is not None
        assert repo_label is not None
        assert github_status_label is not None
        assert sep_github is not None
        assert branch_label is not None
        assert sep_branch is not None
        assert sessions_label is not None
        assert sep_sessions is not None
        assert agent_label is not None
        assert sep_agent is not None
        assert stats_label is not None

        self._labels = _HeaderLabels(
            logo=logo_label,
            project=project_label,
            repo=repo_label,
            github_status=github_status_label,
            sep_github=sep_github,
            branch=branch_label,
            sep_branch=sep_branch,
            sessions=sessions_label,
            sep_sessions=sep_sessions,
            agent=agent_label,
            sep_agent=sep_agent,
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

    def _update_github_status_display(self) -> None:
        """Update GitHub sync status label and separator visibility."""
        labels = self._cache_labels()
        if labels is None:
            return
        if self.plugin_badges_text:
            labels.github_status.update(self.plugin_badges_text)
            labels.github_status.set_class(False, "github-synced")
            labels.github_status.set_class(False, "github-pending")
            labels.github_status.display = True
            labels.sep_github.display = True
            return
        if not self.github_connected:
            labels.github_status.update("")
            labels.github_status.display = False
            labels.sep_github.display = False
            return

        # Show GitHub status with appropriate icon
        if self.github_synced:
            labels.github_status.update(f"{GITHUB_ICON_SYNCED} GitHub")
            labels.github_status.set_class(True, "github-synced")
            labels.github_status.set_class(False, "github-pending")
        else:
            labels.github_status.update(f"{GITHUB_ICON_PENDING} GitHub")
            labels.github_status.set_class(False, "github-synced")
            labels.github_status.set_class(True, "github-pending")
        labels.github_status.display = True
        labels.sep_github.display = True

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
        """Update logo color based on core connection status."""
        labels = self._cache_labels()
        if labels is None:
            return
        if self.core_status == "CONNECTED":
            labels.logo.set_class(False, "logo-disconnected")
            labels.logo.set_class(True, "logo-connected")
        else:
            labels.logo.set_class(False, "logo-connected")
            labels.logo.set_class(True, "logo-disconnected")

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

    def watch_github_connected(self, value: bool) -> None:
        self._update_github_status_display()

    def watch_github_synced(self, value: bool) -> None:
        self._update_github_status_display()

    def watch_plugin_badges_text(self, value: str) -> None:
        self._update_github_status_display()

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

    def update_github_status(self, *, connected: bool, synced: bool = False) -> None:
        """Update the GitHub connection and sync status display.

        Args:
            connected: Whether the repo is connected to GitHub.
            synced: Whether issues have been synced (only relevant if connected).
        """
        self.github_connected = connected
        self.github_synced = synced if connected else False

    def update_plugin_badges(self, badges: list[dict] | None) -> None:
        """Update the declarative plugin badge display (schema-driven UI)."""
        if not badges:
            self.plugin_badges_text = ""
            return

        parts: list[str] = []
        for badge in badges:
            if not isinstance(badge, dict):
                continue
            label = badge.get("label")
            text = badge.get("text")
            state = badge.get("state")
            if not isinstance(label, str) or not label.strip():
                continue
            label = label.strip()
            text = text.strip() if isinstance(text, str) else ""
            icon = "○"
            if state == "ok":
                icon = "◉"
            elif state == "error":
                icon = "⊗"
            part = f"{icon} {label}" if not text else f"{icon} {label} {text}"
            parts.append(part)

        self.plugin_badges_text = "  ".join(parts)

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
