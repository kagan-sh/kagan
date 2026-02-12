"""First-boot onboarding screen for Kagan."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from textual.containers import Container, Horizontal, Vertical
from textual.message import Message
from textual.screen import Screen
from textual.widgets import Button, Label, Rule, Select, Static, Switch

from kagan.core.builtin_agents import BUILTIN_AGENTS, list_builtin_agents
from kagan.core.config import GeneralConfig, KaganConfig
from kagan.core.constants import KAGAN_LOGO
from kagan.tui.keybindings import ONBOARDING_BINDINGS

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.tui.app import KaganApp


class OnboardingScreen(Screen):
    """First-boot onboarding screen shown when no config exists.

    Collects initial settings:
    - AI Assistant selection (Claude/OpenCode)
    - Auto review toggle

    On submit, creates config.toml and posts message to continue startup.
    Base branch is not collected here; it is determined per-project from
    the actual repository branches when linking a repo.
    """

    BINDINGS = ONBOARDING_BINDINGS

    @dataclass
    class Completed(Message):
        """Message posted when onboarding is complete."""

        config: KaganConfig

    def __init__(self) -> None:
        super().__init__()
        self._selected_agent: str = "claude"
        self._auto_review: bool = True

    @property
    def kagan_app(self) -> KaganApp:
        """Get the typed KaganApp instance."""
        from kagan.tui.app import KaganApp

        app = self.app
        if not isinstance(app, KaganApp):
            msg = "Kagan app context is unavailable for onboarding."
            raise RuntimeError(msg)
        return app

    def compose(self) -> ComposeResult:
        agent_options = [
            (agent.config.name, agent.config.short_name) for agent in list_builtin_agents()
        ]

        with Container(id="onboarding-container"):
            yield Static(KAGAN_LOGO, id="onboarding-logo")
            yield Label("First-Time Setup", id="onboarding-subtitle")
            yield Rule(id="onboarding-divider")

            with Vertical(id="onboarding-form"):
                yield Label("AI Assistant", classes="form-label")
                yield Select(
                    options=agent_options,
                    value="claude",
                    id="agent-select",
                    allow_blank=False,
                )

                with Horizontal(id="auto-review-row"):
                    with Vertical(id="auto-review-info"):
                        yield Label("Enable auto review", classes="form-label")
                        yield Label(
                            "Automatically run AI review when tasks complete",
                            classes="form-hint",
                        )
                    yield Switch(id="auto-review-switch", value=self._auto_review)

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

    def on_switch_changed(self, event: Switch.Changed) -> None:
        """Handle switch toggle changes."""
        if event.switch.id == "auto-review-switch":
            self._auto_review = event.value

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "btn-continue":
            self.run_worker(self._save_and_continue())

    async def _save_and_continue(self) -> None:
        """Save configuration and signal completion."""
        config = KaganConfig(
            general=GeneralConfig(
                default_worker_agent=self._selected_agent,
                auto_review=self._auto_review,
            ),
            agents={name: agent.config for name, agent in BUILTIN_AGENTS.items()},
        )

        config_path = self.kagan_app.config_path
        config_path.parent.mkdir(parents=True, exist_ok=True)
        await config.save(config_path)

        self.app.notify("Configuration saved!", severity="information")

        self.app.post_message(self.Completed(config=config))

    def action_quit(self) -> None:
        """Quit the application."""
        self.app.exit()
