"""Settings modal for editing configuration."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Container, Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Input, Label, Rule, Switch

from kagan.keybindings import SETTINGS_BINDINGS, to_textual_bindings

if TYPE_CHECKING:
    from pathlib import Path

    from textual.app import ComposeResult

    from kagan.config import KaganConfig


class SettingsModal(ModalScreen[bool]):
    """Modal for editing application settings."""

    BINDINGS = to_textual_bindings(SETTINGS_BINDINGS)

    def __init__(self, config: KaganConfig, config_path: Path, **kwargs) -> None:
        super().__init__(**kwargs)
        self._config = config
        self._config_path = config_path

    def compose(self) -> ComposeResult:
        with Container(id="settings-container"):
            yield Label("Settings", classes="modal-title")
            yield Rule()

            # Auto Mode Section
            yield Label("Auto Mode", classes="section-title")
            with Horizontal(classes="setting-row"):
                yield Switch(
                    value=self._config.general.auto_start,
                    id="auto-start-switch",
                )
                yield Label("Auto-start agents", classes="setting-label")
            with Horizontal(classes="setting-row"):
                yield Switch(
                    value=self._config.general.auto_approve,
                    id="auto-approve-switch",
                )
                yield Label("Auto-approve permissions", classes="setting-label")
            with Horizontal(classes="setting-row"):
                yield Switch(
                    value=self._config.general.auto_merge,
                    id="auto-merge-switch",
                )
                yield Label("Auto-merge completed tickets", classes="setting-label")

            yield Rule()

            # General Settings Section
            yield Label("General", classes="section-title")
            with Vertical(classes="input-group"):
                yield Label("Default Base Branch", classes="input-label")
                yield Input(
                    value=self._config.general.default_base_branch,
                    id="base-branch-input",
                    placeholder="main",
                )
            with Vertical(classes="input-group"):
                yield Label("Max Concurrent Agents", classes="input-label")
                yield Input(
                    value=str(self._config.general.max_concurrent_agents),
                    id="max-agents-input",
                    placeholder="3",
                    type="integer",
                )
            with Vertical(classes="input-group"):
                yield Label("Max Iterations per Ticket", classes="input-label")
                yield Input(
                    value=str(self._config.general.max_iterations),
                    id="max-iterations-input",
                    placeholder="10",
                    type="integer",
                )
            with Vertical(classes="input-group"):
                yield Label("Iteration Delay (seconds)", classes="input-label")
                yield Input(
                    value=str(self._config.general.iteration_delay_seconds),
                    id="iteration-delay-input",
                    placeholder="2.0",
                )

            yield Rule()

            # UI Preferences Section
            yield Label("UI Preferences", classes="section-title")
            with Horizontal(classes="setting-row"):
                yield Switch(
                    value=self._config.ui.skip_tmux_gateway,
                    id="skip-tmux-gateway-switch",
                )
                yield Label("Skip tmux info on session start", classes="setting-label")

            yield Rule()

            # Buttons
            with Horizontal(classes="button-row"):
                yield Button("Save", variant="primary", id="save-btn")
                yield Button("Cancel", variant="default", id="cancel-btn")

        yield Footer()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "save-btn":
            self.action_save()
        elif event.button.id == "cancel-btn":
            self.action_cancel()

    def action_save(self) -> None:
        """Save settings to config file."""
        # Read values from widgets
        auto_start = self.query_one("#auto-start-switch", Switch).value
        auto_approve = self.query_one("#auto-approve-switch", Switch).value
        auto_merge = self.query_one("#auto-merge-switch", Switch).value
        skip_tmux_gateway = self.query_one("#skip-tmux-gateway-switch", Switch).value
        base_branch = self.query_one("#base-branch-input", Input).value
        max_agents_str = self.query_one("#max-agents-input", Input).value
        max_iterations_str = self.query_one("#max-iterations-input", Input).value
        iteration_delay_str = self.query_one("#iteration-delay-input", Input).value

        # Parse numeric values with validation
        try:
            max_agents = int(max_agents_str) if max_agents_str else 3
            max_iterations = int(max_iterations_str) if max_iterations_str else 10
            iteration_delay = float(iteration_delay_str) if iteration_delay_str else 2.0
        except ValueError:
            self.app.notify("Invalid numeric value", severity="error")
            return

        # Update config object
        self._config.general.auto_start = auto_start
        self._config.general.auto_approve = auto_approve
        self._config.general.auto_merge = auto_merge
        self._config.general.default_base_branch = base_branch
        self._config.general.max_concurrent_agents = max_agents
        self._config.general.max_iterations = max_iterations
        self._config.general.iteration_delay_seconds = iteration_delay
        self._config.ui.skip_tmux_gateway = skip_tmux_gateway

        # Write to TOML file
        self._write_config()
        self.dismiss(True)

    def _write_config(self) -> None:
        """Write config to TOML file."""
        from kagan.data.builtin_agents import BUILTIN_AGENTS

        kagan_dir = self._config_path.parent
        kagan_dir.mkdir(exist_ok=True)

        # Build agent sections
        agent_sections = []
        for key, agent in BUILTIN_AGENTS.items():
            cfg = agent.config
            run_cmd = cfg.run_command.get("*", key)
            agent_sections.append(
                f'''[agents.{key}]
identity = "{cfg.identity}"
name = "{cfg.name}"
short_name = "{cfg.short_name}"
run_command."*" = "{run_cmd}"
active = true'''
            )

        general = self._config.general
        ui = self._config.ui
        config_content = f"""# Kagan Configuration

[general]
auto_start = {str(general.auto_start).lower()}
auto_approve = {str(general.auto_approve).lower()}
auto_merge = {str(general.auto_merge).lower()}
default_base_branch = "{general.default_base_branch}"
default_worker_agent = "{general.default_worker_agent}"
max_concurrent_agents = {general.max_concurrent_agents}
max_iterations = {general.max_iterations}
iteration_delay_seconds = {general.iteration_delay_seconds}

[ui]
skip_tmux_gateway = {str(ui.skip_tmux_gateway).lower()}

{chr(10).join(agent_sections)}
"""

        self._config_path.write_text(config_content)

    def action_cancel(self) -> None:
        """Cancel without saving."""
        self.dismiss(False)
