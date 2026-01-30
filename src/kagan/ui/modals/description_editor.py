"""Full-screen description editor modal."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Footer, Label, Static, TextArea

from kagan.keybindings import DESCRIPTION_EDITOR_BINDINGS, to_textual_bindings

if TYPE_CHECKING:
    from textual.app import ComposeResult


class DescriptionEditorModal(ModalScreen[str | None]):
    """Full-screen modal for editing long descriptions."""

    BINDINGS = to_textual_bindings(DESCRIPTION_EDITOR_BINDINGS)

    def __init__(
        self,
        description: str = "",
        readonly: bool = False,
        title: str = "Edit Description",
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self.description = description
        self.readonly = readonly
        self.modal_title = title

    def compose(self) -> ComposeResult:
        """Compose the full-screen editor layout."""
        with Vertical(id="description-editor-container"):
            with Horizontal(id="description-editor-header"):
                yield Label(self.modal_title, id="editor-title")
                yield Static("", id="header-spacer")
                yield Static("[Esc] Done", id="editor-hint")

            yield TextArea(
                self.description,
                id="description-textarea",
                show_line_numbers=True,
                read_only=self.readonly,
            )

            with Horizontal(id="description-editor-status"):
                yield Static("", id="cursor-position")
                yield Static("", id="status-spacer")
                yield Static("", id="line-count")

        yield Footer()

    def on_mount(self) -> None:
        """Focus the textarea on mount and update status."""
        textarea = self.query_one("#description-textarea", TextArea)
        textarea.focus()
        self._update_status()

    def on_text_area_changed(self, event: TextArea.Changed) -> None:
        """Update status when text changes."""
        self._update_status()

    def _update_status(self) -> None:
        """Update the status bar with cursor position and line count."""
        textarea = self.query_one("#description-textarea", TextArea)
        cursor_pos = self.query_one("#cursor-position", Static)
        line_count = self.query_one("#line-count", Static)

        row, col = textarea.cursor_location
        cursor_pos.update(f"Line {row + 1}, Col {col + 1}")

        lines = textarea.text.count("\n") + 1 if textarea.text else 0
        line_count.update(f"{lines}L")

    def action_done(self) -> None:
        """Save and close the editor."""
        if self.readonly:
            self.dismiss(None)
        else:
            textarea = self.query_one("#description-textarea", TextArea)
            self.dismiss(textarea.text)
