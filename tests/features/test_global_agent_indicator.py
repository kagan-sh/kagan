"""User-facing behavior checks for global agent indicator and switching."""

from __future__ import annotations

from typing import cast

import pytest
from tests.helpers.wait import wait_for_modal, wait_for_screen
from textual.widgets import Label

from kagan.ui.screens.kanban import KanbanScreen
from kagan.ui.screens.planner import PlannerScreen
from kagan.ui.widgets.header import KaganHeader


def _header_agent_text(screen: KanbanScreen | PlannerScreen) -> str:
    header = screen.query_one(KaganHeader)
    return str(header.query_one("#header-agent", Label).content)


@pytest.mark.asyncio
async def test_header_shows_current_global_agent_on_main_screens(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        assert _header_agent_text(kanban) == "AI: Claude"

        kanban.action_open_planner()
        planner = cast("PlannerScreen", await wait_for_screen(pilot, PlannerScreen, timeout=10.0))
        assert _header_agent_text(planner) == "AI: Claude"


@pytest.mark.asyncio
async def test_switch_global_agent_updates_indicator_and_config(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    from kagan.ui.modals.global_agent_picker import GlobalAgentPickerModal

    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        assert _header_agent_text(kanban) == "AI: Claude"

        await pilot.press("A")
        modal = cast(
            "GlobalAgentPickerModal",
            await wait_for_modal(pilot, GlobalAgentPickerModal, timeout=5.0),
        )
        modal.dismiss("codex")
        await pilot.pause()
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        await pilot.pause()

        kanban = cast("KanbanScreen", pilot.app.screen)
        assert app.config.general.default_worker_agent == "codex"
        assert _header_agent_text(kanban) == "AI: Codex"

        config_text = app.config_path.read_text()
        assert 'default_worker_agent = "codex"' in config_text

        kanban.action_open_planner()
        planner = cast("PlannerScreen", await wait_for_screen(pilot, PlannerScreen, timeout=10.0))
        assert _header_agent_text(planner) == "AI: Codex"
