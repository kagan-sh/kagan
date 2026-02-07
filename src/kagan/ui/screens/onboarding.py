"""First-boot onboarding screen for Kagan."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, cast

from textual.containers import Container, Horizontal, Vertical
from textual.message import Message
from textual.screen import Screen
from textual.widgets import Button, Label, Select, Static, Switch

from kagan.builtin_agents import BUILTIN_AGENTS, list_builtin_agents
from kagan.config import GeneralConfig, KaganConfig
from kagan.keybindings import ONBOARDING_BINDINGS

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.app import KaganApp


# Base branch options for dropdown
BASE_BRANCH_OPTIONS = [
    ("main", "main"),
    ("master", "master"),
    ("develop", "develop"),
]


class OnboardingScreen(Screen):
    """First-boot onboarding screen shown when no config exists.

    Collects initial settings:
    - AI Assistant selection (Claude/OpenCode)
    - Default base branch (main/master/develop)
    - Auto Mode toggle (controls auto_start, auto_approve, auto_merge)

    On submit, creates config.toml and posts message to continue startup.
    """

    BINDINGS = ONBOARDING_BINDINGS

    @dataclass
    class Completed(Message):
        """Message posted when onboarding is complete."""

        config: KaganConfig

    def __init__(self) -> None:
        super().__init__()
        self._selected_agent: str = "claude"
        self._selected_branch: str = "main"
        self._auto_mode: bool = False

    @property
    def kagan_app(self) -> KaganApp:
        """Get the typed KaganApp instance."""
        return cast("KaganApp", self.app)

    def compose(self) -> ComposeResult:
        """Compose the onboarding screen UI."""
        # Build agent options from builtin_agents
        agent_options = [
            (agent.config.name, agent.config.short_name) for agent in list_builtin_agents()
        ]

        with Container(id="onboarding-container"):
            yield Static("Welcome to Kagan", id="onboarding-title")
            yield Label(
                "Let's configure your development cockpit.",
                id="onboarding-subtitle",
            )

            with Vertical(id="onboarding-form"):
                # AI Assistant Selection
                yield Label("AI Assistant", classes="form-label")
                yield Select(
                    options=agent_options,
                    value="claude",
                    id="agent-select",
                    allow_blank=False,
                )

                # Base Branch Selection
                yield Label("Default Base Branch", classes="form-label")
                yield Select(
                    options=BASE_BRANCH_OPTIONS,
                    value="main",
                    id="branch-select",
                    allow_blank=False,
                )

                # Auto Mode Toggle
                with Horizontal(id="auto-mode-row"):
                    with Vertical(id="auto-mode-info"):
                        yield Label("Auto Mode", classes="form-label")
                        yield Label(
                            "Automatically start, approve, and merge tasks",
                            classes="form-hint",
                        )
                    yield Switch(id="auto-mode-switch", value=False)

            # Continue Button
            with Horizontal(id="onboarding-actions"):
                yield Button(
                    "Continue to Kagan",
                    id="btn-continue",
                    variant="primary",
                )

    def on_select_changed(self, event: Select.Changed) -> None:
        """Handle dropdown selection changes."""
        if event.select.id == "agent-select" and event.value is not None:
            self._selected_agent = str(event.value)
        elif event.select.id == "branch-select" and event.value is not None:
            self._selected_branch = str(event.value)

    def on_switch_changed(self, event: Switch.Changed) -> None:
        """Handle switch toggle changes."""
        if event.switch.id == "auto-mode-switch":
            self._auto_mode = event.value

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "btn-continue":
            self.run_worker(self._save_and_continue())

    async def _save_and_continue(self) -> None:
        """Save configuration and signal completion."""
        # Build configuration
        config = KaganConfig(
            general=GeneralConfig(
                default_worker_agent=self._selected_agent,
                default_base_branch=self._selected_branch,
                auto_start=self._auto_mode,
                auto_approve=self._auto_mode,
                auto_merge=self._auto_mode,
            ),
            agents={name: agent.config for name, agent in BUILTIN_AGENTS.items()},
        )

        # Save to config file
        config_path = self.kagan_app.config_path
        config_path.parent.mkdir(parents=True, exist_ok=True)
        await config.save(config_path)

        self.app.notify("Configuration saved!", severity="information")

        # Post completion message to the app (not self) so it reaches KaganApp
        self.app.post_message(self.Completed(config=config))

    def action_quit(self) -> None:
        """Quit the application."""
        self.app.exit()
