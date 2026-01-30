"""Unit tests for planner plan parsing and prompt building.

Tests: parse_plan, build_planner_prompt functions.
"""

from __future__ import annotations

import pytest

from kagan.agents.planner import build_planner_prompt, parse_plan
from kagan.database.models import TicketPriority, TicketType

pytestmark = pytest.mark.unit


class TestParsePlan:
    """Test parse_plan function for multi-ticket parsing."""

    def test_parse_single_ticket_in_plan(self) -> None:
        """Parse a plan with a single ticket."""
        response = """
        Here's my plan:
        <plan>
        <ticket>
          <title>Add user authentication</title>
          <type>PAIR</type>
          <description>Implement login/logout functionality</description>
          <acceptance_criteria>
            <criterion>Users can log in</criterion>
            <criterion>Users can log out</criterion>
          </acceptance_criteria>
          <priority>high</priority>
        </ticket>
        </plan>
        """
        tickets = parse_plan(response)
        assert len(tickets) == 1
        assert tickets[0].title == "Add user authentication"
        assert tickets[0].ticket_type == TicketType.PAIR
        assert tickets[0].priority == TicketPriority.HIGH
        assert len(tickets[0].acceptance_criteria) == 2

    def test_parse_multiple_tickets(self) -> None:
        """Parse a plan with multiple tickets."""
        response = """
        <plan>
        <ticket>
          <title>Create database schema</title>
          <type>AUTO</type>
          <description>Set up initial database tables</description>
          <priority>high</priority>
        </ticket>
        <ticket>
          <title>Build REST API</title>
          <type>PAIR</type>
          <description>Create API endpoints</description>
          <priority>medium</priority>
        </ticket>
        <ticket>
          <title>Add logging</title>
          <type>AUTO</type>
          <description>Implement logging infrastructure</description>
          <priority>low</priority>
        </ticket>
        </plan>
        """
        tickets = parse_plan(response)
        assert len(tickets) == 3
        assert tickets[0].title == "Create database schema"
        assert tickets[0].ticket_type == TicketType.AUTO
        assert tickets[1].title == "Build REST API"
        assert tickets[1].ticket_type == TicketType.PAIR
        assert tickets[2].title == "Add logging"
        assert tickets[2].ticket_type == TicketType.AUTO

    def test_parse_plan_default_type_is_pair(self) -> None:
        """Default ticket type should be PAIR when not specified."""
        response = """
        <plan>
        <ticket>
          <title>Design new feature</title>
          <description>Feature without type specified</description>
        </ticket>
        </plan>
        """
        tickets = parse_plan(response)
        assert len(tickets) == 1
        assert tickets[0].ticket_type == TicketType.PAIR

    def test_parse_plan_no_plan_block(self) -> None:
        """Return empty list when no plan block found."""
        response = "I need more information. What features do you need?"
        tickets = parse_plan(response)
        assert tickets == []

    def test_parse_plan_malformed_xml(self) -> None:
        """Handle malformed XML gracefully."""
        response = """
        <plan>
        <ticket>
          <title>Broken ticket
          <description>Missing closing tags
        </ticket>
        </plan>
        """
        tickets = parse_plan(response)
        assert tickets == []

    def test_parse_plan_empty_plan(self) -> None:
        """Handle empty plan block."""
        response = "<plan></plan>"
        tickets = parse_plan(response)
        assert tickets == []

    def test_parse_plan_case_insensitive(self) -> None:
        """Plan wrapper tags should be case insensitive."""
        response = """
        <PLAN>
        <ticket>
          <title>Test ticket</title>
          <type>auto</type>
          <description>Testing case insensitivity</description>
        </ticket>
        </PLAN>
        """
        tickets = parse_plan(response)
        assert len(tickets) == 1
        assert tickets[0].title == "Test ticket"
        assert tickets[0].ticket_type == TicketType.AUTO

    def test_parse_plan_with_surrounding_text(self) -> None:
        """Parse plan when surrounded by other text."""
        response = """
        Based on your requirements, I've created a plan:

        <plan>
        <ticket>
          <title>Implement feature X</title>
          <type>PAIR</type>
          <description>Build the feature</description>
        </ticket>
        </plan>

        Let me know if you'd like any changes!
        """
        tickets = parse_plan(response)
        assert len(tickets) == 1
        assert tickets[0].title == "Implement feature X"


class TestBuildPlannerPrompt:
    """Test build_planner_prompt always includes format instructions."""

    def test_format_instructions_always_included(self) -> None:
        """Format instructions (AUTO/PAIR, XML format) must always be present."""
        prompt = build_planner_prompt("Create a login feature")

        assert "<plan>" in prompt
        assert "<ticket>" in prompt
        assert "<type>AUTO or PAIR</type>" in prompt

        assert "**AUTO**" in prompt
        assert "**PAIR**" in prompt
        assert "Bug fixes with clear steps" in prompt
        assert "New feature design" in prompt

    def test_user_request_included(self) -> None:
        """User request should be included in the final prompt."""
        prompt = build_planner_prompt("Implement OAuth login with Google")

        assert "Implement OAuth login with Google" in prompt
        assert "## User Request" in prompt
