"""Modal for agent permission requests."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, cast

from textual import on
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Label, Static

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.acp import protocol

from kagan.acp.messages import Answer


class PermissionModal(ModalScreen[Answer | None]):
    """Modal for agent permission requests.

    Displayed when an agent needs permission for a potentially dangerous
    operation like writing files or running commands.
    """

    BINDINGS = [
        Binding("escape", "cancel", "Cancel"),
    ]

    def __init__(
        self,
        options: list[protocol.PermissionOption],
        tool_call: protocol.ToolCallUpdatePermissionRequest | protocol.ToolCall,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self.options = options
        self.tool_call = tool_call

    def compose(self) -> ComposeResult:
        with Vertical(id="permission-container"):
            yield Label("Permission Request", classes="modal-title")
            yield Static(self._describe_request(), id="request-description")
            yield Static(self._tool_details(), id="tool-details")
            with Vertical(id="options-container"):
                for opt in self.options:
                    btn_class = self._get_button_class(opt["kind"])
                    yield Button(
                        opt["name"],
                        id=f"opt-{opt['optionId']}",
                        classes=btn_class,
                    )
            with Horizontal(classes="button-row"):
                yield Button("Cancel", id="cancel-btn", variant="default")

    def _describe_request(self) -> str:
        """Create description of the permission request."""
        title = self.tool_call.get("title", "Unknown operation")
        return f"The agent wants to: [bold]{title}[/bold]"

    def _tool_details(self) -> str:
        """Extract and format tool call details."""
        details: list[str] = []
        # Cast to Any for dynamic access since TypedDict may have extra fields at runtime
        tool_call = cast("dict[str, Any]", self.tool_call)

        if kind := tool_call.get("kind"):
            details.append(f"Type: {kind}")

        status = tool_call.get("status")
        if status:
            details.append(f"Status: {status}")

        locations = tool_call.get("locations")
        if locations and isinstance(locations, list):
            paths = [loc.get("path", "") for loc in locations if isinstance(loc, dict)]
            paths = [p for p in paths if p]
            if paths:
                details.append(f"Files: {', '.join(paths[:3])}")
                if len(paths) > 3:
                    details.append(f"  ... and {len(paths) - 3} more")

        raw_input = tool_call.get("rawInput")
        if raw_input and isinstance(raw_input, dict):
            # Show command if present
            if cmd := raw_input.get("command"):
                details.append(f"Command: {cmd}")
            # Show path if present
            if path := raw_input.get("path"):
                details.append(f"Path: {path}")

        return "\n".join(details) if details else ""

    def _get_button_class(self, kind: str) -> str:
        """Get CSS class for permission option button."""
        if "allow" in kind:
            return "permission-allow"
        elif "reject" in kind:
            return "permission-reject"
        return ""

    @on(Button.Pressed)
    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        button_id = event.button.id
        if button_id == "cancel-btn":
            self.dismiss(None)
        elif button_id and button_id.startswith("opt-"):
            option_id = button_id[4:]  # Remove "opt-" prefix
            self.dismiss(Answer(option_id))

    def action_cancel(self) -> None:
        """Cancel the permission request."""
        self.dismiss(None)
