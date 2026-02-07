"""Snapshot tests for WelcomeScreen.

These tests cover:
- WelcomeScreen with CWD suggestion banner
- WelcomeScreen default layout (no banner)

Note: Tests are synchronous because pytest-textual-snapshot's snap_compare
internally calls asyncio.run(), which conflicts with async test functions.
"""

from __future__ import annotations

import asyncio
from datetime import UTC, datetime, timedelta
from typing import TYPE_CHECKING, Any

import pytest

from kagan.ui.screens.welcome import WelcomeScreen
from tests.helpers.journey_runner import bundle_snapshots, execute_test_actions
from tests.helpers.mocks import create_fake_tmux
from tests.snapshots.conftest import _normalize_svg

if TYPE_CHECKING:
    from types import SimpleNamespace

    from tests.snapshots.conftest import MockAgentFactory

# Fixed reference times for deterministic snapshot output.
# _format_time() computes ``_utcnow() - last_opened`` so we keep these
# exactly 23 h apart so the rendered label is always "23h ↵".
_FIXED_OPENED = datetime(2025, 1, 1, 0, 0, 0, tzinfo=UTC)
_FIXED_NOW = _FIXED_OPENED + timedelta(hours=23)


class TestWelcomeScreen:
    @pytest.mark.snapshot
    def test_welcome_journey(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snapshot: Any,
        snapshot_terminal_size: tuple[int, int],
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """WelcomeScreen snapshots with and without CWD suggestion."""
        from kagan.app import KaganApp

        sessions: dict[str, Any] = {}
        fake_tmux = create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)

        # Freeze the clock used by _format_time() so the relative time
        # label is always "23h ↵" regardless of when the test executes.
        monkeypatch.setattr("kagan.ui.screens.welcome._utcnow", lambda: _FIXED_NOW)

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            project_root=snapshot_project.root,
            agent_factory=mock_acp_agent_factory,
        )

        async def _pin_last_opened() -> None:
            """Set last_opened_at to the fixed reference time in the DB."""
            from sqlmodel import col, select

            from kagan.adapters.db.schema import Project as DbProject

            session_factory = app.ctx.project_service._session_factory  # type: ignore[attr-defined]
            assert session_factory is not None
            async with session_factory() as session:
                result = await session.execute(
                    select(DbProject).order_by(col(DbProject.created_at).asc())
                )
                project = result.scalars().first()
                if project is not None:
                    project.last_opened_at = _FIXED_OPENED.replace(tzinfo=None)
                    session.add(project)
                    await session.commit()

        async def run_flow() -> dict[str, str]:
            cols, rows = snapshot_terminal_size
            async with app.run_test(headless=True, size=(cols, rows)) as pilot:
                await pilot.pause()

                # Pin last_opened_at *after* the app has initialised
                # (open_project may have set it to datetime.now()).
                await _pin_last_opened()

                await pilot.app.push_screen(
                    WelcomeScreen(
                        suggest_cwd=True,
                        cwd_path="/Users/dev/my-project",
                    )
                )
                await pilot.pause()
                snapshots = await execute_test_actions(pilot, ["shot(cwd_banner)"])
                await pilot.app.pop_screen()
                await pilot.app.push_screen(
                    WelcomeScreen(
                        suggest_cwd=False,
                        cwd_path=None,
                    )
                )
                await pilot.pause()
                snapshots.update(await execute_test_actions(pilot, ["shot(default)"]))
                return snapshots

        snapshots = asyncio.run(run_flow())
        assert snapshots, "No snapshots captured for welcome journey"
        bundle = bundle_snapshots(snapshots, normalizer=_normalize_svg)
        assert snapshot == bundle
