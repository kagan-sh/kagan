"""CLI tests for TUI core auto-start behavior."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, cast

from kagan.core.config import KaganConfig

if TYPE_CHECKING:
    from pathlib import Path


def test_tui_autostarts_core_before_app_run(monkeypatch, tmp_path: Path) -> None:
    """`kagan tui` auto-starts core when `general.core_autostart=true`."""
    import kagan.cli.commands.tui as tui_command
    from kagan import __main__ as cli_main

    started: dict[str, object] = {}

    class _FakeApp:
        run_called = False

        def __init__(self, db_path: str) -> None:
            self.db_path = db_path

        def run(self) -> None:
            _FakeApp.run_called = True

    config = KaganConfig()
    config.general.core_autostart = True

    def _fake_ensure(*, config: KaganConfig, config_path: Path, db_path: Path) -> object:
        started["config"] = config
        started["config_path"] = config_path
        started["db_path"] = db_path
        return object()

    monkeypatch.setattr(tui_command, "_check_for_updates_gate", lambda: None)
    monkeypatch.setattr("kagan.core.config.KaganConfig.load", lambda _path=None: config)
    monkeypatch.setattr("kagan.core.launcher.ensure_core_running_sync", _fake_ensure)
    monkeypatch.setattr("kagan.tui.app.KaganApp", _FakeApp)

    callback = cast("Any", cli_main.tui.callback)
    db_file = tmp_path / "kagan.db"
    callback(
        db=str(db_file),
        skip_preflight=True,
        skip_update_check=True,
    )

    assert started["config"] is config
    assert started["db_path"] == db_file
    assert _FakeApp.run_called is True
