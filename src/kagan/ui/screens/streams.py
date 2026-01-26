"""Agent streams screen for viewing agent output in tabbed interface."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

from textual.binding import Binding
from textual.containers import Vertical
from textual.screen import Screen
from textual.widgets import Footer, RichLog, Static, TabbedContent, TabPane

from kagan.ui.widgets.header import KaganHeader

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.acp.agent import Agent
    from kagan.app import KaganApp

# Tab IDs
REVIEWER_TAB_ID = "reviewer"


class AgentStreamPane(TabPane):
    """Individual agent tab with status display and streaming output."""

    def __init__(
        self,
        agent_id: str,
        agent: Agent | None = None,
        title: str | None = None,
        **kwargs,
    ) -> None:
        """Initialize agent stream pane.

        Args:
            agent_id: Unique identifier for this agent (usually ticket_id).
            agent: Reference to the Agent instance for message routing.
            title: Display title for the tab.
            **kwargs: Additional TabPane arguments.
        """
        display_title = title or agent_id
        super().__init__(display_title, id=f"agent-{agent_id}", **kwargs)
        self.agent_id = agent_id
        self.agent = agent
        self._is_running = True

    def compose(self) -> ComposeResult:
        """Compose the agent pane layout."""
        yield Static(
            "[bold green]RUNNING[/bold green]",
            id=f"status-{self.agent_id}",
            classes="stream-status",
        )
        yield RichLog(
            id=f"log-{self.agent_id}",
            wrap=True,
            highlight=True,
            markup=True,
            classes="stream-log",
        )

    @property
    def is_running(self) -> bool:
        """Check if agent is running."""
        return self._is_running

    def set_running(self, running: bool) -> None:
        """Update running status display."""
        self._is_running = running
        try:
            status = self.query_one(f"#status-{self.agent_id}", Static)
            if running:
                status.update("[bold green]RUNNING[/bold green]")
            else:
                status.update("[bold blue]STOPPED[/bold blue]")
        except Exception:
            pass

    def append_output(self, text: str, style: str = "") -> None:
        """Append text to the log.

        Args:
            text: Text to append.
            style: Optional Rich style string.
        """
        try:
            log = self.query_one(f"#log-{self.agent_id}", RichLog)
            if style:
                log.write(f"[{style}]{text}[/{style}]")
            else:
                log.write(text)
            log.scroll_end(animate=False)
        except Exception:
            pass

    def get_log(self) -> RichLog | None:
        """Get the RichLog widget for scrolling."""
        try:
            return self.query_one(f"#log-{self.agent_id}", RichLog)
        except Exception:
            return None


class ReviewerPane(TabPane):
    """Fixed reviewer tab showing review agent activity."""

    def __init__(self, **kwargs) -> None:
        """Initialize reviewer pane."""
        super().__init__("Reviewer", id=REVIEWER_TAB_ID, **kwargs)

    def compose(self) -> ComposeResult:
        """Compose the reviewer pane layout."""
        yield Static(
            "[bold cyan]REVIEW AGENT[/bold cyan]",
            id="reviewer-status",
            classes="stream-status",
        )
        yield RichLog(
            id="reviewer-log",
            wrap=True,
            highlight=True,
            markup=True,
            classes="stream-log",
        )

    def log_review_start(self, ticket_id: str) -> None:
        """Log start of a review.

        Args:
            ticket_id: The ticket being reviewed.
        """
        self._append_output(f"\n[bold cyan]--- Review started for {ticket_id} ---[/bold cyan]\n")

    def log_decision(self, approved: bool, ticket_id: str) -> None:
        """Log review decision.

        Args:
            approved: Whether the ticket was approved.
            ticket_id: The ticket ID.
        """
        if approved:
            self._append_output(
                f"[bold green]APPROVED[/bold green] - Ticket {ticket_id} passed review\n"
            )
        else:
            self._append_output(
                f"[bold red]REJECTED[/bold red] - Ticket {ticket_id} needs changes\n"
            )

    def log_summary(self, summary: str) -> None:
        """Log review summary/reason.

        Args:
            summary: The review summary or rejection reason.
        """
        self._append_output(f"[dim]{summary}[/dim]\n")

    def append_output(self, text: str, style: str = "") -> None:
        """Append text to the reviewer log.

        Args:
            text: Text to append.
            style: Optional Rich style string.
        """
        self._append_output(text if not style else f"[{style}]{text}[/{style}]")

    def _append_output(self, text: str) -> None:
        """Internal append method."""
        try:
            log = self.query_one("#reviewer-log", RichLog)
            log.write(text)
            log.scroll_end(animate=False)
        except Exception:
            pass

    def get_log(self) -> RichLog | None:
        """Get the RichLog widget for scrolling."""
        try:
            return self.query_one("#reviewer-log", RichLog)
        except Exception:
            return None


class AgentStreamsScreen(Screen):
    """Full-screen tabbed view of agent output streams."""

    BINDINGS = [
        Binding("escape", "back", "Back"),
        Binding("h", "prev_tab", "Prev Tab", show=False),
        Binding("l", "next_tab", "Next Tab", show=False),
        Binding("j", "scroll_down", "Scroll Down", show=False),
        Binding("k", "scroll_up", "Scroll Up", show=False),
    ]

    def __init__(self, **kwargs) -> None:
        """Initialize agent streams screen."""
        super().__init__(**kwargs)
        self._agent_panes: dict[str, AgentStreamPane] = {}
        self._reviewer_pane: ReviewerPane | None = None

    @property
    def kagan_app(self) -> KaganApp:
        """Get the typed KaganApp instance."""
        return cast("KaganApp", self.app)

    def compose(self) -> ComposeResult:
        """Compose the streams screen layout."""
        yield KaganHeader(ticket_count=0)
        with Vertical(id="streams-container"):
            with TabbedContent(id="streams-tabs"):
                # Fixed reviewer tab
                yield ReviewerPane()
        yield Footer()

    async def on_mount(self) -> None:
        """Initialize screen on mount."""
        # Store reference to reviewer pane
        self._reviewer_pane = self.query_one(f"#{REVIEWER_TAB_ID}", ReviewerPane)

        # Initial tab refresh
        self._refresh_agent_tabs()

        # Set up periodic refresh
        self.set_interval(2.0, self._refresh_agent_tabs)

        # Update header
        header = self.query_one(KaganHeader)
        header.update_count(len(self._agent_panes))

    def _refresh_agent_tabs(self) -> None:
        """Refresh tabs based on active agents.

        - Add new tabs for newly spawned agents
        - Mark stopped tabs (update status)
        """
        try:
            manager = self.kagan_app.agent_manager
            active_ids = set(manager.list_active())

            tabbed = self.query_one("#streams-tabs", TabbedContent)

            # Add new tabs for new agents
            for agent_id in active_ids:
                if agent_id not in self._agent_panes:
                    agent = manager.get(agent_id)
                    # Create short title from agent_id
                    short_title = agent_id[:8] if len(agent_id) > 8 else agent_id
                    pane = AgentStreamPane(
                        agent_id=agent_id,
                        agent=agent,
                        title=short_title,
                    )
                    self._agent_panes[agent_id] = pane
                    tabbed.add_pane(pane)

            # Update status for existing panes
            for agent_id, pane in self._agent_panes.items():
                is_active = agent_id in active_ids
                if pane.is_running != is_active:
                    pane.set_running(is_active)

            # Update header with agent count
            header = self.query_one(KaganHeader)
            header.update_agents(
                len(active_ids),
                self.kagan_app.config.general.max_concurrent_agents,
            )

        except Exception:
            # Silently handle errors during refresh
            pass

    def get_agent_pane(self, agent_id: str) -> AgentStreamPane | None:
        """Get agent pane by ID.

        Args:
            agent_id: The agent/ticket ID.

        Returns:
            The AgentStreamPane or None if not found.
        """
        return self._agent_panes.get(agent_id)

    def get_reviewer_pane(self) -> ReviewerPane | None:
        """Get the reviewer pane.

        Returns:
            The ReviewerPane instance.
        """
        return self._reviewer_pane

    def _get_current_log(self) -> RichLog | None:
        """Get the RichLog of the currently active tab."""
        try:
            tabbed = self.query_one("#streams-tabs", TabbedContent)
            active_tab_id = tabbed.active
            if active_tab_id == REVIEWER_TAB_ID:
                if self._reviewer_pane:
                    return self._reviewer_pane.get_log()
            else:
                # Extract agent_id from tab id (format: "agent-{agent_id}")
                if active_tab_id and active_tab_id.startswith("agent-"):
                    agent_id = active_tab_id[6:]  # Remove "agent-" prefix
                    pane = self._agent_panes.get(agent_id)
                    if pane:
                        return pane.get_log()
        except Exception:
            pass
        return None

    # Actions

    def action_back(self) -> None:
        """Return to Kanban screen."""
        self.app.pop_screen()

    def action_prev_tab(self) -> None:
        """Switch to previous tab."""
        try:
            tabbed = self.query_one("#streams-tabs", TabbedContent)
            # Get list of tab IDs
            tabs = list(tabbed.query("TabPane"))
            if not tabs:
                return
            current_id = tabbed.active
            current_idx = next(
                (i for i, t in enumerate(tabs) if t.id == current_id),
                0,
            )
            new_idx = (current_idx - 1) % len(tabs)
            new_tab = tabs[new_idx]
            if new_tab.id:
                tabbed.active = new_tab.id
        except Exception:
            pass

    def action_next_tab(self) -> None:
        """Switch to next tab."""
        try:
            tabbed = self.query_one("#streams-tabs", TabbedContent)
            # Get list of tab IDs
            tabs = list(tabbed.query("TabPane"))
            if not tabs:
                return
            current_id = tabbed.active
            current_idx = next(
                (i for i, t in enumerate(tabs) if t.id == current_id),
                0,
            )
            new_idx = (current_idx + 1) % len(tabs)
            new_tab = tabs[new_idx]
            if new_tab.id:
                tabbed.active = new_tab.id
        except Exception:
            pass

    def action_scroll_down(self) -> None:
        """Scroll down in current log."""
        log = self._get_current_log()
        if log:
            log.scroll_down()

    def action_scroll_up(self) -> None:
        """Scroll up in current log."""
        log = self._get_current_log()
        if log:
            log.scroll_up()
