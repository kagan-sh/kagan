"""Unit tests for UI widgets to improve coverage."""

from __future__ import annotations

from unittest.mock import MagicMock, patch

import pytest
from textual.app import App, ComposeResult
from textual.containers import Vertical
from textual.widgets import Static

from kagan.database.models import Ticket, TicketPriority, TicketStatus, TicketType
from kagan.ui.screens.planner.state import SlashCommand
from kagan.ui.widgets.plan_approval import PlanApprovalWidget
from kagan.ui.widgets.slash_complete import SlashComplete
from kagan.ui.widgets.streaming_output import StreamingOutput, ThinkingIndicator
from tests.helpers.mocks import MessageCapture

pytestmark = pytest.mark.unit


# =============================================================================
# SlashComplete Widget Tests
# =============================================================================


class SlashCompleteTestApp(App):
    """Test app for SlashComplete widget."""

    def compose(self) -> ComposeResult:
        yield Vertical(id="container")


class TestSlashComplete:
    """Tests for SlashComplete widget."""

    async def test_compose_creates_option_list(self):
        """Test that compose yields an OptionList."""
        app = SlashCompleteTestApp()
        async with app.run_test() as pilot:
            widget = SlashComplete()
            await app.query_one("#container").mount(widget)
            await pilot.pause()
            assert widget.query_one("#slash-options")

    async def test_action_cursor_up(self):
        """Test cursor up action delegates to option list."""
        app = SlashCompleteTestApp()
        async with app.run_test() as pilot:
            widget = SlashComplete()
            await app.query_one("#container").mount(widget)
            widget.slash_commands = [
                SlashCommand("test1", "Test 1"),
                SlashCommand("test2", "Test 2"),
            ]
            await pilot.pause()
            # Should not raise
            widget.action_cursor_up()

    async def test_action_cursor_down(self):
        """Test cursor down action delegates to option list."""
        app = SlashCompleteTestApp()
        async with app.run_test() as pilot:
            widget = SlashComplete()
            await app.query_one("#container").mount(widget)
            widget.slash_commands = [
                SlashCommand("test1", "Test 1"),
                SlashCommand("test2", "Test 2"),
            ]
            await pilot.pause()
            # Should not raise
            widget.action_cursor_down()

    async def test_action_select_posts_completed_message(self):
        """Test select action posts Completed message."""
        app = SlashCompleteTestApp()
        async with app.run_test() as pilot:
            widget = SlashComplete()
            await app.query_one("#container").mount(widget)
            widget.slash_commands = [
                SlashCommand("plan", "Create plan"),
                SlashCommand("help", "Show help"),
            ]
            await pilot.pause()

            capture = MessageCapture()
            with patch.object(widget, "post_message", capture):
                widget.action_select()

            msg = capture.assert_single(SlashComplete.Completed)
            assert msg.command == "plan"

    async def test_action_dismiss_posts_dismissed_message(self):
        """Test dismiss action posts Dismissed message."""
        app = SlashCompleteTestApp()
        async with app.run_test() as pilot:
            widget = SlashComplete()
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            capture = MessageCapture()
            with patch.object(widget, "post_message", capture):
                widget.action_dismiss()

            capture.assert_single(SlashComplete.Dismissed)

    async def test_rebuild_options_empty_commands(self):
        """Test rebuild options handles empty commands list."""
        app = SlashCompleteTestApp()
        async with app.run_test() as pilot:
            widget = SlashComplete()
            await app.query_one("#container").mount(widget)
            widget.slash_commands = []
            await pilot.pause()
            # Should not raise and have empty options
            assert widget._commands_list == []

    async def test_rebuild_options_exception_handling(self):
        """Test that _rebuild_options handles exceptions gracefully."""
        widget = SlashComplete()
        # Not mounted, so query will fail - should handle gracefully
        widget._rebuild_options()  # Should not raise

    async def test_watch_slash_commands_when_not_mounted(self):
        """Test watch_slash_commands doesn't rebuild when not mounted."""
        widget = SlashComplete()
        # Not mounted
        widget.slash_commands = [SlashCommand("test", "Test")]
        widget.watch_slash_commands()
        # Should not raise


# =============================================================================
# PlanApprovalWidget Tests
# =============================================================================


class PlanApprovalTestApp(App):
    """Test app for PlanApprovalWidget."""

    def compose(self) -> ComposeResult:
        yield Vertical(id="container")


