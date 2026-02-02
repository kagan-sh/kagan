"""Debug log viewer modal for in-app debugging."""

from __future__ import annotations

from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING

from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Footer, Label, RichLog, Rule

from kagan.debug_log import (
    LogEntry,
    LogSource,
    clear_log_buffer,
    export_logs_to_file,
    get_buffer_generation,
    log_buffer,
)
from kagan.keybindings import DEBUG_LOG_BINDINGS

if TYPE_CHECKING:
    from textual.app import ComposeResult
    from textual.timer import Timer


class DebugLogModal(ModalScreen[None]):
    """Hidden debug log viewer modal (F12)."""

    BINDINGS = DEBUG_LOG_BINDINGS

    _log_refresh_timer: Timer | None = None

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self._line_count = 0
        self._buffer_generation = 0

    def compose(self) -> ComposeResult:
        with Vertical(id="debug-log-container"):
            yield Label("Debug Logs", classes="modal-title")
            yield Label(
                "[dim]F12 to toggle | c to clear | s to save | Escape to close[/dim]",
                classes="modal-subtitle",
            )
            yield Rule()
            yield RichLog(
                id="debug-log",
                highlight=True,
                markup=True,
                auto_scroll=True,
                wrap=True,
            )
        yield Footer()

    async def on_mount(self) -> None:
        """Load existing logs and start watching for new ones."""
        self._buffer_generation = get_buffer_generation()
        await self._update_logs()
        self._log_refresh_timer = self.set_interval(0.5, self._update_logs)

    def _cleanup_timer(self) -> None:
        """Stop and clear the log refresh timer."""
        if self._log_refresh_timer is not None:
            self._log_refresh_timer.stop()
            self._log_refresh_timer = None

    def on_unmount(self) -> None:
        """Clean up the timer when modal is closed."""
        self._cleanup_timer()

    async def _update_logs(self) -> None:
        """Update the log display with new entries."""
        # Check if buffer was cleared externally
        current_gen = get_buffer_generation()
        if current_gen != self._buffer_generation:
            self._buffer_generation = current_gen
            self._line_count = 0
            rich_log = self.query_one("#debug-log", RichLog)
            rich_log.clear()

        # Only process new entries without copying the entire buffer
        buffer_len = len(log_buffer)
        if buffer_len <= self._line_count:
            # No new entries (or buffer wrapped around)
            if buffer_len < self._line_count:
                # Buffer wrapped - reset and show current entries
                self._line_count = 0
                rich_log = self.query_one("#debug-log", RichLog)
                rich_log.clear()

        if buffer_len > self._line_count:
            rich_log = self.query_one("#debug-log", RichLog)
            # Convert deque to list once and slice only new entries (O(k) where k = new entries)
            # This avoids iterating through all old entries
            new_entries = list(log_buffer)[self._line_count :]
            for entry in new_entries:
                formatted = self._format_entry(entry)
                rich_log.write(formatted)
            self._line_count = buffer_len

    def _format_entry(self, entry: LogEntry) -> str:
        """Format a log entry for display."""
        ts = datetime.fromtimestamp(entry.timestamp).strftime("%H:%M:%S")
        color = self._get_color(entry.group)

        # Add source indicator for logging module entries
        source_indicator = "" if entry.source == LogSource.TEXTUAL else " [PY]"

        return f"[{color}]{ts} [{entry.group}]{source_indicator}[/{color}] {entry.message}"

    def _get_color(self, group: str) -> str:
        """Get color for log entry based on level."""
        colors = {
            "DEBUG": "dim",
            "INFO": "blue",
            "WARNING": "yellow",
            "ERROR": "red",
            "CRITICAL": "bold red",
        }
        return colors.get(group, "white")

    def action_close(self) -> None:
        """Close the modal."""
        self._cleanup_timer()
        self.dismiss(None)

    def action_clear_logs(self) -> None:
        """Clear all logs."""
        clear_log_buffer()
        self._buffer_generation = get_buffer_generation()
        self._line_count = 0
        rich_log = self.query_one("#debug-log", RichLog)
        rich_log.clear()
        rich_log.write("[dim]Logs cleared[/dim]")

    def action_save_logs(self) -> None:
        """Export logs to .kagan/debug.log file."""
        try:
            # Find .kagan directory
            kagan_dir = Path.cwd()
            while kagan_dir != kagan_dir.parent:
                if (kagan_dir / ".kagan").is_dir():
                    log_path = kagan_dir / ".kagan" / "debug.log"
                    count = export_logs_to_file(str(log_path))
                    rich_log = self.query_one("#debug-log", RichLog)
                    rich_log.write(f"[green]✓ Exported {count} log entries to {log_path}[/green]")
                    return
                kagan_dir = kagan_dir.parent

            # Fallback to current directory if .kagan not found
            log_path = Path.cwd() / "kagan_debug.log"
            count = export_logs_to_file(str(log_path))
            rich_log = self.query_one("#debug-log", RichLog)
            rich_log.write(f"[yellow]⚠ .kagan not found, saved to {log_path}[/yellow]")
        except Exception as e:
            rich_log = self.query_one("#debug-log", RichLog)
            rich_log.write(f"[red]✗ Failed to export logs: {e}[/red]")
