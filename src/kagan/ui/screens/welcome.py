"""Welcome screen for first-boot setup."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from textual.containers import Center, Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import Button, Footer, Label, Select, Switch

from kagan.constants import KAGAN_LOGO
from kagan.data.builtin_agents import BUILTIN_AGENTS, list_builtin_agents
from kagan.git_utils import get_current_branch, has_git_repo, list_local_branches
from kagan.keybindings import WELCOME_BINDINGS, to_textual_bindings

if TYPE_CHECKING:
    from textual.app import ComposeResult

DEFAULT_BASE_BRANCHES = ("main", "master", "develop", "trunk")


class WelcomeScreen(Screen):
    """First-boot welcome and configuration screen."""

    BINDINGS = to_textual_bindings(WELCOME_BINDINGS)

    def __init__(self) -> None:
        super().__init__()
        self._agents = list_builtin_agents()
        self._repo_root = Path.cwd()
        self._has_git_repo = has_git_repo(self._repo_root)
        self._branches = list_local_branches(self._repo_root) if self._has_git_repo else []
        self._default_base_branch = self._get_default_base_branch(self._branches)
        self._branch_options = self._build_branch_options(
            self._branches,
            self._default_base_branch,
        )

    def _build_branch_options(self, branches: list[str], default_branch: str) -> list[str]:
        options: list[str] = []
        for name in (default_branch, *branches, *DEFAULT_BASE_BRANCHES):
            if name not in options:
                options.append(name)
        return options

    def _get_default_base_branch(self, branches: list[str]) -> str:
        if self._has_git_repo:
            current = get_current_branch(self._repo_root)
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
        # Build Select options from agents
        agent_options = [
            (f"{a.config.name} ({a.author})", a.config.short_name) for a in self._agents
        ]
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
            yield Select(agent_options, value="claude", id="agent-select")

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

            if not self._has_git_repo:
                yield Label(
                    "No git repo detected. A fresh git repo will be initialized\n"
                    "because Kagan requires git worktrees.",
                    id="git-init-hint",
                    classes="info-label",
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
        if hasattr(self.app, "_continue_after_welcome"):
            self.app._continue_after_welcome()

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

        # Ensure .kagan is gitignored in git repos
        if self._has_git_repo:
            added = self._ensure_gitignored()
            if added:
                self.app.call_later(lambda: self.app.notify("Added .kagan/ to .gitignore"))

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

    def _ensure_gitignored(self) -> bool:
        """Add .kagan/ to .gitignore if not already present.

        Returns True if .gitignore was modified, False otherwise.
        """
        gitignore = Path(".gitignore")

        if gitignore.exists():
            content = gitignore.read_text()
            lines = content.split("\n")
            # Check if already ignored (with or without trailing slash)
            if ".kagan" in lines or ".kagan/" in lines:
                return False
            # Append to existing file
            if not content.endswith("\n"):
                content += "\n"
            content += "\n# Kagan local state\n.kagan/\n"
            gitignore.write_text(content)
        else:
            # Create new .gitignore
            gitignore.write_text("# Kagan local state\n.kagan/\n")

        return True