def create_test_ticket(
    title: str = "Test Ticket",
    ticket_type: TicketType = TicketType.AUTO,
    priority: TicketPriority = TicketPriority.MEDIUM,
) -> Ticket:
    """Create a test ticket."""
    return Ticket.create(
        title=title,
        description="Test description",
        ticket_type=ticket_type,
        priority=priority,
        status=TicketStatus.BACKLOG,
    )


class TestPlanApprovalWidget:
    """Tests for PlanApprovalWidget."""

    async def test_compose_creates_structure(self):
        """Test that compose creates the expected structure."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket()]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            # Check structure
            assert widget.query_one(".plan-header")
            assert widget.query_one("#ticket-list")
            assert widget.query_one("#btn-approve")
            assert widget.query_one("#btn-edit")
            assert widget.query_one("#btn-dismiss")

    async def test_ticket_row_truncates_long_title(self):
        """Test that long titles are truncated."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            long_title = "A" * 100
            tickets = [create_test_ticket(title=long_title)]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            row = widget.query_one("#ticket-row-0", Static)
            text = str(row.render())
            assert "..." in text

    async def test_keyboard_navigation_up(self):
        """Test keyboard navigation up."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket("T1"), create_test_ticket("T2")]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            widget._selected_index = 1
            event = MagicMock()
            event.key = "up"
            widget.on_key(event)

            assert widget._selected_index == 0
            assert event.stop.called

    async def test_keyboard_navigation_down(self):
        """Test keyboard navigation down."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket("T1"), create_test_ticket("T2")]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            widget._selected_index = 0
            event = MagicMock()
            event.key = "down"
            widget.on_key(event)

            assert widget._selected_index == 1
            assert event.stop.called

    async def test_keyboard_navigation_k_j_vim_bindings(self):
        """Test vim-style k/j navigation."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket("T1"), create_test_ticket("T2")]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            # j goes down
            event = MagicMock()
            event.key = "j"
            widget.on_key(event)
            assert widget._selected_index == 1

            # k goes up
            event = MagicMock()
            event.key = "k"
            widget.on_key(event)
            assert widget._selected_index == 0

    async def test_enter_shows_preview(self):
        """Test enter key shows preview notification."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket()]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            event = MagicMock()
            event.key = "enter"
            widget.on_key(event)

            assert event.stop.called

    async def test_action_approve_posts_message_and_removes(self):
        """Test action_approve posts Approved message and removes widget."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket()]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            capture = MessageCapture()
            with patch.object(widget, "post_message", capture):
                widget.action_approve()

            msg = capture.assert_contains(PlanApprovalWidget.Approved)
            assert msg.tickets == tickets

    async def test_action_edit_posts_message(self):
        """Test action_edit posts EditRequested message."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket()]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            capture = MessageCapture()
            with patch.object(widget, "post_message", capture):
                widget.action_edit()

            capture.assert_contains(PlanApprovalWidget.EditRequested)

    async def test_action_dismiss_posts_message(self):
        """Test action_dismiss posts Dismissed message."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket()]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            capture = MessageCapture()
            with patch.object(widget, "post_message", capture):
                widget.action_dismiss()

            capture.assert_contains(PlanApprovalWidget.Dismissed)

    async def test_button_pressed_approve(self):
        """Test button press for approve."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket()]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            capture = MessageCapture()

            from textual.widgets import Button

            btn = widget.query_one("#btn-approve", Button)
            event = MagicMock()
            event.button = btn
            with patch.object(widget, "post_message", capture):
                widget.on_button_pressed(event)

            assert event.stop.called
            capture.assert_contains(PlanApprovalWidget.Approved)

    async def test_button_pressed_edit(self):
        """Test button press for edit."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket()]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            capture = MessageCapture()

            from textual.widgets import Button

            btn = widget.query_one("#btn-edit", Button)
            event = MagicMock()
            event.button = btn
            with patch.object(widget, "post_message", capture):
                widget.on_button_pressed(event)

            capture.assert_contains(PlanApprovalWidget.EditRequested)

    async def test_button_pressed_dismiss(self):
        """Test button press for dismiss."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket()]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            capture = MessageCapture()

            from textual.widgets import Button

            btn = widget.query_one("#btn-dismiss", Button)
            event = MagicMock()
            event.button = btn
            with patch.object(widget, "post_message", capture):
                widget.on_button_pressed(event)

            capture.assert_contains(PlanApprovalWidget.Dismissed)

    async def test_show_preview_with_pair_ticket(self):
        """Test show preview displays correct info for PAIR ticket."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket(ticket_type=TicketType.PAIR)]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            # Just verify it doesn't crash
            widget._show_preview()

    async def test_show_preview_with_empty_acceptance_criteria(self):
        """Test show preview handles empty acceptance criteria."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            ticket = create_test_ticket()
            ticket.acceptance_criteria = []
            widget = PlanApprovalWidget([ticket])
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            # Should not crash
            widget._show_preview()

    async def test_show_preview_out_of_bounds(self):
        """Test show preview handles out of bounds index."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket()]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            widget._selected_index = 10  # Out of bounds
            widget._show_preview()  # Should not crash

    async def test_navigation_boundary_up(self):
        """Test navigation doesn't go below 0."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket(), create_test_ticket()]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            widget._selected_index = 0
            event = MagicMock()
            event.key = "up"
            widget.on_key(event)

            assert widget._selected_index == 0  # Stays at 0

    async def test_navigation_boundary_down(self):
        """Test navigation doesn't go past last item."""
        app = PlanApprovalTestApp()
        async with app.run_test() as pilot:
            tickets = [create_test_ticket(), create_test_ticket()]
            widget = PlanApprovalWidget(tickets)
            await app.query_one("#container").mount(widget)
            await pilot.pause()

            widget._selected_index = 1
            event = MagicMock()
            event.key = "down"
            widget.on_key(event)

            assert widget._selected_index == 1  # Stays at last


