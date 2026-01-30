"""Tests for PlanDisplay widget."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

import pytest
from textual.app import App, ComposeResult

from kagan.ui.widgets.plan_display import STATUS_ICONS, PlanDisplay, PlanEntry, PlanStatus

if TYPE_CHECKING:
    from kagan.acp.protocol import PlanEntry as PlanEntryType

pytestmark = pytest.mark.e2e


class PlanDisplayTestApp(App[None]):
    """Test app for PlanDisplay."""

    def __init__(self, entries: list[PlanEntryType]) -> None:
        super().__init__()
        self._entries = entries

    def compose(self) -> ComposeResult:
        yield PlanDisplay(self._entries, id="plan-display")


class TestPlanDisplay:
    """Tests for PlanDisplay widget."""

    def test_init_stores_entries(self) -> None:
        entries: list[PlanEntryType] = [{"content": "Task 1", "status": "pending"}]
        widget = PlanDisplay(entries)
        assert widget.entries == entries

    def test_has_entries_true_when_not_empty(self) -> None:
        entries: list[PlanEntryType] = [{"content": "Task 1", "status": "pending"}]
        widget = PlanDisplay(entries)
        assert widget.has_entries is True

    def test_has_entries_false_when_empty(self) -> None:
        widget = PlanDisplay([])
        assert widget.has_entries is False

    async def test_update_entries_replaces_list(self) -> None:
        initial: list[PlanEntryType] = [{"content": "Old", "status": "pending"}]
        app = PlanDisplayTestApp(initial)
        async with app.run_test():
            plan_display = app.query_one("#plan-display", PlanDisplay)
            new_entries: list[PlanEntryType] = [{"content": "New", "status": "completed"}]
            plan_display.update_entries(new_entries)
            assert plan_display.entries == new_entries

    def test_update_entry_status_valid_index(self) -> None:
        entries: list[PlanEntryType] = [
            {"content": "Task 1", "status": "pending"},
            {"content": "Task 2", "status": "pending"},
        ]
        widget = PlanDisplay(entries)
        widget.update_entry_status(0, "completed")
        assert widget.entries[0].get("status") == "completed"
        assert widget.entries[1].get("status") == "pending"

    def test_update_entry_status_invalid_index_no_error(self) -> None:
        entries: list[PlanEntryType] = [{"content": "Task 1", "status": "pending"}]
        widget = PlanDisplay(entries)
        # Should not raise
        widget.update_entry_status(5, "completed")
        widget.update_entry_status(-1, "completed")
        assert widget.entries[0].get("status") == "pending"


class TestPlanEntry:
    """Tests for PlanEntry widget."""

    def test_render_pending(self) -> None:
        entry = PlanEntry(entry_content="Task 1", status="pending")
        result = entry.render()
        assert "○ Task 1" in result

    def test_render_completed(self) -> None:
        entry = PlanEntry(entry_content="Task 2", status="completed")
        result = entry.render()
        assert "● Task 2" in result

    def test_render_in_progress(self) -> None:
        entry = PlanEntry(entry_content="In Progress", status="in_progress")
        result = entry.render()
        assert "◐ In Progress" in result

    def test_render_failed(self) -> None:
        entry = PlanEntry(entry_content="Failed", status="failed")
        result = entry.render()
        assert "✗ Failed" in result

    def test_render_all_statuses(self) -> None:
        test_cases = [
            ("Pending", "pending", "○"),
            ("In Progress", "in_progress", "◐"),
            ("Completed", "completed", "●"),
            ("Failed", "failed", "✗"),
        ]
        for content, status, expected_icon in test_cases:
            entry = PlanEntry(entry_content=content, status=cast("PlanStatus", status))
            result = entry.render()
            assert f"{expected_icon} {content}" in result

    def test_entry_content_property(self) -> None:
        entry = PlanEntry(entry_content="Test content", status="pending")
        assert entry.entry_content == "Test content"

    def test_status_property(self) -> None:
        entry = PlanEntry(entry_content="Test", status="completed")
        assert entry.status == "completed"

    def test_update_status(self) -> None:
        entry = PlanEntry(entry_content="Test", status="pending")
        entry.update_status("completed")
        assert entry.status == "completed"

    def test_default_classes(self) -> None:
        entry = PlanEntry(entry_content="Test", status="pending")
        assert entry.DEFAULT_CLASSES == "plan-entry"


class TestPlanDisplayCompose:
    """Tests for PlanDisplay compose behavior."""

    async def test_composes_plan_entries(self) -> None:
        entries: list[PlanEntryType] = [
            {"content": "Task 1", "status": "pending"},
            {"content": "Task 2", "status": "completed"},
        ]
        app = PlanDisplayTestApp(entries)
        async with app.run_test():
            plan_entries = app.query(PlanEntry)
            assert len(plan_entries) == 2

    async def test_empty_entries_no_children(self) -> None:
        app = PlanDisplayTestApp([])
        async with app.run_test():
            plan_entries = app.query(PlanEntry)
            assert len(plan_entries) == 0

    async def test_update_entries_updates_children(self) -> None:
        initial: list[PlanEntryType] = [{"content": "Old", "status": "pending"}]
        app = PlanDisplayTestApp(initial)
        async with app.run_test() as pilot:
            plan_display = app.query_one("#plan-display", PlanDisplay)
            new_entries: list[PlanEntryType] = [
                {"content": "New 1", "status": "completed"},
                {"content": "New 2", "status": "in_progress"},
            ]
            plan_display.update_entries(new_entries)
            await pilot.pause()
            plan_entries = list(plan_display.query(PlanEntry))
            assert len(plan_entries) == 2


class TestStatusIcons:
    """Tests for STATUS_ICONS constant."""

    def test_all_statuses_have_icons(self) -> None:
        expected = {"pending", "in_progress", "completed", "failed"}
        assert set(STATUS_ICONS.keys()) == expected

    def test_icons_are_single_char(self) -> None:
        for icon in STATUS_ICONS.values():
            assert len(icon) == 1
