"""Help modal with keybindings reference and usage guide."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Footer, Label, Rule, Static, TabbedContent, TabPane

from kagan.keybindings import (
    HELP_BINDINGS,
    KANBAN_LEADER_BINDINGS,
)

if TYPE_CHECKING:
    from textual.app import ComposeResult
    from textual.widget import Widget


class HelpModal(ModalScreen[None]):
    """Full help system modal with keybindings, concepts, and workflows."""

    BINDINGS = HELP_BINDINGS

    def compose(self) -> ComposeResult:
        with Vertical(id="help-container"):
            yield Label("Kagan Help", classes="modal-title")
            yield Rule(line_style="heavy")
            with TabbedContent(id="help-tabs"):
                with TabPane("Keybindings", id="tab-keys"):
                    yield VerticalScroll(self._compose_keybindings())
                with TabPane("Navigation", id="tab-nav"):
                    yield VerticalScroll(self._compose_navigation())
                with TabPane("Concepts", id="tab-concepts"):
                    yield VerticalScroll(self._compose_concepts())
                with TabPane("Workflows", id="tab-workflows"):
                    yield VerticalScroll(self._compose_workflows())
        yield Footer()

    def _compose_keybindings(self) -> Vertical:
        """Compose the keybindings reference section."""
        children: list[Widget] = []

        # Board Navigation
        children.append(Static("Board Navigation", classes="help-section-title"))
        children.append(self._key_row("h / Left", "Move focus to left column"))
        children.append(self._key_row("l / Right", "Move focus to right column"))
        children.append(self._key_row("j / Down", "Move focus down in column"))
        children.append(self._key_row("k / Up", "Move focus up in column"))
        children.append(self._key_row("Tab", "Cycle to next column"))
        children.append(self._key_row("Shift+Tab", "Cycle to previous column"))
        children.append(Rule())

        # Primary Actions
        children.append(Static("Primary Actions", classes="help-section-title"))
        children.append(self._key_row("n", "Create new ticket"))
        children.append(self._key_row("e", "Edit selected ticket"))
        children.append(self._key_row("v", "View ticket details"))
        children.append(self._key_row("Enter", "Open session (PAIR: tmux, AUTO: agent)"))
        children.append(self._key_row("x", "Delete ticket"))
        children.append(self._key_row("/", "Toggle search bar"))
        children.append(self._key_row("p", "Switch to Plan mode"))
        children.append(self._key_row(",", "Open settings"))
        children.append(Rule())

        # Leader Key Actions
        children.append(Static("Leader Key (press g, then...)", classes="help-section-title"))
        for b in KANBAN_LEADER_BINDINGS:
            if isinstance(b, Binding):
                key = b.key_display or b.key
                children.append(self._key_row(f"g {key}", b.description))
        children.append(self._key_row("Escape", "Cancel leader mode"))
        children.append(Rule())

        # Context-Specific
        children.append(Static("Context-Specific", classes="help-section-title"))
        children.append(self._key_row("a", "Start agent (AUTO tickets)"))
        children.append(self._key_row("s", "Stop agent (AUTO tickets)"))
        children.append(self._key_row("w", "Watch agent output (AUTO tickets)"))
        children.append(self._key_row("m", "Merge and complete (REVIEW tickets)"))
        children.append(self._key_row("D", "View diff (REVIEW tickets)"))
        children.append(self._key_row("r", "Open review modal (REVIEW tickets)"))
        children.append(Rule())

        # Global
        children.append(Static("Global", classes="help-section-title"))
        children.append(self._key_row("F1 / ?", "Open this help screen"))
        children.append(self._key_row("Ctrl+P", "Open command palette"))
        children.append(self._key_row("q", "Quit application"))
        children.append(self._key_row("Escape", "Close modal / cancel action"))
        children.append(Rule())

        # Modal Patterns
        children.append(Static("Modal Patterns", classes="help-section-title"))
        children.append(self._key_row("Escape", "Close or cancel (never saves)"))
        children.append(self._key_row("Ctrl+S", "Save (in edit contexts)"))
        children.append(self._key_row("y / n", "Yes / No (confirm dialogs)"))

        return Vertical(*children, id="keybindings-content")

    def _compose_navigation(self) -> Vertical:
        """Compose the navigation guide section."""
        # Build leader key list dynamically
        leader_lines = []
        for b in KANBAN_LEADER_BINDINGS:
            if isinstance(b, Binding):
                key = b.key_display or b.key
                leader_lines.append(f"  g + {key}  - {b.description}")

        return Vertical(
            Static("Vim-Style Navigation", classes="help-section-title"),
            Static(
                "Kagan uses vim-inspired navigation for efficiency. "
                "Arrow keys always work as alternatives.",
                classes="help-paragraph",
            ),
            Static(""),
            Static("Movement Keys:", classes="help-subsection"),
            Static("  h / Left   - Move left between columns", classes="help-code"),
            Static("  j / Down   - Move down within a column", classes="help-code"),
            Static("  k / Up     - Move up within a column", classes="help-code"),
            Static("  l / Right  - Move right between columns", classes="help-code"),
            Static(""),
            Rule(),
            Static("Leader Key System", classes="help-section-title"),
            Static(
                "Press 'g' to enter leader mode. A hint bar appears showing available "
                "actions. You have 2 seconds to press the next key, or press Escape to cancel.",
                classes="help-paragraph",
            ),
            Static(""),
            Static("Leader mode enables compound commands:", classes="help-subsection"),
            *[Static(line, classes="help-code") for line in leader_lines],
            Static(""),
            Rule(),
            Static("Focus & Selection", classes="help-section-title"),
            Static(
                "Only ticket cards can receive focus. The focused card is highlighted "
                "with a colored border. Double-click a card to view details.",
                classes="help-paragraph",
            ),
            id="navigation-content",
        )

    def _compose_concepts(self) -> Vertical:
        """Compose the concepts guide section."""
        return Vertical(
            Static("Ticket Types", classes="help-section-title"),
            Static(""),
            Static("PAIR (Human-in-the-loop):", classes="help-subsection"),
            Static(
                "  Opens a tmux session where you work alongside an AI agent. "
                "You control the pace and can intervene at any time. "
                "Best for complex tasks requiring human judgment.",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Static("AUTO (Autonomous Agent):", classes="help-subsection"),
            Static(
                "  Agent works independently with minimal supervision. "
                "You can watch progress or let it run in the background. "
                "Best for well-defined, routine tasks.",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Rule(),
            Static("Workflow Columns", classes="help-section-title"),
            Static(""),
            Static("BACKLOG", classes="help-subsection"),
            Static("  Tickets waiting to be started.", classes="help-paragraph-indented"),
            Static(""),
            Static("IN PROGRESS", classes="help-subsection"),
            Static(
                "  Active work. PAIR tickets have tmux sessions; AUTO tickets have agents running.",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Static("REVIEW", classes="help-subsection"),
            Static(
                "  Work complete, pending review. View diff, run AI review, "
                "then approve or reject.",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Static("DONE", classes="help-subsection"),
            Static(
                "  Merged and completed. Worktrees are cleaned up.",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Rule(),
            Static("Visual Indicators", classes="help-section-title"),
            Static(""),
            Static("Card Borders:", classes="help-subsection"),
            Static("  Green border  - tmux session active", classes="help-code"),
            Static("  Pulsing border - Agent actively working", classes="help-code"),
            Static(""),
            Static("Type Badges:", classes="help-subsection"),
            Static("  Human icon   - PAIR ticket", classes="help-code"),
            Static("  Lightning    - AUTO ticket", classes="help-code"),
            id="concepts-content",
        )

    def _compose_workflows(self) -> Vertical:
        """Compose the workflows guide section."""
        return Vertical(
            Static("Creating Tickets", classes="help-section-title"),
            Static(""),
            Static("Quick Create (n):", classes="help-subsection"),
            Static(
                "  Press 'n' to open the ticket form. Fill in title, description, "
                "priority, and type. Press Ctrl+S to save.",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Static("Plan Mode (p):", classes="help-subsection"),
            Static(
                "  Press 'p' to enter Plan mode. Describe what you want to build "
                "in natural language. The AI will break it down into tickets.",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Rule(),
            Static("Working on Tickets", classes="help-section-title"),
            Static(""),
            Static("PAIR Workflow:", classes="help-subsection"),
            Static(
                "  1. Select ticket in BACKLOG, press Enter\n"
                "  2. Kagan creates a git worktree and tmux session\n"
                "  3. Work with your AI agent in the session\n"
                "  4. When done, move to REVIEW with g+l",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Static("AUTO Workflow:", classes="help-subsection"),
            Static(
                "  1. Select ticket in BACKLOG, press Enter\n"
                "  2. Agent starts working autonomously\n"
                "  3. Press 'w' or g+w to watch progress\n"
                "  4. Agent moves ticket to REVIEW when done",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Rule(),
            Static("Reviewing & Completing", classes="help-section-title"),
            Static(""),
            Static("Review Process:", classes="help-subsection"),
            Static(
                "  1. Select ticket in REVIEW\n"
                "  2. Press g+d to view the diff\n"
                "  3. Press g+r to open review modal\n"
                "  4. Press 'g' to generate AI review\n"
                "  5. Press 'a' to approve or 'r' to reject",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Static("Completing:", classes="help-subsection"),
            Static(
                "  Approved tickets are merged to main, worktrees cleaned up, and moved to DONE.",
                classes="help-paragraph-indented",
            ),
            id="workflows-content",
        )

    def _key_row(self, key: str, description: str) -> Horizontal:
        """Create a key-description row."""
        return Horizontal(
            Static(key, classes="help-key"),
            Static(description, classes="help-desc"),
            classes="help-key-row",
        )

    def action_close(self) -> None:
        self.dismiss(None)
