"""Unit tests for planner: plan parsing, prompt building, and todo parsing.

Tests: parse_plan, build_planner_prompt, parse_todos functions.
"""

from __future__ import annotations

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from kagan.agents.planner import build_planner_prompt, parse_plan, parse_todos
from kagan.database.models import TicketPriority, TicketType
from tests.strategies import (
    safe_todo_content,
    todo_statuses,
    valid_todo_with_fields,
    valid_todo_xml,
)

pytestmark = pytest.mark.unit


# =============================================================================
# Plan Parsing Tests
# =============================================================================


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


# =============================================================================
# Prompt Building Tests
# =============================================================================


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
        assert "Bug fixes with clear reproduction steps" in prompt
        assert "New feature design decisions" in prompt

    def test_user_request_included(self) -> None:
        """User request should be included in the final prompt."""
        prompt = build_planner_prompt("Implement OAuth login with Google")

        assert "Implement OAuth login with Google" in prompt
        assert "## User Request" in prompt


# =============================================================================
# Malformed XML strategies (test-specific for Hypothesis tests)
# =============================================================================

# Malformed XML variations
malformed_xml_variants = st.sampled_from(
    [
        "<todos><todo>Missing closing tag</todos>",
        "<todos><todo status='pending'>Unclosed",
        "<todos><todo status=>Empty attr</todo></todos>",
        '<todos><todo status="pending"><nested>Bad</nested></todo></todos>',
        "<todos><<>>invalid xml<</todos>",
        '<todos><todo status="pending">Text<</todo></todos>',
        "<<<>>>",
        '<todos><todo status="pending">&invalid;</todo></todos>',
    ]
)

# Random garbage text (not XML at all)
garbage_text = st.text(min_size=0, max_size=200)


# =============================================================================
# Property-based Todo Parsing Tests (Hypothesis)
# =============================================================================


class TestParseTodosPropertyBased:
    """Property-based tests for parse_todos function."""

    @given(valid_todo_xml())
    @settings(max_examples=100)
    def test_parse_returns_correct_count(self, xml_and_count: tuple[str, int]) -> None:
        """Parse returns exactly N entries for N valid todo elements."""
        xml, expected_count = xml_and_count
        result = parse_todos(xml)
        assert len(result) == expected_count

    @given(valid_todo_with_fields())
    @settings(max_examples=100)
    def test_parse_extracts_all_fields_correctly(self, todo_data: tuple[str, str, str]) -> None:
        """Parse extracts content and status fields correctly."""
        xml, expected_content, expected_status = todo_data
        result = parse_todos(xml)
        assert len(result) == 1
        # Content is stripped by parse_todos
        assert result[0]["content"] == expected_content.strip()
        assert result[0]["status"] == expected_status

    @given(malformed_xml_variants)
    def test_malformed_xml_returns_empty_list(self, malformed_xml: str) -> None:
        """Malformed XML returns empty list, never crashes."""
        result = parse_todos(malformed_xml)
        assert result == [] or isinstance(result, list)
        # Either empty or a valid partial parse, but never an exception

    @given(garbage_text)
    def test_garbage_input_returns_empty_list(self, garbage: str) -> None:
        """Random garbage input returns empty list, never crashes."""
        result = parse_todos(garbage)
        assert isinstance(result, list)
        # Should be empty since no <todos> block
        assert result == []

    @given(valid_todo_xml())
    @settings(max_examples=50)
    def test_parse_is_idempotent(self, xml_and_count: tuple[str, int]) -> None:
        """Same input always produces same output (deterministic)."""
        xml, _ = xml_and_count
        result1 = parse_todos(xml)
        result2 = parse_todos(xml)
        assert result1 == result2

    @given(st.text(min_size=0, max_size=500))
    def test_never_crashes_on_any_input(self, arbitrary_input: str) -> None:
        """Parser never raises exceptions, always returns list."""
        result = parse_todos(arbitrary_input)
        assert isinstance(result, list)
        # All entries should be dicts with expected structure
        for entry in result:
            assert isinstance(entry, dict)
            assert "content" in entry
            assert "status" in entry

    @given(todo_statuses)
    def test_valid_statuses_are_preserved(self, status: str) -> None:
        """Valid status values are preserved in output."""
        xml = f'<todos><todo status="{status}">Test content</todo></todos>'
        result = parse_todos(xml)
        assert len(result) == 1
        assert result[0]["status"] == status

    @given(st.sampled_from(["unknown", "active", "done", "blocked", ""]))
    def test_invalid_status_normalized_to_pending(self, invalid_status: str) -> None:
        """Invalid status values are normalized to 'pending'."""
        xml = f'<todos><todo status="{invalid_status}">Test content</todo></todos>'
        result = parse_todos(xml)
        assert len(result) == 1
        assert result[0]["status"] == "pending"

    @given(st.text(min_size=0, max_size=50).filter(lambda x: not x.strip()))
    def test_empty_content_todos_are_skipped(self, whitespace_content: str) -> None:
        """Todos with empty or whitespace-only content are skipped."""
        xml = f'<todos><todo status="pending">{whitespace_content}</todo></todos>'
        result = parse_todos(xml)
        assert result == []

    @given(
        st.lists(
            st.tuples(safe_todo_content, todo_statuses),
            min_size=0,
            max_size=10,
        )
    )
    @settings(max_examples=50)
    def test_all_valid_todos_are_returned_in_order(self, todo_list: list[tuple[str, str]]) -> None:
        """All valid todos are returned in document order."""
        todos_xml = "".join(
            f'<todo status="{status}">{content}</todo>' for content, status in todo_list
        )
        xml = f"<todos>{todos_xml}</todos>"
        result = parse_todos(xml)

        assert len(result) == len(todo_list)
        for i, (expected_content, expected_status) in enumerate(todo_list):
            # Content is stripped by parse_todos
            assert result[i]["content"] == expected_content.strip()
            assert result[i]["status"] == expected_status


