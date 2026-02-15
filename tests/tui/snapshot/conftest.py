"""Pytest fixtures for snapshot testing.

These fixtures provide:
- Real git repositories
- Real database
- Real filesystem
- Mocked agent CLI (only external dependency)
- Standardized terminal size for consistent snapshots
- SVG snapshot extension for proper .svg file output

Note on pytest-xdist:
    Syrupy works with xdist for reading/writing snapshots. However, when running
    ``--snapshot-update`` with multiple workers, unused snapshot detection is disabled.
    To detect and delete unused snapshots, run with ``-n 0`` (sequential mode).

    Snapshot tests are grouped on the same xdist worker using ``xdist_group`` marker
    to minimize potential race conditions during snapshot writes.

Note on snapshot strategy:
    We render to SVG via Textual, then normalize to semantic text rows and assert
    with default syrupy text snapshots (.ambr). This avoids platform-dependent
    SVG geometry differences while preserving user-visible content.
"""

from __future__ import annotations

import re
import xml.etree.ElementTree as ET
from importlib import import_module
from pathlib import Path, PurePath
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any

import pytest
from tests.helpers.config import write_test_config
from tests.helpers.git import init_git_repo_with_commit
from tests.helpers.mocks import MockAgent, MockAgentFactory

from kagan.core.command_utils import clear_which_cache
from kagan.tui.app import KaganApp

if TYPE_CHECKING:
    from collections.abc import Awaitable, Callable, Iterable

    from _pytest.fixtures import FixtureRequest
    from syrupy.assertion import SnapshotAssertion
    from textual.app import App
    from textual.pilot import Pilot


__all__ = ["MockAgent", "MockAgentFactory"]


def _normalize_svg(svg: str) -> str:
    """Convert SVG snapshots into deterministic, cross-platform text rows.

    Rich/Textual SVG geometry (x/y/textLength/clip IDs) can vary by platform font
    stack and renderer details. For stable snapshots, we preserve semantic text
    content in display order and normalize OS-dependent path/symbol forms.
    """
    svg = re.sub(r"\bterminal-\d+-([\w-]+)", r"terminal-\1", svg)

    root = ET.fromstring(svg)
    nodes: list[tuple[float, float, str]] = []
    for element in root.iter():
        if not element.tag.endswith("text"):
            continue
        raw = "".join(element.itertext())
        if not raw:
            continue
        x_raw = element.attrib.get("x", "0")
        y_raw = element.attrib.get("y", "0")
        try:
            x = float(x_raw)
        except ValueError:
            x = 0.0
        try:
            y = float(y_raw)
        except ValueError:
            y = 0.0
        text = raw.replace("\xa0", " ").replace("\\", "/")
        text = text.replace("ᘚᘛ", "<>")
        text = text.replace("●", "*").replace("○", "o")
        text = re.sub(r"[A-Za-z]:/", "/", text)
        text = re.sub(
            r"(?:/[A-Za-z0-9._-]+)+/(snapshot_project|snapshot_repo_two)",
            r"/.../\1",
            text,
        )
        text = re.sub(r"(?:/[A-Za-z0-9._-]+)+/(kagan\.db|config\.toml)", r"/.../\1", text)
        nodes.append((round(y, 1), x, text))

    nodes.sort(key=lambda item: (item[0], item[1]))

    rows_by_y: dict[float, list[str]] = {}
    for y, _x, text in nodes:
        rows_by_y.setdefault(y, []).append(text)

    lines: list[str] = []
    for y in sorted(rows_by_y):
        line = "".join(rows_by_y[y])
        line = re.sub(r"[ \t]+", " ", line).strip()
        line = re.sub(r"▔{8,}", "▔" * 32, line)
        line = re.sub(r"▁{8,}", "▁" * 32, line)
        line = re.sub(r"─{8,}", "─" * 32, line)
        line = re.sub(r"━{8,}", "━" * 32, line)
        if line:
            lines.append(line)

    return "\n".join(lines)


SNAPSHOT_TERMINAL_COLS = 120
SNAPSHOT_TERMINAL_ROWS = 40


@pytest.fixture(autouse=True)
def _mock_agent_gates_for_ci(monkeypatch: pytest.MonkeyPatch) -> None:
    """Bypass agent-availability gates on CI.

    On CI runners the ``claude`` (or other agent) CLI is not installed, so:

    1. ``AgentHealthServiceImpl._check_agent()`` marks the agent as unavailable
       and PlannerScreen never calls ``_start_planner()``.
    2. ``check_agent_installed()`` returns False, blocking PAIR session flows
       behind an AgentChoiceModal.

    This fixture mocks the two ``shutil.which`` entry-points so that all
    snapshot tests run identically on CI and locally.
    """
    clear_which_cache()
    agent_health_module = import_module("kagan.core.services.agent_health")
    agents_installer_module = import_module("kagan.core.agents.installer")
    monkeypatch.setattr(
        agent_health_module.shutil,
        "which",
        lambda _cmd, *_a, **_kw: "/usr/bin/mock",
    )
    monkeypatch.setattr(
        agents_installer_module.shutil,
        "which",
        lambda _cmd, *_a, **_kw: "/usr/bin/mock",
    )


