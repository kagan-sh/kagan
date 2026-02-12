from __future__ import annotations

from types import SimpleNamespace
from typing import Any, cast

from kagan.core.config import KaganConfig


def test_tui_preflight_uses_configured_pair_terminal_backend(
    monkeypatch,
    tmp_path,
) -> None:
    import kagan.cli.commands.tui as tui_command
    from kagan import __main__ as cli_main

    captured: dict[str, object] = {}

    async def fake_detect_issues(**kwargs: object) -> SimpleNamespace:
        captured.update(kwargs)
        return SimpleNamespace(issues=[])

    class _FakeApp:
        run_called = False

        def __init__(self, db_path: str) -> None:
            self.db_path = db_path

        def run(self) -> None:
            _FakeApp.run_called = True

    config = KaganConfig()
    config.general.default_pair_terminal_backend = "tmux"

    monkeypatch.setattr(tui_command, "_check_for_updates_gate", lambda: None)
    monkeypatch.setattr(tui_command, "_auto_cleanup_done_workspaces", lambda _db: None)
    monkeypatch.setattr(tui_command, "_ensure_core_ready_for_cli", lambda _db: None)
    monkeypatch.setattr(tui_command, "_display_agent_status", lambda: {"claude": True})
    monkeypatch.setattr("kagan.core.config.KaganConfig.load", lambda _path=None: config)
    monkeypatch.setattr("kagan.core.preflight.detect_issues", fake_detect_issues)
    monkeypatch.setattr(
        "kagan.core.builtin_agents.get_first_available_agent",
        lambda: SimpleNamespace(
            config=SimpleNamespace(name="Claude"),
            install_command="install",
        ),
    )
    monkeypatch.setattr("kagan.tui.app.KaganApp", _FakeApp)

    callback = cast("Any", cli_main.tui.callback)
    callback(
        db=str(tmp_path / "kagan.db"),
        skip_preflight=False,
        skip_update_check=True,
    )

    assert captured["default_pair_terminal_backend"] == "tmux"
    assert _FakeApp.run_called is True
