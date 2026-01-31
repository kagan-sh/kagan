"""Snapshot tests for Kagan TUI visual regression testing.

These tests use isolated test apps that render just the components we want
to test, with fixed data (no dynamic dates) to ensure reproducible snapshots.
"""

from __future__ import annotations

from datetime import datetime

import pytest
from textual.app import App, ComposeResult
from textual.containers import Horizontal, Vertical

from kagan.database.models import Ticket, TicketPriority, TicketStatus, TicketType
from kagan.ui.widgets.card import TicketCard

pytestmark = pytest.mark.snapshot

# Fixed date for reproducible snapshots
FIXED_DATE = datetime(2025, 1, 15, 12, 0, 0)

# Card styles for test app
CARD_TEST_CSS = """
Screen { layout: horizontal; padding: 1; }
.card-column { width: 1fr; height: auto; padding: 0 1; }
TicketCard {
    width: 100%; height: auto; max-height: 10; min-height: 4;
    padding: 0 1; margin: 0 0 1 0; background: #0f1419; border: solid #2a3342;
}
TicketCard:focus { border: solid #e75535; background: #171c24; }
TicketCard.session-active { border: solid #98c379; }
TicketCard.agent-active { border: solid #c678dd; }
TicketCard .card-title { width: 100%; height: 1; text-style: bold; color: #dfe0e1; padding: 0; }
TicketCard .card-title-continued { width: 100%; height: 1; color: #dfe0e1; }
TicketCard .card-desc { width: 100%; height: 1; color: #5c6773; }
TicketCard .card-desc.high { color: #e75535; }
TicketCard .card-desc.medium { color: #e5c07b; }
TicketCard .card-desc.low { color: #98c379; }
TicketCard .card-meta { width: 100%; height: 1; color: #5c6773; }
TicketCard .card-review { width: 100%; height: 1; color: #56b6c2; }
TicketCard .card-checks { width: 100%; height: 1; color: #5c6773; }
"""


def make_ticket(
    title: str,
    description: str = "Test description",
    priority: TicketPriority = TicketPriority.MEDIUM,
    status: TicketStatus = TicketStatus.BACKLOG,
    ticket_type: TicketType = TicketType.PAIR,
    ticket_id: str = "test1234",
    review_summary: str | None = None,
    checks_passed: bool | None = None,
    session_active: bool = False,
) -> Ticket:
    """Create a ticket with fixed date for snapshot testing."""
    return Ticket(
        id=ticket_id,
        title=title,
        description=description,
        priority=priority,
        status=status,
        ticket_type=ticket_type,
        review_summary=review_summary,
        checks_passed=checks_passed,
        session_active=session_active,
        created_at=FIXED_DATE,
        updated_at=FIXED_DATE,
    )


class CardSnapshotApp(App):
    """Test app that displays TicketCards in isolation for snapshot testing."""

    CSS = CARD_TEST_CSS

    def __init__(self, tickets: list[tuple[Ticket, dict]], **kwargs):
        """Initialize with list of (ticket, state_flags) tuples."""
        super().__init__(**kwargs)
        self.tickets = tickets

    def compose(self) -> ComposeResult:
        """Compose the test layout with cards."""
        with Horizontal():
            with Vertical(classes="card-column"):
                for ticket, flags in self.tickets:
                    card = TicketCard(ticket)
                    if flags.get("session_active"):
                        card.is_session_active = True
                    if flags.get("agent_active"):
                        card.is_agent_active = True
                    yield card


class TestCardSnapshots:
    """Snapshot tests for TicketCard visual states."""

    def test_card_types_and_priorities(self, snap_compare):
        """Test card rendering with different types and priorities."""
        tickets = [
            (
                make_ticket(
                    "Implement auth",
                    "Add JWT authentication",
                    priority=TicketPriority.HIGH,
                    ticket_type=TicketType.AUTO,
                    ticket_id="auto1111",
                ),
                {},
            ),
            (
                make_ticket(
                    "Fix database bug",
                    "Connection timeout issues",
                    priority=TicketPriority.MEDIUM,
                    ticket_type=TicketType.PAIR,
                    ticket_id="pair2222",
                ),
                {},
            ),
            (
                make_ticket(
                    "Update documentation",
                    "Improve README",
                    priority=TicketPriority.LOW,
                    ticket_id="docs3333",
                ),
                {},
            ),
        ]
        assert snap_compare(CardSnapshotApp(tickets), terminal_size=(50, 20))

    def test_card_long_title_wrapping(self, snap_compare):
        """Test card with long title that wraps to multiple lines."""
        tickets = [
            (
                make_ticket(
                    "This is a very long title that should wrap to two lines",
                    "Testing multi-line title wrapping behavior",
                    ticket_id="long1111",
                ),
                {},
            ),
            (make_ticket("Short", "Short title for comparison", ticket_id="shrt2222"), {}),
        ]
        assert snap_compare(CardSnapshotApp(tickets), terminal_size=(50, 16))

    def test_card_session_states(self, snap_compare):
        """Test card rendering with different session states."""
        tickets = [
            (make_ticket("Normal card", "No session active", ticket_id="norm1111"), {}),
            (
                make_ticket(
                    "Session active",
                    "tmux session running",
                    ticket_id="sess2222",
                    session_active=True,
                ),
                {"session_active": True},
            ),
            (
                make_ticket("Agent working", "AI actively working", ticket_id="agnt3333"),
                {"agent_active": True},
            ),
        ]
        assert snap_compare(CardSnapshotApp(tickets), terminal_size=(50, 20))

    def test_card_review_status(self, snap_compare):
        """Test card rendering in review status with summary and checks."""
        tickets = [
            (
                make_ticket(
                    "Feature ready",
                    "Awaiting review",
                    status=TicketStatus.REVIEW,
                    ticket_id="pass1111",
                    review_summary="Added 3 endpoints",
                    checks_passed=True,
                ),
                {},
            ),
            (
                make_ticket(
                    "Needs fixes",
                    "Tests failing",
                    status=TicketStatus.REVIEW,
                    ticket_id="fail2222",
                    review_summary="Refactored API",
                    checks_passed=False,
                ),
                {},
            ),
            (
                make_ticket(
                    "Pending checks",
                    "Checks not run yet",
                    status=TicketStatus.REVIEW,
                    ticket_id="pend3333",
                    review_summary="Initial implementation",
                    checks_passed=None,
                ),
                {},
            ),
        ]
        assert snap_compare(CardSnapshotApp(tickets), terminal_size=(50, 26))
