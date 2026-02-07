from __future__ import annotations

import asyncio
from datetime import datetime
from typing import TYPE_CHECKING, Any

import pytest
from syrupy.extensions.image import SVGImageSnapshotExtension

from kagan.adapters.db.repositories import TaskRepository
from kagan.adapters.db.schema import Task
from kagan.app import KaganApp
from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType
from tests.helpers.journey_runner import execute_test_actions
from tests.helpers.wait import wait_for_screen
from tests.snapshots.conftest import _normalize_svg

if TYPE_CHECKING:
    from types import SimpleNamespace

SNAPSHOT_TIME = datetime(2024, 1, 1, 12, 0, 0)


async def _seed_task(db_path: str) -> None:
    repo = TaskRepository(db_path)
    await repo.initialize()
    project_id = repo.default_project_id
    if project_id is None:
        project_id = await repo.ensure_test_project("Settings Test")
    await repo.create(
        Task(
            id="settings-seed",
            project_id=project_id,
            title="Seed task",
            description="Ensures the app opens to KanbanScreen.",
            priority=TaskPriority.LOW,
            status=TaskStatus.BACKLOG,
            task_type=TaskType.PAIR,
            created_at=SNAPSHOT_TIME,
            updated_at=SNAPSHOT_TIME,
        )
    )
    await repo.close()


class TestSettingsModal:
    @pytest.mark.snapshot
    def test_settings_modal_default(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: Any,
        snapshot_terminal_size: tuple[int, int],
        snapshot: Any,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        from tests.helpers.mocks import create_fake_tmux

        asyncio.run(_seed_task(snapshot_project.db))

        sessions: dict[str, dict[str, Any]] = {}
        fake_tmux = create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            project_root=snapshot_project.root,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_flow() -> dict[str, str]:
            from kagan.ui.modals.settings import SettingsModal
            from kagan.ui.screens.kanban import KanbanScreen

            cols, rows = snapshot_terminal_size
            async with app.run_test(headless=True, size=(cols, rows)) as pilot:
                await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
                await pilot.pause()

                await pilot.app.screen.action_open_settings()  # type: ignore[attr-defined]
                await wait_for_screen(pilot, SettingsModal, timeout=5.0)
                await pilot.pause()

                return await execute_test_actions(pilot, ["shot(settings_modal)"])

        snapshots = asyncio.run(run_flow())
        assert snapshots, "No snapshots captured for settings modal"

        snapshot = snapshot.use_extension(SVGImageSnapshotExtension)
        svg = snapshots.get("settings_modal")
        if svg is None:
            raise AssertionError("Missing settings_modal snapshot")
        snapshot.assert_match(_normalize_svg(svg))
