"""Tests for TicketFormBuilder form factory.

Comprehensive tests for ticket form field generation and value extraction.
"""

from __future__ import annotations

import pytest
from hypothesis import given, settings
from textual.app import App, ComposeResult
from textual.containers import Container, Vertical
from textual.widgets import Input, Label, Select, Static, TextArea

from kagan.database.models import Ticket, TicketPriority, TicketStatus, TicketType
from kagan.ui.forms.ticket_form import FormMode, TicketFormBuilder
from kagan.ui.widgets.base import (
    AcceptanceCriteriaArea,
    AgentBackendSelect,
    DescriptionArea,
    PrioritySelect,
    StatusSelect,
    TicketTypeSelect,
    TitleInput,
)
from tests.strategies import priorities, ticket_form_data, ticket_types, tickets

pytestmark = pytest.mark.unit


# =============================================================================
# Test Fixtures
# =============================================================================


def create_test_ticket(
    title: str = "Test Ticket",
    description: str = "Test description",
    priority: TicketPriority = TicketPriority.MEDIUM,
    ticket_type: TicketType = TicketType.PAIR,
    status: TicketStatus = TicketStatus.BACKLOG,
    acceptance_criteria: list[str] | None = None,
    agent_backend: str = "",
) -> Ticket:
    """Create a test ticket with defaults."""
    return Ticket.create(
        title=title,
        description=description,
        priority=priority,
        ticket_type=ticket_type,
        status=status,
        acceptance_criteria=acceptance_criteria or [],
        agent_backend=agent_backend,
    )


class FormTestApp(App):
    """Test app for TicketFormBuilder tests."""

    CSS = """
    .edit-fields { display: block; }
    .view-only { display: block; }
    """

    def __init__(
        self,
        ticket: Ticket | None = None,
        mode: FormMode = FormMode.CREATE,
        agent_options: list[tuple[str, str]] | None = None,
        editing: bool = False,
    ):
        super().__init__()
        self.ticket = ticket
        self.mode = mode
        self.agent_options = agent_options
        self.editing = editing

    def compose(self) -> ComposeResult:
        with Container(id="form-container"):
            yield from TicketFormBuilder.build_field_selects(
                self.ticket, self.mode, self.agent_options
            )
            yield from TicketFormBuilder.build_status_field(self.ticket, self.mode)
            yield from TicketFormBuilder.build_title_field(self.ticket, self.mode)
            yield from TicketFormBuilder.build_description_field(
                self.ticket, self.mode, self.editing
            )
            yield from TicketFormBuilder.build_acceptance_criteria_field(self.ticket, self.mode)


# =============================================================================
# TestBuildFieldSelects
# =============================================================================


