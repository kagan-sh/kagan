"""Help modal with keybindings reference and usage guide."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Footer, Label, Rule, Static, TabbedContent, TabPane

from kagan.keybindings import (
    APP_BINDINGS,
    CONFIRM_BINDINGS,
    DESCRIPTION_EDITOR_BINDINGS,
    HELP_BINDINGS,
    KANBAN_BINDINGS,
    KANBAN_LEADER_BINDINGS,
    REVIEW_BINDINGS,
    get_bindings_for_category,
    get_key_for_action,
    to_textual_bindings,
)

if TYPE_CHECKING:
    from textual.app import ComposeResult
    from textual.widget import Widget

    from kagan.keybindings import KeyBindingDef


class HelpModal(ModalScreen[None]):
    """Full help system modal with keybindings, concepts, and workflows."""

    BINDINGS = to_textual_bindings(HELP_BINDINGS)

    def _key(
        self,
        action: str,
        bindings: list[KeyBindingDef] | None = None,
        leader: bool = False,
    ) -> str:
        """Get display key for an action.

        Args:
            action: Action name to look up
            bindings: Optional binding list (defaults to KANBAN_BINDINGS)
            leader: If True, prefix with 'g+' for leader sequences
        """
        source = bindings or KANBAN_BINDINGS
        key = get_key_for_action(source, action)
        return f"g+{key}" if leader else key

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
        """Compose the keybindings reference section dynamically from registry."""
        children: list[Widget] = []

        # Board Navigation - combine vim keys with arrow alternatives
        children.append(Static("Board Navigation", classes="help-section-title"))
        nav_bindings = get_bindings_for_category(KANBAN_BINDINGS, "navigation")

        # Group vim + arrow keys together for cleaner display
        shown_actions = set()
        key_h = self._key("focus_left")
        key_j = self._key("focus_down")
        key_k = self._key("focus_up")
        key_l = self._key("focus_right")

        for b in nav_bindings:
            if b.help_group == "vim_horizontal" and b.action == "focus_left":
                children.append(self._key_row(f"{key_h} / Left", "Move focus to left column"))
                shown_actions.add("focus_left")
            elif b.help_group == "vim_horizontal" and b.action == "focus_right":
                if "focus_right" not in shown_actions:
                    children.append(self._key_row(f"{key_l} / Right", "Move focus to right column"))
                    shown_actions.add("focus_right")
            elif b.help_group == "vim_vertical" and b.action == "focus_down":
                if "focus_down" not in shown_actions:
                    children.append(self._key_row(f"{key_j} / Down", "Move focus down in column"))
                    shown_actions.add("focus_down")
            elif b.help_group == "vim_vertical" and b.action == "focus_up":
                if "focus_up" not in shown_actions:
                    children.append(self._key_row(f"{key_k} / Up", "Move focus up in column"))
                    shown_actions.add("focus_up")
            elif not b.help_group:  # Tab navigation
                display = "Tab" if b.key == "tab" else "Shift+Tab"
                children.append(self._key_row(display, b.help_description))
        children.append(Rule())

        # Primary Actions from registry
        children.append(Static("Primary Actions", classes="help-section-title"))
        primary_order = ["n", "e", "v", "enter", "x", "slash", "p", "comma"]
        primary_bindings = get_bindings_for_category(KANBAN_BINDINGS, "primary")
        for key in primary_order:
            for b in primary_bindings:
                if b.key == key:
                    display = b.display_key.capitalize() if b.key == "enter" else b.display_key
                    children.append(self._key_row(display, b.help_description))
                    break
        children.append(Rule())

        # Leader Key Actions from registry
        children.append(Static("Leader Key (press g, then...)", classes="help-section-title"))
        for b in KANBAN_LEADER_BINDINGS:
            children.append(self._key_row(f"g {b.display_key}", b.help_description))
        children.append(self._key_row("Escape", "Cancel leader mode"))
        children.append(Rule())

        # Context-Specific from registry
        children.append(Static("Context-Specific", classes="help-section-title"))
        context_bindings = get_bindings_for_category(KANBAN_BINDINGS, "context")
        for b in context_bindings:
            if b.key in ("a", "m"):  # Main context bindings
                children.append(self._key_row(b.key, b.help_description))
        children.append(Rule())

        # Global from registry
        children.append(Static("Global", classes="help-section-title"))
        # Show F1/? for help (F1 works everywhere, ? when not in text input)
        children.append(self._key_row("F1 / ?", "Open this help screen"))
        for b in APP_BINDINGS:
            if b.key == "ctrl+p":
                children.append(self._key_row("Ctrl+P", b.help_description))
        for b in APP_BINDINGS:
            if b.key == "q":
                children.append(self._key_row("q", b.help_description))
        children.append(self._key_row("Escape", "Close modal / cancel action"))
        children.append(Rule())

        # Modal Patterns (common patterns) - dynamic lookups
        key_save = get_key_for_action(DESCRIPTION_EDITOR_BINDINGS, "done")
        key_yes = get_key_for_action(CONFIRM_BINDINGS, "confirm")
        key_no = get_key_for_action(CONFIRM_BINDINGS, "cancel")
        # Format ctrl+s nicely
        save_display = "Ctrl+S" if key_save == "ctrl+s" else key_save

        children.append(Static("Modal Patterns", classes="help-section-title"))
        children.append(self._key_row("Escape", "Close or cancel (never saves)"))
        children.append(self._key_row(save_display, "Save (in edit contexts)"))
        children.append(self._key_row(f"{key_yes} / {key_no}", "Yes / No (confirm dialogs)"))

        return Vertical(*children, id="keybindings-content")

    def _compose_navigation(self) -> Vertical:
        """Compose the navigation guide section."""
        # Build leader key list dynamically
        leader_lines = []
        for b in KANBAN_LEADER_BINDINGS:
            leader_lines.append(
                f"  g + {b.display_key}  - {b.help_description}",
            )

        # Get keys from registry
        key_h = self._key("focus_left")
        key_j = self._key("focus_down")
        key_k = self._key("focus_up")
        key_l = self._key("focus_right")
        key_leader = self._key("activate_leader")

        return Vertical(
            Static("Vim-Style Navigation", classes="help-section-title"),
            Static(
                "Kagan uses vim-inspired navigation for efficiency. "
                "Arrow keys always work as alternatives.",
                classes="help-paragraph",
            ),
            Static(""),
            Static("Movement Keys:", classes="help-subsection"),
            Static(f"  {key_h} / Left   - Move left between columns", classes="help-code"),
            Static(f"  {key_j} / Down   - Move down within a column", classes="help-code"),
            Static(f"  {key_k} / Up     - Move up within a column", classes="help-code"),
            Static(f"  {key_l} / Right  - Move right between columns", classes="help-code"),
            Static(""),
            Rule(),
            Static("Leader Key System", classes="help-section-title"),
            Static(
                f"Press '{key_leader}' to enter leader mode. A hint bar appears showing available "
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
        # Get keys from registry for workflows
        key_new = self._key("new_ticket")
        key_plan = self._key("open_planner")
        key_open = self._key("open_session")
        key_watch = self._key("watch_agent")
        key_move_fwd = self._key("move_forward", KANBAN_LEADER_BINDINGS)
        key_diff = self._key("view_diff", KANBAN_LEADER_BINDINGS)
        key_review_modal = self._key("open_review", KANBAN_LEADER_BINDINGS)
        key_gen_review = self._key("generate_review", REVIEW_BINDINGS)
        key_approve = self._key("approve", REVIEW_BINDINGS)
        key_reject = self._key("reject", REVIEW_BINDINGS)

        # Get save key from modal binding
        key_save = get_key_for_action(DESCRIPTION_EDITOR_BINDINGS, "done")
        save_display = "Ctrl+S" if key_save == "ctrl+s" else key_save

        return Vertical(
            Static("Creating Tickets", classes="help-section-title"),
            Static(""),
            Static(f"Quick Create ({key_new}):", classes="help-subsection"),
            Static(
                f"  Press '{key_new}' to open the ticket form. Fill in title, description, "
                f"priority, and type. Press {save_display} to save.",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Static(f"Plan Mode ({key_plan}):", classes="help-subsection"),
            Static(
                f"  Press '{key_plan}' to enter Plan mode. Describe what you want to build "
                "in natural language. The AI will break it down into tickets.",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Rule(),
            Static("Working on Tickets", classes="help-section-title"),
            Static(""),
            Static("PAIR Workflow:", classes="help-subsection"),
            Static(
                f"  1. Select ticket in BACKLOG, press {key_open.capitalize()}\n"
                "  2. Kagan creates a git worktree and tmux session\n"
                "  3. Work with your AI agent in the session\n"
                f"  4. When done, move to REVIEW with g+{key_move_fwd}",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Static("AUTO Workflow:", classes="help-subsection"),
            Static(
                f"  1. Select ticket in BACKLOG, press {key_open.capitalize()}\n"
                "  2. Agent starts working autonomously\n"
                f"  3. Press '{key_watch}' or g+{key_watch} to watch progress\n"
                "  4. Agent moves ticket to REVIEW when done",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Rule(),
            Static("Reviewing & Completing", classes="help-section-title"),
            Static(""),
            Static("Review Process:", classes="help-subsection"),
            Static(
                f"  1. Select ticket in REVIEW\n"
                f"  2. Press g+{key_diff} to view the diff\n"
                f"  3. Press g+{key_review_modal} to open review modal\n"
                f"  4. Press '{key_gen_review}' to generate AI review\n"
                f"  5. Press '{key_approve}' to approve or '{key_reject}' to reject",
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
