"""Welcome screen for first-boot setup."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, cast

from textual.containers import Center, Horizontal, Vertical
from textual.css.query import NoMatches
from textual.screen import Screen
from textual.widgets import Button, Footer, Label, Select, Switch

from kagan.constants import KAGAN_LOGO
from kagan.data.builtin_agents import (
    BUILTIN_AGENTS,
    get_all_agent_availability,
    list_builtin_agents,
)
from kagan.git_utils import get_current_branch, has_git_repo, list_local_branches
from kagan.keybindings import WELCOME_BINDINGS, to_textual_bindings

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.app import KaganApp

DEFAULT_BASE_BRANCHES = ("main", "master", "develop", "trunk")


class WelcomeScreen(Screen):
    """First-boot welcome and configuration screen."""

    BINDINGS = to_textual_bindings(WELCOME_BINDINGS)

    def __init__(self) -> None:
        super().__init__()
        self._agents = list_builtin_agents()
        self._agent_availability = get_all_agent_availability()
        self._repo_root = Path.cwd()
        # Git state - populated in on_mount
        self._has_git_repo: bool = False
        self._branches: list[str] = []
        self._default_base_branch: str = "main"
        self._branch_options: list[str] = list(DEFAULT_BASE_BRANCHES)

    def _build_branch_options(self, branches: list[str], default_branch: str) -> list[str]:
        options: list[str] = []
        for name in (default_branch, *branches, *DEFAULT_BASE_BRANCHES):
            if name not in options:
                options.append(name)
        return options

    async def _get_default_base_branch(self, branches: list[str]) -> str:
        if self._has_git_repo:
            current = await get_current_branch(self._repo_root)
            if current:
                return current
            for candidate in DEFAULT_BASE_BRANCHES:
                if candidate in branches:
                    return candidate
            if branches:
                return branches[0]
        return "main"

    def compose(self) -> ComposeResult:
        """Compose the welcome screen layout."""
        # Build Select options from agents with availability status
        # Pre-select first available agent (priority: claude > opencode)
        agent_options: list[tuple[str, str]] = []
        default_agent: str | None = None

        for avail in self._agent_availability:
            agent = avail.agent
            if avail.is_available:
                label = f"{agent.config.name} ({agent.author})"
                if default_agent is None:
                    default_agent = agent.config.short_name
            else:
                label = f"{agent.config.name} [Not Installed]"
            agent_options.append((label, agent.config.short_name))

        # Fallback to first agent if none available (shouldn't happen in normal flow)
        if default_agent is None and agent_options:
            default_agent = agent_options[0][1]

        # Final fallback to "claude" if somehow no options exist
        if default_agent is None:
            default_agent = "claude"

        # Initial branch options - will be updated in on_mount with git data
        base_branch_options = [(name, name) for name in self._branch_options]

        with Vertical(id="welcome-container"):
            # Large ASCII art logo
            yield Label(KAGAN_LOGO, id="logo")
            yield Label("Your Development Cockpit", id="subtitle")

            # AI Assistant selection
            yield Label(
                "AI Assistant:",
                classes="section-label",
            )
            yield Select(agent_options, value=default_agent, id="agent-select")

            # Base branch selection
            yield Label(
                "Base branch for worktrees:",
                classes="section-label",
            )
            yield Select(
                base_branch_options,
                value=self._default_base_branch,
                id="base-branch-select",
            )

            # Git init hint - hidden by default, shown in on_mount if no git repo
            yield Label(
                "No git repo detected. A fresh git repo will be initialized\n"
                "because Kagan requires git worktrees.",
                id="git-init-hint",
                classes="info-label hidden",
            )

            # AUTO Mode Settings section
            yield Label("Agent Settings:", classes="section-label settings-header")
            with Horizontal(classes="toggle-row"):
                yield Switch(value=False, id="auto-mode-switch")
                yield Label("Would you like to enable auto mode for agents?", classes="toggle-text")

            # Continue button
            with Center(id="buttons"):
                yield Button("Start Using Kagan", variant="primary", id="continue-btn")

            # Footer with key bindings
            yield Footer()

    async def on_mount(self) -> None:
        """Load git info and update UI after mount."""
        # Fetch git state asynchronously
        self._has_git_repo = await has_git_repo(self._repo_root)
        if self._has_git_repo:
            self._branches = await list_local_branches(self._repo_root)
        else:
            self._branches = []

        self._default_base_branch = await self._get_default_base_branch(self._branches)
        self._branch_options = self._build_branch_options(
            self._branches,
            self._default_base_branch,
        )

        # Update branch select with loaded options
        self._update_branch_select()

        # Show git init hint if no git repo
        if not self._has_git_repo:
            self._show_git_init_hint()

    def _update_branch_select(self) -> None:
        """Update branch select with loaded options."""
        try:
            select = self.query_one("#base-branch-select", Select)
            options = [(name, name) for name in self._branch_options]
            select.set_options(options)
            select.value = self._default_base_branch
        except NoMatches:
            pass

    def _show_git_init_hint(self) -> None:
        """Show the git init hint label."""
        try:
            hint = self.query_one("#git-init-hint", Label)
            hint.remove_class("hidden")
        except NoMatches:
            pass

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "continue-btn":
            self._save_and_continue()

    def action_skip(self) -> None:
        """Skip setup, use defaults (escape key)."""
        self._save_and_continue()

    def _save_and_continue(self) -> None:
        """Save configuration and continue."""
        base_branch_select = self.query_one("#base-branch-select", Select)
        base_branch = str(base_branch_select.value) if base_branch_select.value else "main"

        select = self.query_one("#agent-select", Select)
        agent = str(select.value) if select.value else "claude"
        worker = agent

        # Read toggle value
        auto_mode_switch = self.query_one("#auto-mode-switch", Switch)
        auto_mode = auto_mode_switch.value
        # Set all automation flags based on single toggle
        auto_start = auto_mode
        auto_approve = auto_mode
        auto_merge = auto_mode

        self._write_config(worker, auto_start, auto_approve, auto_merge, base_branch)
        self.app.pop_screen()
        self.app.call_later(self._notify_setup_complete)

    def _notify_setup_complete(self) -> None:
        """Notify app that setup is complete and it should continue mounting."""
        app = cast("KaganApp", self.app)
        app._continue_after_welcome()

    def _write_config(
        self,
        worker: str,
        auto_start: bool,
        auto_approve: bool,
        auto_merge: bool,
        base_branch: str,
    ) -> None:
        """Write config.toml file with correct ACP run commands."""
        kagan_dir = Path(".kagan")
        kagan_dir.mkdir(exist_ok=True)

        # Note: .gitignore handling is done in git_utils.init_git_repo()
        # which is called from app.py after welcome screen completes

        # Build agent sections from BUILTIN_AGENTS with correct ACP commands
        agent_sections = []
        for key, agent in BUILTIN_AGENTS.items():
            cfg = agent.config
            run_cmd = cfg.run_command.get("*", key)
            agent_sections.append(f'''[agents.{key}]
identity = "{cfg.identity}"
name = "{cfg.name}"
short_name = "{cfg.short_name}"
run_command."*" = "{run_cmd}"
active = true''')

        config_content = f'''# Kagan Configuration
# Generated by first-boot setup

[general]
auto_start = {str(auto_start).lower()}
auto_approve = {str(auto_approve).lower()}
auto_merge = {str(auto_merge).lower()}
default_base_branch = "{base_branch}"
default_worker_agent = "{worker}"

{chr(10).join(agent_sections)}
'''

        (kagan_dir / "config.toml").write_text(config_content)