class TestBuildFieldSelects:
    """Tests for build_field_selects method."""

    async def test_view_mode_yields_nothing(self):
        """VIEW mode should not yield any widgets."""
        widgets = list(TicketFormBuilder.build_field_selects(None, FormMode.VIEW))
        assert len(widgets) == 0

    async def test_create_mode_yields_priority_type_agent(self):
        """CREATE mode yields all three select fields."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            # Check for presence of all three selects
            priority_select = container.query_one("#priority-select", PrioritySelect)
            type_select = container.query_one("#type-select", TicketTypeSelect)
            agent_select = container.query_one("#agent-backend-select", AgentBackendSelect)

            assert priority_select is not None
            assert type_select is not None
            assert agent_select is not None

    async def test_edit_mode_disables_type_selector(self):
        """EDIT mode should disable the type selector."""
        ticket = create_test_ticket()
        app = FormTestApp(ticket=ticket, mode=FormMode.EDIT)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            type_select = container.query_one("#type-select", TicketTypeSelect)
            assert type_select.disabled is True

    async def test_create_mode_enables_type_selector(self):
        """CREATE mode should enable the type selector."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            type_select = container.query_one("#type-select", TicketTypeSelect)
            assert type_select.disabled is False

    async def test_uses_ticket_values_when_provided(self):
        """Should use ticket's priority/type/backend when provided."""
        ticket = create_test_ticket(
            priority=TicketPriority.HIGH,
            ticket_type=TicketType.AUTO,
            agent_backend="custom-agent",
        )
        app = FormTestApp(
            ticket=ticket,
            mode=FormMode.EDIT,
            agent_options=[("Default", ""), ("Custom", "custom-agent")],
        )
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            priority_select = container.query_one("#priority-select", Select)
            type_select = container.query_one("#type-select", Select)
            agent_select = container.query_one("#agent-backend-select", Select)

            assert priority_select.value == TicketPriority.HIGH.value
            assert type_select.value == TicketType.AUTO.value
            assert agent_select.value == "custom-agent"

    async def test_defaults_when_no_ticket(self):
        """Should use defaults (MEDIUM priority, PAIR type) when no ticket."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            priority_select = container.query_one("#priority-select", Select)
            type_select = container.query_one("#type-select", Select)

            assert priority_select.value == TicketPriority.MEDIUM.value
            assert type_select.value == TicketType.PAIR.value

    async def test_agent_options_provided(self):
        """Should use provided agent options."""
        agent_options = [("Agent A", "a"), ("Agent B", "b")]
        # Create a ticket with a matching agent_backend to avoid BLANK value
        ticket = create_test_ticket(agent_backend="a")
        app = FormTestApp(ticket=ticket, mode=FormMode.EDIT, agent_options=agent_options)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            agent_select = container.query_one("#agent-backend-select", AgentBackendSelect)
            # Agent should be set to "a" from the ticket
            assert agent_select.value == "a"

    async def test_agent_defaults_when_no_options(self):
        """Should use default agent option when none provided."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE, agent_options=None)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            agent_select = container.query_one("#agent-backend-select", AgentBackendSelect)
            assert agent_select.value == ""

    @given(priorities)
    @settings(max_examples=5)
    async def test_various_priorities(self, priority: TicketPriority):
        """Should handle any valid priority value."""
        ticket = create_test_ticket(priority=priority)
        app = FormTestApp(ticket=ticket, mode=FormMode.EDIT)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            priority_select = container.query_one("#priority-select", Select)
            assert priority_select.value == priority.value

    @given(ticket_types)
    @settings(max_examples=5)
    async def test_various_ticket_types(self, tt: TicketType):
        """Should handle any valid ticket type value."""
        ticket = create_test_ticket(ticket_type=tt)
        app = FormTestApp(ticket=ticket, mode=FormMode.EDIT)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            type_select = container.query_one("#type-select", Select)
            assert type_select.value == tt.value


# =============================================================================
# TestBuildStatusField
# =============================================================================


class TestBuildStatusField:
    """Tests for build_status_field method."""

    async def test_create_mode_yields_status_field(self):
        """CREATE mode should yield status select."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            status_select = container.query_one("#status-select", StatusSelect)
            assert status_select is not None
            assert status_select.value == TicketStatus.BACKLOG.value

    async def test_view_mode_yields_nothing(self):
        """VIEW mode should not yield status field."""
        widgets = list(TicketFormBuilder.build_status_field(None, FormMode.VIEW))
        assert len(widgets) == 0

    async def test_edit_mode_yields_nothing(self):
        """EDIT mode should not yield status field."""
        ticket = create_test_ticket()
        widgets = list(TicketFormBuilder.build_status_field(ticket, FormMode.EDIT))
        assert len(widgets) == 0


# =============================================================================
# TestBuildTitleField
# =============================================================================


class TestBuildTitleField:
    """Tests for build_title_field method."""

    async def test_yields_both_view_and_edit_elements(self):
        """Should yield static display AND input field."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            # View mode elements
            title_label = container.query_one("#title-section-label", Label)
            title_display = container.query_one("#title-display", Static)
            # Edit mode element
            title_input = container.query_one("#title-input", TitleInput)

            assert title_label is not None
            assert title_display is not None
            assert title_input is not None

    async def test_uses_ticket_title_when_provided(self):
        """Should populate with ticket title when ticket provided."""
        ticket = create_test_ticket(title="My Special Title")
        app = FormTestApp(ticket=ticket, mode=FormMode.EDIT)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            title_display = container.query_one("#title-display", Static)
            title_input = container.query_one("#title-input", Input)

            # Check that both show the title
            assert "My Special Title" in str(title_display.render())
            assert title_input.value == "My Special Title"

    async def test_empty_title_when_no_ticket(self):
        """Should have empty title when no ticket provided."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            title_input = container.query_one("#title-input", Input)
            assert title_input.value == ""


# =============================================================================
# TestBuildDescriptionField
# =============================================================================


class TestBuildDescriptionField:
    """Tests for build_description_field method."""

    async def test_yields_header_and_content(self):
        """Should yield header with expand hint and content areas."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            # Check for description elements
            description_content = container.query_one("#description-content", Static)
            description_input = container.query_one("#description-input", DescriptionArea)
            expand_btn = container.query_one("#expand-btn", Static)

            assert description_content is not None
            assert description_input is not None
            assert expand_btn is not None

    async def test_editing_shows_f5_hint(self):
        """When editing=True, should show [F5] Full Editor hint."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE, editing=True)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            expand_btn = container.query_one("#expand-btn", Static)
            # Rich markup renders [F5] as a style, so check for "Full Editor"
            rendered = str(expand_btn.render())
            assert "Full Editor" in rendered

    async def test_view_shows_expand_hint(self):
        """When editing=False, should show [f] Expand hint."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE, editing=False)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            expand_btn = container.query_one("#expand-btn", Static)
            # Rich markup renders [f] as a style, so check for "Expand"
            rendered = str(expand_btn.render())
            assert "Expand" in rendered

    async def test_no_description_shows_placeholder(self):
        """Should show '(No description)' when description is empty."""
        ticket = create_test_ticket(description="")
        app = FormTestApp(ticket=ticket, mode=FormMode.VIEW)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            description_content = container.query_one("#description-content", Static)
            assert "(No description)" in str(description_content.render())

    async def test_description_shown_when_provided(self):
        """Should show description when provided."""
        ticket = create_test_ticket(description="My detailed description here")
        app = FormTestApp(ticket=ticket, mode=FormMode.VIEW)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            description_content = container.query_one("#description-content", Static)
            description_input = container.query_one("#description-input", TextArea)

            assert "My detailed description here" in str(description_content.render())
            assert description_input.text == "My detailed description here"


