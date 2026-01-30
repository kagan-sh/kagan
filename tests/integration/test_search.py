"""Search functionality tests."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.ui.widgets.search_bar import SearchBar
from tests.helpers.pages import get_all_visible_tickets

if TYPE_CHECKING:
    from kagan.app import KaganApp

pytestmark = pytest.mark.integration


class TestSearchToggle:
    async def test_slash_shows_search_bar(self, e2e_app_with_tickets: KaganApp):
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            search_bar = pilot.app.screen.query_one("#search-bar", SearchBar)
            assert not search_bar.is_visible

            await pilot.press("slash")
            await pilot.pause()

            assert search_bar.is_visible

    async def test_escape_closes_search(self, e2e_app_with_tickets: KaganApp):
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("slash")
            await pilot.pause()

            search_bar = pilot.app.screen.query_one("#search-bar", SearchBar)
            assert search_bar.is_visible

            await pilot.press("escape")
            await pilot.pause()
            assert not search_bar.is_visible


class TestSearchFiltering:
    async def test_search_filters_by_title(self, e2e_app_with_tickets: KaganApp):
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            initial_count = len(await get_all_visible_tickets(pilot))
            assert initial_count == 3

            await pilot.press("slash")
            await pilot.pause()

            for char in "Backlog":
                await pilot.press(char)
            await pilot.pause()

            visible = await get_all_visible_tickets(pilot)
            assert len(visible) == 1
            assert visible[0].title == "Backlog task"

    async def test_search_is_case_insensitive(self, e2e_app_with_tickets: KaganApp):
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("slash")
            await pilot.pause()

            for char in "backlog":
                await pilot.press(char)
            await pilot.pause()

            visible = await get_all_visible_tickets(pilot)
            assert len(visible) == 1

    async def test_no_match_shows_empty(self, e2e_app_with_tickets: KaganApp):
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("slash")
            await pilot.pause()

            for char in "nonexistent":
                await pilot.press(char)
            await pilot.pause()

            visible = await get_all_visible_tickets(pilot)
            assert len(visible) == 0

    async def test_clearing_search_restores_all(self, e2e_app_with_tickets: KaganApp):
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("slash")
            await pilot.pause()

            for char in "Backlog":
                await pilot.press(char)
            await pilot.pause()
            assert len(await get_all_visible_tickets(pilot)) == 1

            # Clear via backspace
            for _ in range(7):
                await pilot.press("backspace")
            await pilot.pause()

            assert len(await get_all_visible_tickets(pilot)) == 3

    async def test_escape_clears_filter(self, e2e_app_with_tickets: KaganApp):
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("slash")
            await pilot.pause()

            for char in "Backlog":
                await pilot.press(char)
            await pilot.pause()
            assert len(await get_all_visible_tickets(pilot)) == 1

            await pilot.press("escape")
            await pilot.pause()

            assert len(await get_all_visible_tickets(pilot)) == 3
