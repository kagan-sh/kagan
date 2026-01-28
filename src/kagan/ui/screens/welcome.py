"""Welcome screen for first-boot setup."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from textual.containers import Center, Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import Button, Footer, Label, Select

from kagan.agents.prompt_loader import dump_default_prompts
from kagan.data.builtin_agents import BUILTIN_AGENTS, list_builtin_agents
from kagan.git_utils import get_current_branch, has_git_repo, list_local_branches

if TYPE_CHECKING:
    from textual.app import ComposeResult

# Large block letter ASCII art logo
KAGAN_LOGO = """\
ᘚᘛ  ██╗  ██╗ █████╗  ██████╗  █████╗ ███╗   ██╗  ᘚᘛ
ᘚᘛ  ██║ ██╔╝██╔══██╗██╔════╝ ██╔══██╗████╗  ██║  ᘚᘛ
ᘚᘛ  █████╔╝ ███████║██║  ███╗███████║██╔██╗ ██║  ᘚᘛ
ᘚᘛ  ██╔═██╗ ██╔══██║██║   ██║██╔══██║██║╚██╗██║  ᘚᘛ
ᘚᘛ  ██║  ██╗██║  ██║╚██████╔╝██║  ██║██║ ╚████║  ᘚᘛ
ᘚᘛ  ╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═══╝  ᘚᘛ"""

DEFAULT_BASE_BRANCHES = ("main", "master", "develop", "trunk")
MCP_SETUP_COMMAND = "claude mcp add kagan -- kagan-mcp"


class WelcomeScreen(Screen):
    """First-boot welcome and configuration screen."""

    BINDINGS = [
        ("escape", "skip", "Continue"),
    ]

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

            # MCP setup section
            yield Label("─" * 50, classes="separator")
            yield Label(
                "i For full integration, run once in your terminal:",
                classes="info-label",
            )
            with Horizontal(id="mcp-command-row"):
                yield Label(MCP_SETUP_COMMAND, id="mcp-command", classes="command-text")
                yield Button("Copy Command", id="copy-command-btn", variant="default")
            yield Button("I'll do it later", id="skip-mcp-btn", variant="default")

            # Continue button
            with Center(id="buttons"):
                yield Button("Start Using Kagan", variant="primary", id="continue-btn")

            # Footer with key bindings
            yield Footer()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "continue-btn":
            self._save_and_continue()
        elif event.button.id == "copy-command-btn":
            self._copy_mcp_command()
        elif event.button.id == "skip-mcp-btn":
            # Just dismiss the info, continue with setup
            pass

    def action_skip(self) -> None:
        """Skip setup, use defaults (escape key)."""
        self._save_and_continue()

    def _copy_mcp_command(self) -> None:
        """Copy MCP setup command to clipboard."""
        # Try to copy using system clipboard commands
        import shutil
        import subprocess

        # Try xclip (Linux), pbcopy (macOS), or clip (Windows)
        if shutil.which("xclip"):
            try:
                subprocess.run(
                    ["xclip", "-selection", "clipboard"],
                    input=MCP_SETUP_COMMAND.encode(),
                    check=True,
                )
                self.notify("Command copied to clipboard!", severity="information")
                return
            except (subprocess.CalledProcessError, FileNotFoundError):
                pass
        elif shutil.which("pbcopy"):
            try:
                subprocess.run(
                    ["pbcopy"],
                    input=MCP_SETUP_COMMAND.encode(),
                    check=True,
                )
                self.notify("Command copied to clipboard!", severity="information")
                return
            except (subprocess.CalledProcessError, FileNotFoundError):
                pass
        elif shutil.which("clip"):
            try:
                subprocess.run(
                    ["clip"],
                    input=MCP_SETUP_COMMAND.encode(),
                    check=True,
                )
                self.notify("Command copied to clipboard!", severity="information")
                return
            except (subprocess.CalledProcessError, FileNotFoundError):
                pass

        # Fallback: show notification with command
        self.notify(
            f"Run this command: {MCP_SETUP_COMMAND}",
            severity="information",
            timeout=5.0,
        )

    def _save_and_continue(self) -> None:
        """Save configuration and continue."""
        base_branch_select = self.query_one("#base-branch-select", Select)
        base_branch = str(base_branch_select.value) if base_branch_select.value else "main"

        select = self.query_one("#agent-select", Select)
        agent = str(select.value) if select.value else "claude"
        worker = agent

        # No auto-start for session-first model
        auto_start = False
        auto_merge = False

        self._write_config(worker, auto_start, auto_merge, base_branch)
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
        auto_merge: bool,
        base_branch: str,
    ) -> None:
        """Write config.toml file with correct ACP run commands."""
        kagan_dir = Path(".kagan")
        kagan_dir.mkdir(exist_ok=True)

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
auto_merge = {str(auto_merge).lower()}
default_base_branch = "{base_branch}"
default_worker_agent = "{worker}"

{chr(10).join(agent_sections)}
'''

        (kagan_dir / "config.toml").write_text(config_content)

        # Dump default prompt templates for user customization
        dump_default_prompts(kagan_dir / "prompts")
