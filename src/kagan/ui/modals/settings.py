"""Settings modal for editing configuration."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Container, Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Input, Label, Rule, Switch

from kagan.keybindings import SETTINGS_BINDINGS

if TYPE_CHECKING:
    from pathlib import Path

    from textual.app import ComposeResult

    from kagan.config import KaganConfig


class SettingsModal(ModalScreen[bool]):
    """Modal for editing application settings."""

    BINDINGS = SETTINGS_BINDINGS

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
                yield Label("Auto-merge completed tasks", classes="setting-label")
            with Horizontal(classes="setting-row"):
                yield Switch(
                    value=self._config.general.auto_retry_on_merge_conflict,
                    id="auto-retry-merge-conflict-switch",
                    disabled=not self._config.general.auto_merge,
                )
                yield Label("Retry on merge conflict", classes="setting-label")

            yield Rule()

            # Merge Policy Section
            yield Label("Merge Policy", classes="section-title")
            with Horizontal(classes="setting-row"):
                yield Switch(
                    value=self._config.general.require_review_approval,
                    id="require-review-approval-switch",
                )
                yield Label("Require review approval before merge", classes="setting-label")
            with Horizontal(classes="setting-row"):
                yield Switch(
                    value=self._config.general.serialize_merges,
                    id="serialize-merges-switch",
                )
                yield Label("Serialize manual merges", classes="setting-label")

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
                yield Label("Max Iterations per Task", classes="input-label")
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

            # Model Defaults Section
            yield Label("Model Defaults", classes="section-title")
            with Vertical(classes="input-group"):
                yield Label("Default Claude Model", classes="input-label")
                yield Input(
                    value=self._config.general.default_model_claude or "",
                    id="default-model-claude-input",
                    placeholder="sonnet",
                )
            with Vertical(classes="input-group"):
                yield Label("Default OpenCode Model", classes="input-label")
                yield Input(
                    value=self._config.general.default_model_opencode or "",
                    id="default-model-opencode-input",
                    placeholder="anthropic/claude-sonnet-4-5",
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

        yield Footer(show_command_palette=False)

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "save-btn":
            self.action_save()
        elif event.button.id == "cancel-btn":
            self.action_cancel()

    def on_switch_changed(self, event: Switch.Changed) -> None:
        """Handle switch changes to update dependent settings."""
        if event.switch.id == "auto-merge-switch":
            # Enable/disable the retry switch based on auto-merge state
            retry_switch = self.query_one("#auto-retry-merge-conflict-switch", Switch)
            retry_switch.disabled = not event.value

    def action_save(self) -> None:
        """Save settings to config file."""
        # Read values from widgets
        auto_start = self.query_one("#auto-start-switch", Switch).value
        auto_approve = self.query_one("#auto-approve-switch", Switch).value
        auto_merge = self.query_one("#auto-merge-switch", Switch).value
        auto_retry_on_merge_conflict = self.query_one(
            "#auto-retry-merge-conflict-switch", Switch
        ).value
        require_review_approval = self.query_one("#require-review-approval-switch", Switch).value
        serialize_merges = self.query_one("#serialize-merges-switch", Switch).value
        skip_tmux_gateway = self.query_one("#skip-tmux-gateway-switch", Switch).value
        base_branch = self.query_one("#base-branch-input", Input).value
        max_agents_str = self.query_one("#max-agents-input", Input).value
        max_iterations_str = self.query_one("#max-iterations-input", Input).value
        iteration_delay_str = self.query_one("#iteration-delay-input", Input).value
        default_model_claude = self.query_one("#default-model-claude-input", Input).value
        default_model_claude = default_model_claude.strip() or None
        default_model_opencode = self.query_one("#default-model-opencode-input", Input).value
        default_model_opencode = default_model_opencode.strip() or None

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
        self._config.general.auto_retry_on_merge_conflict = auto_retry_on_merge_conflict
        self._config.general.require_review_approval = require_review_approval
        self._config.general.serialize_merges = serialize_merges
        self._config.general.default_base_branch = base_branch
        self._config.general.max_concurrent_agents = max_agents
        self._config.general.max_iterations = max_iterations
        self._config.general.iteration_delay_seconds = iteration_delay
        self._config.general.default_model_claude = default_model_claude
        self._config.general.default_model_opencode = default_model_opencode
        self._config.ui.skip_tmux_gateway = skip_tmux_gateway

        # Write to TOML file asynchronously
        self.run_worker(self._write_config(), exclusive=True, exit_on_error=False)
        self.dismiss(True)

    async def _write_config(self) -> None:
        """Write config to TOML file."""
        import aiofiles

        from kagan.builtin_agents import BUILTIN_AGENTS

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

        # Build model lines conditionally (only include if set)
        model_claude_line = (
            f'default_model_claude = "{general.default_model_claude}"'
            if general.default_model_claude
            else ""
        )
        model_opencode_line = (
            f'default_model_opencode = "{general.default_model_opencode}"'
            if general.default_model_opencode
            else ""
        )

        # Build the general section with optional model lines
        general_lines = [
            f"auto_start = {str(general.auto_start).lower()}",
            f"auto_approve = {str(general.auto_approve).lower()}",
            f"auto_merge = {str(general.auto_merge).lower()}",
            f"auto_retry_on_merge_conflict = {str(general.auto_retry_on_merge_conflict).lower()}",
            f"require_review_approval = {str(general.require_review_approval).lower()}",
            f"serialize_merges = {str(general.serialize_merges).lower()}",
            f'default_base_branch = "{general.default_base_branch}"',
            f'default_worker_agent = "{general.default_worker_agent}"',
            f"max_concurrent_agents = {general.max_concurrent_agents}",
            f"max_iterations = {general.max_iterations}",
            f"iteration_delay_seconds = {general.iteration_delay_seconds}",
        ]
        if model_claude_line:
            general_lines.append(model_claude_line)
        if model_opencode_line:
            general_lines.append(model_opencode_line)

        general_section = "\n".join(general_lines)

        config_content = f"""# Kagan Configuration

[general]
{general_section}

[ui]
skip_tmux_gateway = {str(ui.skip_tmux_gateway).lower()}

{chr(10).join(agent_sections)}
"""

        async with aiofiles.open(self._config_path, "w", encoding="utf-8") as f:
            await f.write(config_content)

    def action_cancel(self) -> None:
        """Cancel without saving."""
        self.dismiss(False)