# =============================================================================
# ThinkingIndicator Tests
# =============================================================================


class ThinkingIndicatorTestApp(App):
    """Test app for ThinkingIndicator."""

    def compose(self) -> ComposeResult:
        yield Vertical(id="container")


class TestThinkingIndicator:
    """Tests for ThinkingIndicator widget."""

    async def test_animation_starts_on_mount(self):
        """Test that animation starts when mounted."""
        app = ThinkingIndicatorTestApp()
        async with app.run_test() as pilot:
            indicator = ThinkingIndicator()
            await app.query_one("#container").mount(indicator)
            await pilot.pause()

            assert indicator._timer is not None

    async def test_animation_stops_on_unmount(self):
        """Test that animation stops when unmounted."""
        app = ThinkingIndicatorTestApp()
        async with app.run_test() as pilot:
            indicator = ThinkingIndicator()
            await app.query_one("#container").mount(indicator)
            await pilot.pause()

            _timer = indicator._timer
            await indicator.remove()
            await pilot.pause()

            # Timer should have been stopped (though we can't easily check)
            assert True  # Just verify no crash

    async def test_next_frame_cycles(self):
        """Test that _next_frame cycles through frames."""
        app = ThinkingIndicatorTestApp()
        async with app.run_test() as pilot:
            indicator = ThinkingIndicator()
            await app.query_one("#container").mount(indicator)
            await pilot.pause()

            initial_index = indicator._frame_index
            indicator._next_frame()
            assert indicator._frame_index == (initial_index + 1) % 4


# =============================================================================
# StreamingOutput Additional Tests
# =============================================================================


class StreamingOutputTestApp(App):
    """Test app for StreamingOutput."""

    def compose(self) -> ComposeResult:
        yield Vertical(id="container")


