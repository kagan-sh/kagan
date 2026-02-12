"""Full-screen description editor modal."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual import on
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Footer, Label, Static, TextArea

from kagan.tui.keybindings import DESCRIPTION_EDITOR_BINDINGS
from kagan.tui.ui.widgets.task_mentions import (
    TaskMentionArea,
    TaskMentionComplete,
    TaskMentionItem,
    handle_mention_completed,
    handle_mention_dismissed,
    handle_mention_key,
    handle_mention_query,
)

if TYPE_CHECKING:
    from textual.app import ComposeResult


class DescriptionEditorModal(ModalScreen[str | None]):
    """Full-screen modal for editing long descriptions."""

    BINDINGS = DESCRIPTION_EDITOR_BINDINGS

    def __init__(
        self,
        description: str = "",
        readonly: bool = False,
        title: str = "Edit Description",
        mention_items: list[TaskMentionItem] | None = None,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self.description = description
        self.readonly = readonly
        self.modal_title = title
        self._mention_items = list(mention_items or [])
        self._mention_complete: TaskMentionComplete | None = None

    def compose(self) -> ComposeResult:
        with Vertical(id="description-editor-container"):
            with Horizontal(id="description-editor-header"):
                yield Label(self.modal_title, id="editor-title")
                yield Static("", id="header-spacer")
                yield Static("[Esc] Cancel | [F2] Save", id="editor-hint")

            if self.readonly:
                yield TextArea(
                    self.description,
                    id="description-textarea",
                    show_line_numbers=True,
                    read_only=True,
                )
            else:
                yield TaskMentionArea(
                    self.description,
                    id="description-textarea",
                    show_line_numbers=True,
                )
                yield TaskMentionComplete(id="mention-complete")

            with Horizontal(id="description-editor-status"):
                yield Static("", id="cursor-position")
                yield Static("", id="status-spacer")
                yield Static("", id="line-count")

        yield Footer(show_command_palette=False)

    def on_mount(self) -> None:
        """Focus the textarea on mount and update status."""
        textarea = self.query_one("#description-textarea", TextArea)
        textarea.focus()
        self._update_status()
        if not self.readonly:
            self._mention_complete = self.query_one("#mention-complete", TaskMentionComplete)
            self._mention_complete.set_items(self._mention_items)

    def on_text_area_changed(self, event: TextArea.Changed) -> None:
        """Update status when text changes."""
        self._update_status()

    @on(TaskMentionArea.MentionQuery, "#description-textarea")
    def on_mention_query(self, event: TaskMentionArea.MentionQuery) -> None:
        if self.readonly:
            return
        handle_mention_query(self._ensure_mention_complete(), event.query)

    @on(TaskMentionArea.MentionDismissed, "#description-textarea")
    def on_mention_dismissed(self, event: TaskMentionArea.MentionDismissed) -> None:
        handle_mention_dismissed(self._mention_complete)

    @on(TaskMentionArea.MentionKey, "#description-textarea")
    def on_mention_key(self, event: TaskMentionArea.MentionKey) -> None:
        handle_mention_key(
            self._mention_complete,
            self.query_one("#description-textarea", TaskMentionArea),
            event.key,
        )

    @on(TaskMentionComplete.Completed)
    def on_mention_completed(self, event: TaskMentionComplete.Completed) -> None:
        handle_mention_completed(
            self._mention_complete,
            self.query_one("#description-textarea", TaskMentionArea),
            event.task_id,
        )

    def _update_status(self) -> None:
        """Update the status bar with cursor position and line count."""
        textarea = self.query_one("#description-textarea", TextArea)
        cursor_pos = self.query_one("#cursor-position", Static)
        line_count = self.query_one("#line-count", Static)

        row, col = textarea.cursor_location
        cursor_pos.update(f"Line {row + 1}, Col {col + 1}")

        lines = textarea.text.count("\n") + 1 if textarea.text else 0
        line_count.update(f"{lines}L")

    def _ensure_mention_complete(self) -> TaskMentionComplete:
        if self._mention_complete is None:
            self._mention_complete = self.query_one("#mention-complete", TaskMentionComplete)
            self._mention_complete.set_items(self._mention_items)
        return self._mention_complete

    def action_cancel(self) -> None:
        self.dismiss(None)

    def action_save(self) -> None:
        """Save and close the editor."""
        if self.readonly:
            self.dismiss(None)
        else:
            textarea = self.query_one("#description-textarea", TextArea)
            self.dismiss(textarea.text)

    def action_done(self) -> None:
        """Legacy: redirect to save."""
        self.action_save()