# =============================================================================
# TestBuildAcceptanceCriteriaField
# =============================================================================


class TestBuildAcceptanceCriteriaField:
    """Tests for build_acceptance_criteria_field method."""

    async def test_view_mode_with_criteria_shows_list(self):
        """VIEW mode with criteria should render list items."""
        ticket = create_test_ticket(
            acceptance_criteria=["User can login", "Error messages shown", "Data is saved"]
        )
        app = FormTestApp(ticket=ticket, mode=FormMode.VIEW)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            # Check for AC section
            ac_section = container.query_one("#ac-section", Vertical)
            assert ac_section is not None

            # Check for individual criteria items
            ac_items = container.query(".ac-item")
            assert len(ac_items) == 3

    async def test_view_mode_without_criteria_no_section(self):
        """VIEW mode without criteria should not render section."""
        ticket = create_test_ticket(acceptance_criteria=[])
        app = FormTestApp(ticket=ticket, mode=FormMode.VIEW)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            # AC section should not exist (or have no items)
            ac_sections = container.query("#ac-section")
            assert len(ac_sections) == 0

    async def test_edit_mode_yields_textarea(self):
        """EDIT mode should yield textarea for criteria."""
        ticket = create_test_ticket(acceptance_criteria=["Criterion 1", "Criterion 2"])
        app = FormTestApp(ticket=ticket, mode=FormMode.EDIT)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            ac_input = container.query_one("#ac-input", AcceptanceCriteriaArea)
            assert ac_input is not None
            # Check that criteria are populated
            assert "Criterion 1" in ac_input.text
            assert "Criterion 2" in ac_input.text

    async def test_create_mode_yields_empty_textarea(self):
        """CREATE mode should yield empty textarea for criteria."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            ac_input = container.query_one("#ac-input", AcceptanceCriteriaArea)
            assert ac_input is not None
            assert ac_input.text == ""


# =============================================================================
# TestGetFormValues
# =============================================================================


class TestGetFormValues:
    """Tests for get_form_values extraction method."""

    async def test_extracts_title_value(self):
        """Should extract title from title input."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            # Set title value
            title_input = container.query_one("#title-input", Input)
            title_input.value = "  My New Title  "

            values = TicketFormBuilder.get_form_values(container)
            assert values["title"] == "My New Title"  # Should be stripped

    async def test_extracts_description_value(self):
        """Should extract description from textarea."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            desc_input = container.query_one("#description-input", TextArea)
            desc_input.text = "My description text"

            values = TicketFormBuilder.get_form_values(container)
            assert values["description"] == "My description text"

    async def test_extracts_priority_value(self):
        """Should extract priority enum from select."""
        ticket = create_test_ticket(priority=TicketPriority.HIGH)
        app = FormTestApp(ticket=ticket, mode=FormMode.EDIT)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            values = TicketFormBuilder.get_form_values(container)
            assert values["priority"] == TicketPriority.HIGH

    async def test_extracts_ticket_type_value(self):
        """Should extract ticket type enum from select."""
        ticket = create_test_ticket(ticket_type=TicketType.AUTO)
        app = FormTestApp(ticket=ticket, mode=FormMode.EDIT)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            values = TicketFormBuilder.get_form_values(container)
            assert values["ticket_type"] == TicketType.AUTO

    async def test_extracts_status_value(self):
        """Should extract status enum from select (CREATE mode only)."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            values = TicketFormBuilder.get_form_values(container)
            assert values["status"] == TicketStatus.BACKLOG

    async def test_extracts_agent_backend_value(self):
        """Should extract agent backend from select."""
        ticket = create_test_ticket(agent_backend="my-agent")
        app = FormTestApp(
            ticket=ticket,
            mode=FormMode.EDIT,
            agent_options=[("Default", ""), ("My Agent", "my-agent")],
        )
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            values = TicketFormBuilder.get_form_values(container)
            assert values["agent_backend"] == "my-agent"

    async def test_handles_missing_widgets_gracefully(self):
        """Should return partial dict when some widgets missing."""

        # Create a minimal app that only has some widgets
        class MinimalFormApp(App):
            def compose(self) -> ComposeResult:
                with Container(id="form-container"):
                    yield TitleInput(value="Only Title")

        app = MinimalFormApp()
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            values = TicketFormBuilder.get_form_values(container)
            assert values["title"] == "Only Title"
            # Other values should not be present
            assert "description" not in values
            assert "priority" not in values

    async def test_extracts_acceptance_criteria(self):
        """Should extract AC list from textarea."""
        ticket = create_test_ticket(acceptance_criteria=["AC 1", "AC 2"])
        app = FormTestApp(ticket=ticket, mode=FormMode.EDIT)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            values = TicketFormBuilder.get_form_values(container)
            assert "acceptance_criteria" in values
            assert values["acceptance_criteria"] == ["AC 1", "AC 2"]

    async def test_agent_backend_none_when_empty(self):
        """Should return None for agent_backend when empty string selected."""
        app = FormTestApp(ticket=None, mode=FormMode.CREATE)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            values = TicketFormBuilder.get_form_values(container)
            assert values.get("agent_backend") is None

    async def test_handles_blank_select_values(self):
        """Should handle blank select values gracefully."""

        # This tests the Select.BLANK handling
        class BlankSelectApp(App):
            def compose(self) -> ComposeResult:
                with Container(id="form-container"):
                    yield Select(
                        options=[("Option A", "a"), ("Option B", "b")],
                        allow_blank=True,
                        id="priority-select",
                    )

        app = BlankSelectApp()
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            # Select is blank by default when allow_blank=True
            values = TicketFormBuilder.get_form_values(container)
            # Priority should not be in values because it's BLANK
            assert "priority" not in values


