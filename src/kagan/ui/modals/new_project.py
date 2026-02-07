"""Modal for creating a new project."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, cast

from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label, Static

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.app import KaganApp


class NewProjectModal(ModalScreen[dict | None]):
    """Modal for creating a new project."""

    def compose(self) -> ComposeResult:
        with Vertical(id="dialog"):
            yield Static("New Project", classes="dialog-title")

            with Vertical(classes="field"):
                yield Label("Project Name", classes="field-label")
                yield Input(placeholder="My Awesome Project", id="name-input")

            with Vertical(classes="field"):
                yield Label("Repository Path (optional)", classes="field-label")
                yield Input(placeholder="/path/to/repo", id="path-input")
                yield Label("Leave blank to add repos later", classes="hint")

            with Horizontal(id="dialog-actions"):
                yield Button("Cancel", id="btn-cancel")
                yield Button("Create", id="btn-create", variant="primary")

    async def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-cancel":
            self.dismiss(None)
        elif event.button.id == "btn-create":
            await self._create_project()

    async def _create_project(self) -> None:
        name = self.query_one("#name-input", Input).value.strip()
        path = self.query_one("#path-input", Input).value.strip()

        if not name:
            self.notify("Project name is required", severity="error")
            return

        resolved_path: Path | None = None
        if path:
            resolved_path = Path(path).expanduser().resolve()
            if not resolved_path.exists():
                self.notify(f"Path does not exist: {resolved_path}", severity="error")
                return

            if not (resolved_path / ".git").exists():
                self.notify(f"Not a git repository: {resolved_path}", severity="warning")
                # Allow creation anyway - they might want to init later

        # Create the project via service
        app = cast("KaganApp", self.app)
        project_service = app.ctx.project_service
        repo_paths: list[str | Path] = []
        if resolved_path is not None:
            repo_paths.append(str(resolved_path))
        project_id = await project_service.create_project(name=name, repo_paths=repo_paths)

        self.dismiss({"project_id": project_id})
