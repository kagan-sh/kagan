"""Settings modal for editing configuration."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Container, Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Input, Label, Rule, Select, Switch

from kagan.builtin_agents import BUILTIN_AGENTS
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

            yield Label("Auto Review", classes="section-title")
            with Horizontal(classes="setting-row"):
                yield Switch(
                    value=self._config.general.auto_review,
                    id="auto-review-switch",
                )
                yield Label("Enable auto review", classes="setting-label")

            yield Rule()

            yield Label("Planner Permissions", classes="section-title")
            with Horizontal(classes="setting-row"):
                yield Switch(
                    value=self._config.general.auto_approve,
                    id="auto-approve-switch",
                )
                yield Label(
                    "Auto-approve planner tool calls (workers always auto-approve)",
                    classes="setting-label",
                )

            yield Rule()

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
                yield Label("Default Agent", classes="input-label")
                agent_options: list[tuple[str, str]] = [
                    (agent.config.name, name) for name, agent in BUILTIN_AGENTS.items()
                ]
                yield Select[str](
                    options=agent_options,
                    value=self._config.general.default_worker_agent,
                    id="default-agent-select",
                    allow_blank=False,
                )
            with Vertical(classes="input-group"):
                yield Label("Default PAIR Terminal", classes="input-label")
                yield Select[str](
                    options=[
                        ("tmux", "tmux"),
                        ("VS Code", "vscode"),
                        ("Cursor", "cursor"),
                    ],
                    value=self._config.general.default_pair_terminal_backend,
                    id="default-pair-terminal-select",
                    allow_blank=False,
                )

            yield Rule()

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

            yield Label("UI Preferences", classes="section-title")
            with Horizontal(classes="setting-row"):
                yield Switch(
                    value=self._config.ui.skip_pair_instructions,
                    id="skip-pair-instructions-switch",
                )
                yield Label("Skip PAIR instructions popup", classes="setting-label")

            yield Rule()

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

    def action_save(self) -> None:
        """Save settings to config file."""
        auto_review = self.query_one("#auto-review-switch", Switch).value
        auto_approve = self.query_one("#auto-approve-switch", Switch).value
        require_review_approval = self.query_one("#require-review-approval-switch", Switch).value
        serialize_merges = self.query_one("#serialize-merges-switch", Switch).value
        skip_pair_instructions = self.query_one("#skip-pair-instructions-switch", Switch).value
        base_branch = self.query_one("#base-branch-input", Input).value
        max_agents_str = self.query_one("#max-agents-input", Input).value
        default_agent_select = self.query_one("#default-agent-select", Select)
        default_agent = str(default_agent_select.value) if default_agent_select.value else "claude"
        pair_terminal_select = self.query_one("#default-pair-terminal-select", Select)
        pair_terminal_backend = (
            str(pair_terminal_select.value)
            if pair_terminal_select.value in {"tmux", "vscode", "cursor"}
            else "tmux"
        )
        default_model_claude = self.query_one("#default-model-claude-input", Input).value
        default_model_claude = default_model_claude.strip() or None
        default_model_opencode = self.query_one("#default-model-opencode-input", Input).value
        default_model_opencode = default_model_opencode.strip() or None

        try:
            max_agents = int(max_agents_str) if max_agents_str else 3
        except ValueError:
            self.app.notify("Invalid numeric value", severity="error")
            return

        self._config.general.auto_review = auto_review
        self._config.general.auto_approve = auto_approve
        self._config.general.require_review_approval = require_review_approval
        self._config.general.serialize_merges = serialize_merges
        self._config.general.default_base_branch = base_branch
        self._config.general.max_concurrent_agents = max_agents
        self._config.general.default_worker_agent = default_agent
        self._config.general.default_pair_terminal_backend = pair_terminal_backend  # type: ignore[assignment]
        self._config.general.default_model_claude = default_model_claude
        self._config.general.default_model_opencode = default_model_opencode
        self._config.ui.skip_pair_instructions = skip_pair_instructions

        self.run_worker(self._write_config(), exclusive=True, exit_on_error=False)
        self.dismiss(True)

    async def _write_config(self) -> None:
        """Write config to TOML file."""
        import aiofiles

        from kagan.builtin_agents import BUILTIN_AGENTS

        kagan_dir = self._config_path.parent
        kagan_dir.mkdir(exist_ok=True)

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

        general_lines = [
            f"auto_review = {str(general.auto_review).lower()}",
            f"auto_approve = {str(general.auto_approve).lower()}",
            f"require_review_approval = {str(general.require_review_approval).lower()}",
            f"serialize_merges = {str(general.serialize_merges).lower()}",
            f'default_base_branch = "{general.default_base_branch}"',
            f'default_worker_agent = "{general.default_worker_agent}"',
            f'default_pair_terminal_backend = "{general.default_pair_terminal_backend}"',
            f"max_concurrent_agents = {general.max_concurrent_agents}",
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
skip_pair_instructions = {str(ui.skip_pair_instructions).lower()}

{chr(10).join(agent_sections)}
"""

        async with aiofiles.open(self._config_path, "w", encoding="utf-8") as f:
            await f.write(config_content)

    def action_cancel(self) -> None:
        """Cancel without saving."""
        self.dismiss(False)