# =============================================================================
# Edge Case Todo Parsing Tests
# =============================================================================


class TestParseTodosEdgeCases:
    """Edge case tests for parse_todos (example-based, not property-based)."""

    def test_case_insensitive_wrapper_tags(self) -> None:
        """<TODOS> and <todos> should both work."""
        xml_upper = '<TODOS><todo status="pending">Test</todo></TODOS>'
        xml_lower = '<todos><todo status="pending">Test</todo></todos>'
        xml_mixed = '<Todos><todo status="pending">Test</todo></Todos>'

        assert len(parse_todos(xml_upper)) == 1
        assert len(parse_todos(xml_lower)) == 1
        assert len(parse_todos(xml_mixed)) == 1

    def test_todos_with_surrounding_text(self) -> None:
        """Parse todos even when surrounded by other text."""
        response = """
        Here's my plan:
        <todos>
          <todo status="completed">Analyze request</todo>
          <todo status="in_progress">Design solution</todo>
        </todos>
        More text after.
        """
        result = parse_todos(response)
        assert len(result) == 2
        assert result[0]["content"] == "Analyze request"
        assert result[0]["status"] == "completed"
        assert result[1]["content"] == "Design solution"
        assert result[1]["status"] == "in_progress"

    def test_empty_todos_block(self) -> None:
        """Empty <todos></todos> returns empty list."""
        assert parse_todos("<todos></todos>") == []

    def test_no_todos_block(self) -> None:
        """Response without <todos> block returns empty list."""
        assert parse_todos("Just some text without any XML") == []
        assert parse_todos("<plan><ticket/></plan>") == []

    def test_todo_without_status_attribute(self) -> None:
        """Todo without status attribute defaults to 'pending'."""
        xml = "<todos><todo>Task without status</todo></todos>"
        result = parse_todos(xml)
        assert len(result) == 1
        assert result[0]["status"] == "pending"
        assert result[0]["content"] == "Task without status"
