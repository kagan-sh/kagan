"""Modal for picking a folder to open as a project."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, DirectoryTree, Input, Label, Static

if TYPE_CHECKING:
    from textual.app import ComposeResult


class FolderPickerModal(ModalScreen[str | None]):
    """Modal for selecting a folder path to open as a project."""

    def compose(self) -> ComposeResult:
        with Vertical(id="dialog"):
            yield Static("Open Folder", classes="dialog-title")

            with Vertical(classes="field"):
                yield Label("Folder Path", classes="field-label")
                yield Input(
                    placeholder="Enter path to folder (e.g., ~/code/my-project)",
                    id="path-input",
                )
                yield Label(
                    "Enter an absolute path or path with ~ for home directory",
                    classes="hint",
                )

            with Vertical(classes="field"):
                yield Label("Browse", classes="field-label")
                yield DirectoryTree(Path.home(), id="folder-tree")

            with Horizontal(id="dialog-actions"):
                yield Button("Cancel", id="btn-cancel")
                yield Button("Open", id="btn-open", variant="primary")

    async def on_mount(self) -> None:
        """Focus the input on mount."""
        self.query_one("#path-input", Input).focus()

    def on_directory_tree_directory_selected(self, event: DirectoryTree.DirectorySelected) -> None:
        """Update the input when a directory is selected in the tree."""
        self.query_one("#path-input", Input).value = str(event.path)

    async def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "btn-cancel":
            self.dismiss(None)
        elif event.button.id == "btn-open":
            await self._open_folder()

    async def _open_folder(self) -> None:
        """Validate and return the folder path."""
        path = self.query_one("#path-input", Input).value.strip()

        if not path:
            self.notify("Folder path is required", severity="error")
            return

        resolved_path = Path(path).expanduser().resolve()
        if not resolved_path.exists():
            self.notify(f"Path does not exist: {resolved_path}", severity="error")
            return

        if not resolved_path.is_dir():
            self.notify(f"Path is not a directory: {resolved_path}", severity="error")
            return

        self.dismiss(str(resolved_path))