# =============================================================================
# TestResetFormToTicket
# =============================================================================


class TestResetFormToTicket:
    """Tests for reset_form_to_ticket method."""

    async def test_resets_all_fields_to_ticket_values(self):
        """Should reset all form fields to match ticket."""
        # Start with default/empty values
        app = FormTestApp(
            ticket=None,
            mode=FormMode.CREATE,
            agent_options=[("Default", ""), ("Custom", "custom")],
        )
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            # Create a ticket with specific values to reset to
            ticket = create_test_ticket(
                title="Reset Title",
                description="Reset Description",
                priority=TicketPriority.HIGH,
                ticket_type=TicketType.AUTO,
                acceptance_criteria=["Reset AC 1", "Reset AC 2"],
                agent_backend="custom",
            )

            # Reset the form
            TicketFormBuilder.reset_form_to_ticket(container, ticket)

            # Verify all values are reset
            title_input = container.query_one("#title-input", Input)
            assert title_input.value == "Reset Title"

            desc_input = container.query_one("#description-input", TextArea)
            assert desc_input.text == "Reset Description"

            priority_select = container.query_one("#priority-select", Select)
            assert priority_select.value == TicketPriority.HIGH.value

            type_select = container.query_one("#type-select", Select)
            assert type_select.value == TicketType.AUTO.value

            agent_select = container.query_one("#agent-backend-select", Select)
            assert agent_select.value == "custom"

            ac_input = container.query_one("#ac-input", TextArea)
            assert "Reset AC 1" in ac_input.text
            assert "Reset AC 2" in ac_input.text

    async def test_handles_missing_widgets_gracefully(self):
        """Should not crash when some widgets don't exist."""

        class MinimalFormApp(App):
            def compose(self) -> ComposeResult:
                with Container(id="form-container"):
                    yield TitleInput(value="Initial")

        app = MinimalFormApp()
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            ticket = create_test_ticket(title="New Title")

            # Should not raise even though most widgets are missing
            TicketFormBuilder.reset_form_to_ticket(container, ticket)

            # Title should be updated
            title_input = container.query_one("#title-input", Input)
            assert title_input.value == "New Title"

    async def test_resets_empty_acceptance_criteria(self):
        """Should handle tickets with no acceptance criteria."""
        app = FormTestApp(
            ticket=create_test_ticket(acceptance_criteria=["Initial AC"]),
            mode=FormMode.EDIT,
        )
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            # Reset to ticket with no AC
            ticket = create_test_ticket(acceptance_criteria=[])
            TicketFormBuilder.reset_form_to_ticket(container, ticket)

            ac_input = container.query_one("#ac-input", TextArea)
            assert ac_input.text == ""

    async def test_resets_none_description(self):
        """Should handle tickets with empty description."""
        app = FormTestApp(
            ticket=create_test_ticket(description="Initial Description"),
            mode=FormMode.EDIT,
        )
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            # Create ticket with empty description (simulating None)
            ticket = Ticket.create(
                title="Test",
                description="",
                priority=TicketPriority.MEDIUM,
            )
            TicketFormBuilder.reset_form_to_ticket(container, ticket)

            desc_input = container.query_one("#description-input", TextArea)
            assert desc_input.text == ""


