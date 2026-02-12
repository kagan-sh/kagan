"""Regression test: local/source installs must not attempt PyPI auto-update.

When kagan is installed from a local path (e.g. ``uv tool install .``), the
auto-updater previously ran ``uv tool upgrade kagan==X.Y.Z`` which failed
because uv tried to resolve from PyPI instead of the local source.

The fix detects local installs via PEP 610 ``direct_url.json`` metadata and
skips the update entirely.
"""

from __future__ import annotations

import json
import subprocess

import pytest

from kagan.cli.update import check_for_updates, prompt_and_update


def test_local_install_never_runs_upgrade_subprocess(monkeypatch: pytest.MonkeyPatch) -> None:
    """A local/file-source install must never spawn an upgrade subprocess,
    even when PyPI has a newer version available."""

    # Simulate a local install: direct_url.json points to a file:// URL
    direct_url_payload = json.dumps(
        {"url": "file:///home/user/projects/kagan", "dir_info": {"editable": False}}
    )

    class FakeDist:
        def read_text(self, name: str) -> str | None:
            if name == "direct_url.json":
                return direct_url_payload
            if name == "INSTALLER":
                return "uv"
            return None

        def locate_file(self, path: str) -> str:
            return "/home/user/.local/share/uv/tools/kagan/lib/python3.13/site-packages"

    monkeypatch.setattr("kagan.cli.update.distribution", lambda _name: FakeDist())
    monkeypatch.setattr("kagan.cli.update.__version__", "0.3.0")

    # Guard: subprocess.run must never be called
    def _boom(*args: object, **kwargs: object) -> None:
        pytest.fail("subprocess.run was called — local install must not spawn upgrade commands")

    monkeypatch.setattr(subprocess, "run", _boom)

    # Run the full check → prompt_and_update chain
    result = check_for_updates()
    updated = prompt_and_update(result, force=True)

    assert updated is False
