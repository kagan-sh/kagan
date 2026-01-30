"""Tests for RejectionInputModal."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest
from textual.widgets import Button, TextArea

from kagan.ui.modals.rejection_input import RejectionInputModal
from tests.helpers.pages import is_on_screen, navigate_to_kanban

if TYPE_CHECKING:
    from kagan.app import KaganApp

pytestmark = pytest.mark.e2e


class TestRejectionInputOpen:
    """Test modal opens with expected widgets."""

    async def test_modal_has_expected_widgets(self, e2e_app: KaganApp):
        """Modal contains TextArea and buttons."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)

            modal = RejectionInputModal(ticket_title="Test Ticket")
            pilot.app.push_screen(modal)
            await pilot.pause()

            assert is_on_screen(pilot, "RejectionInputModal")
            assert pilot.app.screen.query_one("#feedback-input", TextArea)
            assert pilot.app.screen.query_one("#submit-btn", Button)
            assert pilot.app.screen.query_one("#cancel-btn", Button)

    async def test_modal_focuses_textarea_on_mount(self, e2e_app: KaganApp):
        """TextArea is focused when modal opens."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)

            modal = RejectionInputModal(ticket_title="Test Ticket")
            pilot.app.push_screen(modal)
            await pilot.pause()

            focused = pilot.app.focused
            assert isinstance(focused, TextArea)
            assert focused.id == "feedback-input"


class TestRejectionInputSubmit:
    """Test submit behavior."""

    async def test_submit_button_dismisses_with_feedback(self, e2e_app: KaganApp):
        """Clicking submit returns feedback text."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)

            result = None

            def capture_result(value):
                nonlocal result
                result = value

            modal = RejectionInputModal(ticket_title="Test Ticket")
            pilot.app.push_screen(modal, capture_result)
            await pilot.pause()

            text_area = pilot.app.screen.query_one("#feedback-input", TextArea)
            text_area.insert("Fix the bug")
            await pilot.pause()

            await pilot.click("#submit-btn")
            await pilot.pause()

            assert result == "Fix the bug"

    async def test_ctrl_s_submits(self, e2e_app: KaganApp):
        """Ctrl+S submits the feedback."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)

            result = None

            def capture_result(value):
                nonlocal result
                result = value

            modal = RejectionInputModal(ticket_title="Test Ticket")
            pilot.app.push_screen(modal, capture_result)
            await pilot.pause()

            text_area = pilot.app.screen.query_one("#feedback-input", TextArea)
            text_area.insert("Use ctrl+s")
            await pilot.pause()

            await pilot.press("ctrl+s")
            await pilot.pause()

            assert result == "Use ctrl+s"

    async def test_empty_feedback_returns_none(self, e2e_app: KaganApp):
        """Empty feedback returns None on submit."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)

            result: str | None = "sentinel"

            def capture_result(value: str | None) -> None:
                nonlocal result
                result = value

            modal = RejectionInputModal(ticket_title="Test Ticket")
            pilot.app.push_screen(modal, capture_result)
            await pilot.pause()

            await pilot.click("#submit-btn")
            await pilot.pause()

            assert result is None


class TestRejectionInputCancel:
    """Test cancel/escape behavior."""

    async def test_escape_cancels(self, e2e_app: KaganApp):
        """Escape dismisses with None."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)

            result: str | None = "sentinel"

            def capture_result(value: str | None) -> None:
                nonlocal result
                result = value

            modal = RejectionInputModal(ticket_title="Test Ticket")
            pilot.app.push_screen(modal, capture_result)
            await pilot.pause()

            text_area = pilot.app.screen.query_one("#feedback-input", TextArea)
            text_area.insert("This will be discarded")
            await pilot.pause()

            await pilot.press("escape")
            await pilot.pause()

            assert result is None

    async def test_cancel_button_dismisses(self, e2e_app: KaganApp):
        """Cancel button dismisses with None."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)

            result: str | None = "sentinel"

            def capture_result(value: str | None) -> None:
                nonlocal result
                result = value

            modal = RejectionInputModal(ticket_title="Test Ticket")
            pilot.app.push_screen(modal, capture_result)
            await pilot.pause()

            await pilot.click("#cancel-btn")
            await pilot.pause()

            assert result is None