# =============================================================================
# Hypothesis Property-Based Tests
# =============================================================================


class TestTicketFormHypothesis:
    """Property-based tests for ticket form handling."""

    @given(tickets())
    @settings(max_examples=10)
    async def test_form_roundtrip_with_any_ticket(self, ticket: Ticket):
        """Form should correctly display and extract values for any valid ticket."""
        app = FormTestApp(
            ticket=ticket,
            mode=FormMode.EDIT,
            agent_options=[("Default", ""), ("Agent", ticket.agent_backend or "")],
        )
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            values = TicketFormBuilder.get_form_values(container)

            # Title should match (stripped because get_form_values strips)
            assert values["title"] == ticket.title.strip()
            # Priority and ticket_type should always match
            assert values["priority"] == ticket.priority
            assert values["ticket_type"] == ticket.ticket_type

    @given(ticket_form_data())
    @settings(max_examples=10)
    async def test_form_accepts_various_form_data(self, form_data: dict):
        """Form should handle various form data inputs."""
        # Handle None description from strategy
        description = form_data["description"] if form_data["description"] is not None else ""
        ticket = Ticket.create(
            title=form_data["title"],
            description=description,
            priority=form_data["priority"],
            ticket_type=form_data["ticket_type"],
            acceptance_criteria=form_data["acceptance_criteria"],
        )

        app = FormTestApp(ticket=ticket, mode=FormMode.EDIT)
        async with app.run_test() as pilot:
            await pilot.pause()
            container = app.query_one("#form-container")

            values = TicketFormBuilder.get_form_values(container)

            # Values should match the input (title is stripped by get_form_values)
            assert values["title"] == form_data["title"].strip()
            assert values["priority"] == form_data["priority"]
            assert values["ticket_type"] == form_data["ticket_type"]


# =============================================================================
# FormMode Tests
# =============================================================================


class TestFormMode:
    """Tests for FormMode enum."""

    def test_form_mode_values(self):
        """FormMode should have CREATE, VIEW, and EDIT values."""
        assert FormMode.CREATE is not None
        assert FormMode.VIEW is not None
        assert FormMode.EDIT is not None
        assert len(FormMode) == 3

    def test_form_modes_are_distinct(self):
        """Each FormMode should be unique."""
        modes = [FormMode.CREATE, FormMode.VIEW, FormMode.EDIT]
        assert len(set(modes)) == 3
