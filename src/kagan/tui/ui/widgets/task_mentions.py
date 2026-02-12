"""Task mention autocomplete widgets for description fields."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from textual.containers import VerticalGroup
from textual.css.query import NoMatches
from textual.message import Message
from textual.reactive import var
from textual.widgets import OptionList, TextArea
from textual.widgets.option_list import Option

if TYPE_CHECKING:
    from textual import events
    from textual.app import ComposeResult


@dataclass(frozen=True)
class TaskMentionItem:
    """Display metadata for a single mentionable task."""

    task_id: str
    title: str
    status: str

    def format_label(self, max_title_len: int = 64) -> str:
        """Format a compact option label for dropdown rendering."""
        title = self.title.strip()
        if len(title) > max_title_len:
            title = title[: max_title_len - 3] + "..."
        return f"{self.task_id}  [{self.status}]  {title}"


def handle_mention_query(
    mention_complete: TaskMentionComplete,
    query: str,
) -> None:
    """Filter and show/hide the mention dropdown based on *query*."""
    count = mention_complete.filter(query)
    if count > 0:
        mention_complete.show()
    else:
        mention_complete.hide()


def handle_mention_key(
    mention_complete: TaskMentionComplete | None,
    textarea: TaskMentionArea,
    key: str,
) -> None:
    """Dispatch a navigation/selection key to the mention dropdown."""
    if mention_complete is None or not mention_complete.is_visible:
        return
    if key == "up":
        mention_complete.action_cursor_up()
    elif key == "down":
        mention_complete.action_cursor_down()
    elif key == "enter":
        mention_complete.action_select()
    elif key == "escape":
        mention_complete.hide()
        textarea.cancel_mention()


def handle_mention_completed(
    mention_complete: TaskMentionComplete | None,
    textarea: TaskMentionArea,
    task_id: str,
) -> None:
    """Apply a completed mention and hide the dropdown."""
    textarea.apply_mention(task_id)
    if mention_complete is not None:
        mention_complete.hide()


def handle_mention_dismissed(mention_complete: TaskMentionComplete | None) -> None:
    """Hide the mention dropdown on dismiss."""
    if mention_complete is not None:
        mention_complete.hide()


class TaskMentionComplete(VerticalGroup):
    """Autocomplete dropdown for task mentions."""

    DEFAULT_CLASSES = "task-mention-complete"

    mention_items: var[list[TaskMentionItem]] = var(list, init=False)

    @dataclass
    class Completed(Message):
        """Message emitted when the user confirms an item selection."""

        task_id: str

    def __init__(
        self,
        *,
        id: str | None = None,
        classes: str | None = None,
    ) -> None:
        """Initialize mention dropdown state."""
        super().__init__(id=id, classes=classes)
        self._items: list[TaskMentionItem] = []
        self._filtered: list[TaskMentionItem] = []
        self._query: str = ""
        self._has_matches: bool = False

    def compose(self) -> ComposeResult:
        """Render the option list used for autocomplete results."""
        yield OptionList(id="mention-options")

    def on_mount(self) -> None:
        """Populate options on mount to keep UI state consistent."""
        self._rebuild_options()

    def set_items(self, items: list[TaskMentionItem]) -> None:
        """Replace candidate items and re-apply the active filter."""
        self._items = list(items)
        self.filter(self._query)

    def filter(self, query: str) -> int:
        """Filter items by task id/title and return the match count."""
        self._query = query
        if not query:
            self._filtered = list(self._items)
        else:
            needle = query.lower()
            self._filtered = [
                item
                for item in self._items
                if needle in item.task_id.lower() or needle in item.title.lower()
            ]
        self._rebuild_options()
        return len(self._filtered)

    def _rebuild_options(self) -> None:
        try:
            option_list = self.query_one("#mention-options", OptionList)
        except NoMatches:
            return
        option_list.clear_options()
        self._has_matches = bool(self._filtered)

        if not self._filtered:
            option_list.add_option(Option("No matches", id="no-matches"))
            option_list.highlighted = 0
            return

        for item in self._filtered:
            option_list.add_option(Option(item.format_label(), id=item.task_id))

        option_list.highlighted = 0

    def action_cursor_up(self) -> None:
        """Move dropdown highlight to the previous option."""
        self.query_one("#mention-options", OptionList).action_cursor_up()

    def action_cursor_down(self) -> None:
        """Move dropdown highlight to the next option."""
        self.query_one("#mention-options", OptionList).action_cursor_down()

    def action_select(self) -> None:
        """Emit completion message for the currently highlighted item."""
        if not self._has_matches:
            return
        option_list = self.query_one("#mention-options", OptionList)
        if option_list.highlighted is None:
            return
        idx = option_list.highlighted
        if 0 <= idx < len(self._filtered):
            self.post_message(self.Completed(task_id=self._filtered[idx].task_id))

    def show(self) -> None:
        """Make the autocomplete dropdown visible."""
        self.add_class("visible")

    def hide(self) -> None:
        """Hide the autocomplete dropdown."""
        self.remove_class("visible")

    @property
    def is_visible(self) -> bool:
        """Return whether the dropdown is currently visible."""
        return self.has_class("visible")


class TaskMentionArea(TextArea):
    """TextArea that emits task mention autocomplete events."""

    @dataclass
    class MentionQuery(Message):
        """Message posted when an active mention query is detected."""

        query: str
        start_index: int
        end_index: int

        @property
        def control(self) -> TaskMentionArea:
            """Return the emitting TaskMentionArea instance."""
            sender = self._sender
            if isinstance(sender, TaskMentionArea):
                return sender
            msg = "Mention query sender is not a TaskMentionArea."
            raise RuntimeError(msg)

    @dataclass
    class MentionDismissed(Message):
        """Message posted when mention mode is exited."""

        @property
        def control(self) -> TaskMentionArea:
            """Return the emitting TaskMentionArea instance."""
            sender = self._sender
            if isinstance(sender, TaskMentionArea):
                return sender
            msg = "Mention dismissed sender is not a TaskMentionArea."
            raise RuntimeError(msg)

    @dataclass
    class MentionKey(Message):
        """Message posted for mention-specific navigation keys."""

        key: str

        @property
        def control(self) -> TaskMentionArea:
            """Return the emitting TaskMentionArea instance."""
            sender = self._sender
            if isinstance(sender, TaskMentionArea):
                return sender
            msg = "Mention key sender is not a TaskMentionArea."
            raise RuntimeError(msg)

    def __init__(self, *args, **kwargs) -> None:
        """Initialize mention-tracking state for text editing."""
        super().__init__(*args, **kwargs)
        self._mention_active = False
        self._mention_range: tuple[int, int] | None = None
        self._mention_query = ""
        self._suppress_mentions = False

    async def _on_key(self, event: events.Key) -> None:
        if self._mention_active and event.key in ("up", "down", "enter", "escape"):
            event.prevent_default()
            event.stop()
            self.post_message(self.MentionKey(event.key))
            return
        await super()._on_key(event)

    def on_text_area_changed(self, event: TextArea.Changed) -> None:
        """Track mention boundaries and emit query/dismiss events."""
        if self._suppress_mentions:
            return
        mention = self._find_active_mention()
        if mention is None:
            if self._mention_active:
                self._mention_active = False
                self._mention_range = None
                self._mention_query = ""
                self.post_message(self.MentionDismissed())
            return

        start, end, query = mention
        self._mention_active = True
        if self._mention_range != (start, end) or self._mention_query != query:
            self._mention_range = (start, end)
            self._mention_query = query
            self.post_message(self.MentionQuery(query=query, start_index=start, end_index=end))

    def apply_mention(self, task_id: str) -> None:
        """Replace active mention token with a concrete `@task_id` value."""
        if not self._mention_range:
            return
        start, end = self._mention_range
        insertion = f"@{task_id} "
        new_text = f"{self.text[:start]}{insertion}{self.text[end:]}"
        self._suppress_mentions = True
        self.text = new_text
        self._suppress_mentions = False
        self.cancel_mention()
        cursor_index = start + len(insertion)
        self.cursor_location = self._index_to_location(new_text, cursor_index)

    def cancel_mention(self) -> None:
        """Clear mention state without modifying editor text."""
        self._mention_active = False
        self._mention_range = None
        self._mention_query = ""

    def _find_active_mention(self) -> tuple[int, int, str] | None:
        text = self.text
        cursor_index = self._cursor_index(text)
        if cursor_index < 0:
            return None

        line_start = text.rfind("\n", 0, cursor_index) + 1
        segment = text[line_start:cursor_index]
        at_offset = segment.rfind("@")
        if at_offset == -1:
            return None

        at_index = line_start + at_offset
        if at_index > 0:
            prev = text[at_index - 1]
            if not (prev.isspace() or prev in "([{"):
                return None

        query = text[at_index + 1 : cursor_index]
        if any(ch.isspace() for ch in query):
            return None
        if query and not query.isalnum():
            return None
        return (at_index, cursor_index, query)

    def _cursor_index(self, text: str) -> int:
        row, col = self.cursor_location
        lines = text.split("\n")
        if row < 0 or row >= len(lines):
            return -1
        index = 0
        for line in lines[:row]:
            index += len(line) + 1
        return index + min(col, len(lines[row]))

    def _index_to_location(self, text: str, index: int) -> tuple[int, int]:
        lines = text.split("\n")
        current = 0
        for row, line in enumerate(lines):
            line_len = len(line)
            if index <= current + line_len:
                return (row, index - current)
            current += line_len + 1
        if not lines:
            return (0, 0)
        return (len(lines) - 1, len(lines[-1]))
