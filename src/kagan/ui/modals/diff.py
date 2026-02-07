"""Modal for viewing git diffs."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Label, RichLog, Rule, Static, TabbedContent, TabPane

from kagan.keybindings import DIFF_BINDINGS
from kagan.ui.utils.clipboard import copy_with_notification

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.core.models.entities import Task
    from kagan.services.diffs import RepoDiff


class DiffModal(ModalScreen[str | None]):
    """Modal for showing task diffs.

    Returns:
        str | None:
            - "approve" if user pressed 'a'
            - "reject" if user pressed 'r'
            - None if user just closed the modal
    """

    BINDINGS = DIFF_BINDINGS

    def __init__(
        self,
        *,
        title: str | None = None,
        diff_text: str | None = None,
        diffs: list[RepoDiff] | None = None,
        task: Task | None = None,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self._title = title or "WORKSPACE DIFF"
        self._diff_text = diff_text or ""
        self._diffs = diffs or []
        self._task_model = task

    def compose(self) -> ComposeResult:
        with Vertical(id="diff-container"):
            yield Label(self._title, classes="modal-title")
            if self._diffs:
                with TabbedContent():
                    for diff in self._diffs:
                        tab_label = (
                            f"{diff.repo_name} (+{diff.total_additions}/-{diff.total_deletions})"
                        )
                        with TabPane(tab_label, id=f"tab-{diff.repo_id}"):
                            yield from self._render_repo_diff(diff)
            else:
                yield RichLog(id="diff-log", wrap=True, highlight=True)
            yield Rule()
            with Horizontal(classes="button-row"):
                yield Button("Close", variant="primary", id="close-btn")
        yield Footer(show_command_palette=False)

    def on_mount(self) -> None:
        # Style button row for center alignment (match other modals)
        button_row = self.query_one(".button-row", Horizontal)
        button_row.styles.width = "100%"
        button_row.styles.height = "auto"
        button_row.styles.align = ("center", "middle")
        button_row.styles.padding = (1, 0, 0, 0)

        if not self._diffs:
            log = self.query_one("#diff-log", RichLog)
            # Remove border to avoid double-border effect (container already has border)
            log.styles.border = ("none", "transparent")
            for line in self._diff_text.splitlines() or ["(No diff available)"]:
                log.write(line)

    def _render_repo_diff(self, diff: RepoDiff) -> ComposeResult:
        with VerticalScroll(classes="diff-scroll"):
            for file in diff.files:
                yield Label(
                    f"{file.status.upper()}: {file.path} (+{file.additions}/-{file.deletions})",
                    classes="diff-header",
                )
                yield Static(
                    self._colorize_diff(file.diff_content),
                    classes="diff-content",
                )

    def _colorize_diff(self, content: str) -> str:
        lines: list[str] = []
        for line in content.split("\n"):
            if line.startswith("+") and not line.startswith("+++"):
                lines.append(f"[green]{line}[/green]")
            elif line.startswith("-") and not line.startswith("---"):
                lines.append(f"[red]{line}[/red]")
            elif line.startswith("@@"):
                lines.append(f"[cyan]{line}[/cyan]")
            else:
                lines.append(line)
        return "\n".join(lines)

    def action_close(self) -> None:
        """Close the modal without any action."""
        self.dismiss(None)

    def action_approve(self) -> None:
        """Approve and dismiss the modal."""
        self.dismiss("approve")

    def action_reject(self) -> None:
        """Reject and dismiss the modal."""
        self.dismiss("reject")

    def action_copy(self) -> None:
        """Copy diff content to clipboard."""
        content = self._diff_text or self._build_unified_diff()
        copy_with_notification(self.app, content, "Diff")

    async def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "close-btn":
            self.dismiss(None)

    def _build_unified_diff(self) -> str:
        if not self._diffs:
            return self._diff_text

        lines: list[str] = []
        for diff in self._diffs:
            lines.append(f"# === {diff.repo_name} ({diff.target_branch}) ===")
            lines.append(f"# +{diff.total_additions} -{diff.total_deletions}")
            lines.append("")
            for file in diff.files:
                lines.append(file.diff_content)
                lines.append("")
        return "\n".join(lines)