class TestStreamingOutputExtended:
    """Additional tests for StreamingOutput widget."""

    async def test_flush_xml_buffer_with_content(self):
        """Test flush_xml_buffer returns non-XML content."""
        app = StreamingOutputTestApp()
        async with app.run_test() as pilot:
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await pilot.pause()

            # Manually set buffer with incomplete content
            output._xml_buffer = "some text without xml"
            result = output.flush_xml_buffer()

            assert result == "some text without xml"
            assert output._xml_buffer == ""

    async def test_flush_xml_buffer_empty(self):
        """Test flush_xml_buffer with empty buffer."""
        app = StreamingOutputTestApp()
        async with app.run_test() as pilot:
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await pilot.pause()

            result = output.flush_xml_buffer()
            assert result == ""

    async def test_flush_xml_buffer_discards_incomplete_xml(self):
        """Test flush_xml_buffer discards incomplete XML tags."""
        app = StreamingOutputTestApp()
        async with app.run_test() as pilot:
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await pilot.pause()

            output._xml_buffer = "<todos incomplete"
            result = output.flush_xml_buffer()

            assert result == ""

    async def test_post_tool_call_generates_id_for_unknown(self):
        """Test post_tool_call generates ID for unknown tool_id."""
        app = StreamingOutputTestApp()
        async with app.run_test() as pilot:
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await pilot.pause()

            await output.post_tool_call("unknown", "Test Tool", "bash")
            await pilot.pause()

            # Check that a tool was added with auto-generated ID
            assert len(output._tool_calls) == 1
            tool_id = next(iter(output._tool_calls.keys()))
            assert tool_id.startswith("auto-")

    async def test_post_tool_call_with_empty_id(self):
        """Test post_tool_call handles empty tool_id."""
        app = StreamingOutputTestApp()
        async with app.run_test() as pilot:
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await pilot.pause()

            await output.post_tool_call("", "Test Tool", "bash")
            await pilot.pause()

            tool_id = next(iter(output._tool_calls.keys()))
            assert tool_id.startswith("auto-")

    async def test_update_tool_status_existing(self):
        """Test update_tool_status updates existing tool."""
        app = StreamingOutputTestApp()
        async with app.run_test() as pilot:
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await pilot.pause()

            await output.post_tool_call("tool-1", "Test Tool", "bash")
            output.update_tool_status("tool-1", "completed")

            assert output._tool_calls["tool-1"].tool_call["status"] == "completed"

    async def test_update_tool_status_nonexistent(self):
        """Test update_tool_status ignores nonexistent tool."""
        app = StreamingOutputTestApp()
        async with app.run_test() as pilot:
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await pilot.pause()

            # Should not raise
            output.update_tool_status("nonexistent", "completed")

    async def test_post_note(self):
        """Test post_note adds static widget."""
        app = StreamingOutputTestApp()
        async with app.run_test() as pilot:
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await pilot.pause()

            await output.post_note("Test note", "test-class")
            await pilot.pause()

            notes = output.query(".streaming-note")
            assert len(notes) == 1

    async def test_post_turn_separator(self):
        """Test post_turn_separator adds a rule."""
        app = StreamingOutputTestApp()
        async with app.run_test() as pilot:
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await pilot.pause()

            await output.post_turn_separator()
            await pilot.pause()

            from textual.widgets import Rule

            rules = output.query(Rule)
            assert len(rules) == 1

    async def test_reset_turn_clears_state(self):
        """Test reset_turn clears internal state."""
        app = StreamingOutputTestApp()
        async with app.run_test() as pilot:
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await pilot.pause()

            await output.post_response("Test")
            await output.post_thought("Thinking")
            output._xml_buffer = "some buffer"
            output._phase = "streaming"

            output.reset_turn()

            assert output._agent_response is None
            assert output._agent_thought is None
            assert output._plan_display is None
            assert output._xml_buffer == ""
            assert output._phase == "idle"

    async def test_set_phase(self):
        """Test set_phase changes phase."""
        output = StreamingOutput()
        assert output.phase == "idle"

        output.set_phase("streaming")
        assert output.phase == "streaming"

    async def test_get_text_content_with_tool_calls(self):
        """Test get_text_content extracts tool call info."""
        app = StreamingOutputTestApp()
        async with app.run_test() as pilot:
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await pilot.pause()

            await output.post_tool_call("t1", "Run Command", "bash")
            await pilot.pause()

            content = output.get_text_content()
            assert "Run Command" in content

    async def test_filter_xml_content_with_closing_tag(self):
        """Test _filter_xml_content handles complete blocks."""
        output = StreamingOutput()

        result = output._filter_xml_content("Before <todos>content</todos> After")

        assert "<todos>" not in result
        assert "Before" in result
        assert "After" in result

    async def test_filter_xml_content_with_partial_tag(self):
        """Test _filter_xml_content buffers partial tags."""
        output = StreamingOutput()

        # First fragment ends with partial tag
        result1 = output._filter_xml_content("Text <todos")
        assert result1 == "Text "
        assert output._xml_buffer == "<todos"

        # Second fragment completes and closes tag
        result2 = output._filter_xml_content(">stuff</todos>")
        assert result2 == ""
        assert output._xml_buffer == ""

    async def test_post_plan_updates_existing(self):
        """Test post_plan updates existing plan display."""
        app = StreamingOutputTestApp()
        async with app.run_test() as pilot:
            output = StreamingOutput()
            await app.query_one("#container").mount(output)
            await pilot.pause()

            entries = [{"id": "1", "content": "Step 1", "status": "pending"}]
            await output.post_plan(entries)
            await pilot.pause()

            plan1 = output._plan_display

            # Post again with updated entries
            entries2 = [{"id": "1", "content": "Step 1 updated", "status": "done"}]
            await output.post_plan(entries2)
            await pilot.pause()

            # Should be same widget instance, updated
            assert output._plan_display is plan1