@pytest.fixture(autouse=True)
def _snapshot_color_env(monkeypatch: pytest.MonkeyPatch) -> None:
    """Stabilize snapshot rendering colors across the full test suite."""
    monkeypatch.delenv("NO_COLOR", raising=False)
    monkeypatch.setenv("TEXTUAL_COLOR_SYSTEM", "truecolor")
    monkeypatch.setenv("COLORTERM", "truecolor")
    monkeypatch.setenv("TZ", "UTC")
    monkeypatch.setenv("LC_ALL", "C.UTF-8")
    monkeypatch.setenv("LANG", "C.UTF-8")


@pytest.fixture(autouse=True)
def _snapshot_platform_stability(monkeypatch: pytest.MonkeyPatch) -> None:
    """Force snapshot rendering through one platform code path."""
    monkeypatch.setattr("platform.system", lambda: "Linux")
    monkeypatch.setattr("kagan.core.command_utils.is_windows", lambda: False)


@pytest.fixture
def snap_compare(
    snapshot: SnapshotAssertion, request: FixtureRequest
) -> Callable[[str | PurePath | App[Any]], bool]:
    """Compare Textual app screenshots with stored SVG snapshots."""

    def compare(
        app: str | PurePath | App[Any],
        press: Iterable[str] = (),
        terminal_size: tuple[int, int] = (80, 24),
        run_before: Callable[[Pilot], Awaitable[None] | None] | None = None,
    ) -> bool:
        """Compare current app screenshot with stored snapshot."""
        from textual._doc import take_svg_screenshot
        from textual._import_app import import_app
        from textual.app import App as TextualApp

        if isinstance(app, TextualApp):
            app_instance = app
        else:
            path = Path(app)
            if path.is_absolute():
                app_path = str(path.resolve())
            else:
                node_path = request.node.path.parent
                app_path = str((node_path / app).resolve())
            app_instance = import_app(app_path)

        actual_screenshot = take_svg_screenshot(
            app=app_instance,
            press=press,
            terminal_size=terminal_size,
            run_before=run_before,
        )

        normalized_screenshot = _normalize_svg(actual_screenshot)
        return snapshot == normalized_screenshot

    return compare


@pytest.fixture
async def snapshot_project(tmp_path: Path) -> SimpleNamespace:
    """Create a real project with git repo and kagan config for snapshot testing."""
    from kagan.core.adapters.db.repositories import RepoRepository, TaskRepository

    project = tmp_path / "snapshot_project"
    project.mkdir()

    await init_git_repo_with_commit(project)

    config_dir = tmp_path / "kagan-config"
    config_dir.mkdir()
    data_dir = tmp_path / "kagan-data"
    data_dir.mkdir()

    config_path = config_dir / "config.toml"
    write_test_config(
        config_path,
        auto_review=False,
        header_comment="Kagan Snapshot Test Configuration",
    )

    db_path = str(data_dir / "kagan.db")

    task_repo = TaskRepository(db_path, project_root=project)
    await task_repo.initialize()

    project_id = await task_repo.ensure_test_project("Snapshot Test Project")

    assert task_repo._session_factory is not None
    repo_repo = RepoRepository(task_repo._session_factory)
    repo, _ = await repo_repo.get_or_create(project, default_branch="main")
    if repo.id:
        await repo_repo.update_default_branch(repo.id, "main", mark_configured=True)
        await repo_repo.add_to_project(project_id, repo.id, is_primary=True)

    await task_repo.close()

    return SimpleNamespace(
        root=project,
        db=db_path,
        config=str(config_path),
    )


@pytest.fixture
def mock_acp_agent_factory() -> MockAgentFactory:
    """Factory that returns mock agents with controllable responses."""
    return MockAgentFactory()


@pytest.fixture
async def snapshot_app(
    snapshot_project: SimpleNamespace,
    mock_acp_agent_factory: MockAgentFactory,
    global_mock_tmux: dict[str, dict[str, Any]],
) -> KaganApp:
    """Create KaganApp with real DB, real git, but mocked agent factory."""
    del global_mock_tmux

    app = KaganApp(
        db_path=snapshot_project.db,
        config_path=snapshot_project.config,
        project_root=snapshot_project.root,
        agent_factory=mock_acp_agent_factory,
    )

    return app


@pytest.fixture
def snapshot_terminal_size() -> tuple[int, int]:
    """Return standardized terminal size for snapshots (cols, rows)."""
    return (SNAPSHOT_TERMINAL_COLS, SNAPSHOT_TERMINAL_ROWS)


def pytest_collection_modifyitems(items: list[pytest.Item]) -> None:
    """Automatically group snapshot tests on the same xdist worker."""
    for item in items:
        if item.get_closest_marker("snapshot"):
            item.add_marker(pytest.mark.xdist_group(name="snapshots"))
