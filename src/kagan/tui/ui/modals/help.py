"""Help modal with keybindings reference and usage guide."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Footer, Label, Rule, Static, TabbedContent, TabPane

from kagan.core.limits import DEBUG_BUILD
from kagan.tui.keybindings import HELP_BINDINGS

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
        yield Footer(show_command_palette=False)

    def _compose_keybindings(self) -> Vertical:
        """Compose the keybindings reference section."""
        children: list[Widget] = []

        def add_section(title: str) -> None:
            children.append(Static(title, classes="help-section-title"))

        def add_subsection(title: str) -> None:
            children.append(Static(title, classes="help-subsection"))

        def add_rows(rows: list[tuple[str, str]]) -> None:
            for key, desc in rows:
                children.append(self._key_row(key, desc))

        add_section("Global")
        global_keys: list[tuple[str, str]] = [
            ("? / F1", "Help"),
            (". / Ctrl+P", "Actions palette"),
            ("Ctrl+O", "Project selector"),
            ("Ctrl+R", "Repo selector"),
        ]
        if DEBUG_BUILD:
            global_keys.append(("F12", "Debug log"))
        global_keys.append(("q", "Quit"))
        add_rows(global_keys)
        children.append(Rule())

        add_section("Board (Kanban)")
        add_subsection("Navigation")
        add_rows(
            [
                ("h / Left", "Focus left column"),
                ("l / Right", "Focus right column"),
                ("j / Down", "Focus next card"),
                ("k / Up", "Focus previous card"),
                ("Tab", "Next column"),
                ("Shift+Tab", "Previous column"),
                ("Esc", "Clear focus / close search / close peek"),
            ]
        )
        add_subsection("Tasks")
        add_rows(
            [
                ("n", "New task"),
                ("Shift+N", "New AUTO task"),
                ("Enter", "Open task workspace"),
                ("/", "Search tasks"),
                ("v", "View details"),
                ("e", "Edit task"),
                ("x", "Delete task"),
                ("y", "Duplicate task"),
                ("c", "Copy task ID"),
                ("space", "Peek overlay"),
                ("f", "Expand description"),
                ("F5", "Full editor"),
            ]
        )
        add_subsection("Workflow")
        add_rows(
            [
                ("Shift+H / Shift+L", "Move task left/right"),
                ("p", "Plan mode"),
                ("a", "Start agent (AUTO)"),
                ("s", "Stop agent (AUTO)"),
                ("Shift+D", "View diff (REVIEW)"),
                ("r", "Task Output (REVIEW)"),
                ("m", "Merge (REVIEW)"),
                ("b", "Set task branch"),
                (",", "Settings"),
                ("Ctrl+C", "Quit"),
            ]
        )
        children.append(Rule())

        add_section("Planner")
        add_subsection("Screen")
        add_rows(
            [
                ("Esc", "Back to board"),
                ("Ctrl+C", "Stop current run"),
                ("F2", "Enhance prompt"),
                ("b", "Set task branch"),
            ]
        )
        add_subsection("Input")
        add_rows(
            [
                ("Enter", "Send message"),
                ("Shift+Enter / Ctrl+J", "New line"),
                ("/clear", "Clear conversation"),
                ("/help", "Show commands"),
            ]
        )
        add_subsection("Slash Complete")
        add_rows(
            [
                ("Up / Down", "Navigate commands"),
                ("Enter", "Select command"),
                ("Esc", "Dismiss list"),
            ]
        )
        add_subsection("Plan Approval")
        add_rows(
            [
                ("Up / Down or j / k", "Move selection"),
                ("Enter", "Preview task"),
                ("a", "Approve"),
                ("e", "Edit"),
                ("d / Esc", "Dismiss"),
            ]
        )
        children.append(Rule())

        add_section("Welcome & Onboarding")
        add_subsection("Welcome Screen")
        add_rows(
            [
                ("Enter", "Open selected project"),
                ("n", "New project"),
                ("o", "Open folder"),
                ("s", "Settings"),
                ("1-9", "Open project by number"),
                ("Esc", "Quit"),
            ]
        )
        add_subsection("Onboarding")
        add_rows([("Esc", "Quit")])
        children.append(Rule())

        add_section("Repo Picker")
        add_rows(
            [
                ("Up / Down or j / k", "Navigate repos"),
                ("Enter", "Select repo"),
                ("n", "Add repo"),
                ("Esc", "Cancel"),
            ]
        )
        children.append(Rule())

        add_section("Modals")
        add_subsection("Help")
        add_rows([("Esc / q", "Close")])
        add_subsection("Confirm")
        add_rows(
            [
                ("Enter", "Confirm"),
                ("y", "Yes"),
                ("n", "No"),
                ("Esc", "Cancel"),
            ]
        )
        add_subsection("Task Details")
        add_rows(
            [
                ("e", "Toggle edit"),
                ("d", "Delete"),
                ("f", "Expand description"),
                ("F5", "Full editor"),
                ("F2 / Alt+S", "Save (edit mode)"),
                ("y", "Copy"),
                ("Esc", "Close/Cancel"),
            ]
        )
        add_subsection("Task Editor")
        add_rows(
            [
                ("F2 / Alt+S", "Finish editing"),
                ("Esc", "Cancel"),
            ]
        )
        add_subsection("Description Editor")
        add_rows(
            [
                ("F2 / Alt+S", "Save"),
                ("Esc", "Cancel"),
            ]
        )
        add_subsection("Settings")
        add_rows(
            [
                ("F2 / Alt+S", "Save"),
                ("Esc", "Cancel"),
            ]
        )
        add_subsection("Duplicate Task")
        add_rows(
            [
                ("Enter", "Create"),
                ("Esc", "Cancel"),
            ]
        )
        add_subsection("Diff")
        add_rows(
            [
                ("Enter", "Approve"),
                ("r", "Reject"),
                ("y", "Copy"),
                ("Esc", "Close"),
            ]
        )
        add_subsection("Task Output")
        add_rows(
            [
                ("Enter", "Approve"),
                ("r", "Reject"),
                ("g", "Run review"),
                ("y", "Copy"),
                ("Esc", "Close/Cancel"),
            ]
        )
        add_subsection("Rejection Input")
        add_rows(
            [
                ("Enter", "Back to In Progress"),
                ("Esc", "Backlog"),
            ]
        )
        add_subsection("Agent/Review Chat Input")
        add_rows(
            [
                ("Enter", "Send message"),
                ("Shift+Enter / Ctrl+J", "New line"),
            ]
        )
        if DEBUG_BUILD:
            add_subsection("Debug Log")
            add_rows(
                [
                    ("c", "Clear logs"),
                    ("s", "Save logs"),
                    ("Esc", "Close"),
                ]
            )
        add_subsection("Tmux Gateway")
        add_rows(
            [
                ("Enter", "Continue"),
                ("Esc", "Cancel"),
                ("s", "Don't show again"),
            ]
        )
        add_subsection("Base Branch")
        add_rows(
            [
                ("Enter", "Submit"),
                ("Esc", "Cancel"),
            ]
        )
        add_subsection("Permission Prompt")
        add_rows(
            [
                ("Enter", "Allow once"),
                ("a", "Allow always"),
                ("Esc / n", "Deny"),
            ]
        )
        add_subsection("No Dedicated Hotkeys")
        add_rows(
            [
                ("Merge Dialog", "Use buttons and checkboxes"),
                ("New Project", "Use inputs and buttons"),
                ("Folder Picker", "Use input/tree and buttons"),
            ]
        )

        return Vertical(*children, id="keybindings-content")

    def _compose_navigation(self) -> Vertical:
        """Compose the navigation guide section."""
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
            Static("Actions Palette", classes="help-section-title"),
            Static(
                "Press '.' (or Ctrl+P) to open the Actions palette. It lists commands for the "
                "current screen and task context.",
                classes="help-paragraph",
            ),
            Static(""),
            Static("Palette Tips:", classes="help-subsection"),
            Static("  Type to filter commands", classes="help-code"),
            Static("  Enter to run selected action", classes="help-code"),
            Static(""),
            Rule(),
            Static("Focus & Selection", classes="help-section-title"),
            Static(
                "Only task cards can receive focus. The focused card is highlighted "
                "with a colored border. Double-click a card to view details.",
                classes="help-paragraph",
            ),
            id="navigation-content",
        )

    def _compose_concepts(self) -> Vertical:
        """Compose the concepts guide section."""
        return Vertical(
            Static("Task Types", classes="help-section-title"),
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
            Static("  Tasks waiting to be started.", classes="help-paragraph-indented"),
            Static(""),
            Static("IN PROGRESS", classes="help-subsection"),
            Static(
                "  Active work. PAIR tasks have tmux sessions; AUTO tasks have agents running.",
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
            Static("  Human icon   - PAIR task", classes="help-code"),
            Static("  Lightning    - AUTO task", classes="help-code"),
            id="concepts-content",
        )

    def _compose_workflows(self) -> Vertical:
        """Compose the workflows guide section."""
        return Vertical(
            Static("Creating Tasks", classes="help-section-title"),
            Static(""),
            Static("Quick Create (n):", classes="help-subsection"),
            Static(
                "  Press 'n' to open the task form. Fill in title, description, "
                "priority, and type. Press F2 to save.",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Static("Plan Mode (p):", classes="help-subsection"),
            Static(
                "  Press 'p' (or use the Actions palette) to enter Plan mode. Describe what "
                "you want to build in natural language. The AI will break it down into tasks.",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Rule(),
            Static("Working on Tasks", classes="help-section-title"),
            Static(""),
            Static("PAIR Workflow:", classes="help-subsection"),
            Static(
                "  1. Select task in BACKLOG, press Enter\n"
                "  2. Kagan creates a git worktree and tmux session\n"
                "  3. Work with your AI agent in the session\n"
                "  4. When done, move to REVIEW with Shift+L",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Static("AUTO Workflow:", classes="help-subsection"),
            Static(
                "  1. Select task in BACKLOG, press Enter\n"
                "  2. Confirm start to run the agent and open output\n"
                "  3. Enter on IN_PROGRESS opens output\n"
                "  4. Agent moves task to REVIEW when done",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Rule(),
            Static("Reviewing & Completing", classes="help-section-title"),
            Static(""),
            Static("Review Process:", classes="help-subsection"),
            Static(
                "  1. Select task in REVIEW\n"
                "  2. Press Enter to open Task Output\n"
                "  3. Use Summary/Diff/Review Output/Agent Output tabs\n"
                "  4. Press Enter to approve or 'r' to reject",
                classes="help-paragraph-indented",
            ),
            Static(""),
            Static("Completing:", classes="help-subsection"),
            Static(
                "  Press 'm' on the board to merge approved tasks. Worktrees are cleaned up.",
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
