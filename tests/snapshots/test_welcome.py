"""Snapshot tests for WelcomeScreen.

These tests cover:
- WelcomeScreen with CWD suggestion banner
- WelcomeScreen default layout (no banner)

Note: Tests are synchronous because pytest-textual-snapshot's snap_compare
internally calls asyncio.run(), which conflicts with async test functions.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import pytest

from kagan.ui.screens.welcome import WelcomeScreen

if TYPE_CHECKING:
    from types import SimpleNamespace

    from textual.pilot import Pilot

    from tests.snapshots.conftest import MockAgentFactory


def _create_fake_tmux(sessions: dict[str, Any]) -> object:
    """Create a fake tmux function that tracks session state."""

    async def fake_run_tmux(*args: str) -> str:
        if not args:
            return ""
        command, args_list = args[0], list(args)
        if command == "new-session" and "-s" in args_list:
            idx = args_list.index("-s")
            name = args_list[idx + 1] if idx + 1 < len(args_list) else None
            if name:
                cwd = args_list[args_list.index("-c") + 1] if "-c" in args_list else ""
                env: dict[str, str] = {}
                for i, val in enumerate(args_list):
                    if val == "-e" and i + 1 < len(args_list):
                        key, _, env_value = args_list[i + 1].partition("=")
                        env[key] = env_value
                sessions[name] = {"cwd": cwd, "env": env, "sent_keys": []}
        elif command == "kill-session" and "-t" in args_list:
            sessions.pop(args_list[args_list.index("-t") + 1], None)
        elif command == "send-keys" and "-t" in args_list:
            idx = args_list.index("-t")
            name = args_list[idx + 1]
            keys = args_list[idx + 2] if idx + 2 < len(args_list) else ""
            if name in sessions:
                sessions[name]["sent_keys"].append(keys)
        elif command == "list-sessions":
            return "\n".join(sorted(sessions.keys()))
        return ""

    return fake_run_tmux


class TestWelcomeScreen:
    """Snapshot tests for WelcomeScreen."""

    @pytest.mark.snapshot
    def test_welcome_with_cwd_suggestion(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """WelcomeScreen shows CWD suggestion banner when suggest_cwd=True."""
        from kagan.app import KaganApp

        # Mock tmux
        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            project_root=snapshot_project.root,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Push WelcomeScreen with CWD suggestion
            await pilot.app.push_screen(
                WelcomeScreen(
                    suggest_cwd=True,
                    cwd_path="/Users/dev/my-project",
                )
            )
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_welcome_default_layout(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """WelcomeScreen without CWD banner shows normal layout."""
        from kagan.app import KaganApp

        # Mock tmux
        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            project_root=snapshot_project.root,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Push WelcomeScreen without CWD suggestion
            await pilot.app.push_screen(
                WelcomeScreen(
                    suggest_cwd=False,
                    cwd_path=None,
                )
            )
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)
