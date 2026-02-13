from __future__ import annotations

from dataclasses import dataclass
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, cast

from tests.helpers.config import write_test_config
from tests.helpers.git import init_git_repo_with_commit
from tests.helpers.wait import wait_for_screen, wait_for_widget, wait_until_async
from textual.widgets import Static

from kagan.core.adapters.db.repositories import RepoRepository, TaskRepository
from kagan.core.adapters.db.schema import Task
from kagan.core.models.enums import ExecutionStatus, TaskPriority, TaskStatus, TaskType
from kagan.tui.app import KaganApp
from kagan.tui.ui.modals.review import ReviewModal
from kagan.tui.ui.screens.kanban import KanbanScreen
from kagan.tui.ui.widgets.card import TaskCard
from kagan.tui.ui.widgets.chat_panel import ChatPanel

if TYPE_CHECKING:
    from pathlib import Path

    from textual.pilot import Pilot


@dataclass(frozen=True)
class TaskIds:
    auto_backlog: str
    auto_in_progress: str
    pair_in_progress: str
    pair_review: str
    auto_review: str
    auto_done: str


def make_execution_stub(
    execution_id: str,
    *,
    status: ExecutionStatus,
    metadata: dict[str, Any] | None = None,
) -> Any:
    return SimpleNamespace(id=execution_id, status=status, metadata_=(metadata or {}))


def make_execution_log_entry(logs: str) -> Any:
    return SimpleNamespace(logs=logs)


async def seed_enter_flow_app(tmp_path: Path) -> tuple[KaganApp, TaskIds]:
    repo_path = tmp_path / "repo"
    repo_path.mkdir()
    await init_git_repo_with_commit(repo_path)

    config_path = write_test_config(
        tmp_path / "config.toml",
        auto_review=True,
        skip_pair_instructions=True,
        default_pair_terminal_backend="tmux",
    )

    db_path = tmp_path / "kagan.db"
    manager = TaskRepository(db_path, project_root=repo_path)
    await manager.initialize()
    project_id = await manager.ensure_test_project("Enter Flow")

    assert manager._session_factory is not None
    repo_repo = RepoRepository(manager._session_factory)
    repo, _ = await repo_repo.get_or_create(repo_path, default_branch="main")
    if repo.id:
        await repo_repo.update_default_branch(repo.id, "main", mark_configured=True)
        await repo_repo.add_to_project(project_id, repo.id, is_primary=True)

    tasks = [
        Task(
            id="auto0001",
            project_id=project_id,
            title="AUTO backlog",
            description="",
            priority=TaskPriority.MEDIUM,
            status=TaskStatus.BACKLOG,
            task_type=TaskType.AUTO,
        ),
        Task(
            id="auto0002",
            project_id=project_id,
            title="AUTO in progress",
            description="",
            priority=TaskPriority.MEDIUM,
            status=TaskStatus.IN_PROGRESS,
            task_type=TaskType.AUTO,
        ),
        Task(
            id="pair0001",
            project_id=project_id,
            title="PAIR in progress",
            description="",
            priority=TaskPriority.MEDIUM,
            status=TaskStatus.IN_PROGRESS,
            task_type=TaskType.PAIR,
        ),
        Task(
            id="prvw0001",
            project_id=project_id,
            title="PAIR review",
            description="",
            priority=TaskPriority.MEDIUM,
            status=TaskStatus.REVIEW,
            task_type=TaskType.PAIR,
        ),
        Task(
            id="revw0001",
            project_id=project_id,
            title="AUTO review",
            description="",
            priority=TaskPriority.MEDIUM,
            status=TaskStatus.REVIEW,
            task_type=TaskType.AUTO,
        ),
        Task(
            id="done0001",
            project_id=project_id,
            title="AUTO done",
            description="",
            priority=TaskPriority.MEDIUM,
            status=TaskStatus.DONE,
            task_type=TaskType.AUTO,
        ),
    ]
    for task in tasks:
        await manager.create(task)
    await manager.close()

    app = KaganApp(
        db_path=str(db_path),
        config_path=str(config_path),
        project_root=repo_path,
    )
    return (
        app,
        TaskIds(
            auto_backlog="auto0001",
            auto_in_progress="auto0002",
            pair_in_progress="pair0001",
            pair_review="prvw0001",
            auto_review="revw0001",
            auto_done="done0001",
        ),
    )


async def focus_task_card(pilot: Pilot, task_id: str) -> None:
    await wait_for_widget(pilot, f"#card-{task_id}", timeout=10.0)
    card = pilot.app.screen.query_one(f"#card-{task_id}", TaskCard)
    card.focus()
    await pilot.pause()


async def open_review_for_task(pilot: Pilot, task_id: str) -> ReviewModal:
    await focus_task_card(pilot, task_id)
    await pilot.press("enter")
    return cast("ReviewModal", await wait_for_screen(pilot, ReviewModal, timeout=10.0))


def mark_auto_runtime_running(app: KaganApp, task_id: str, agent: Any | None = None) -> None:
    runtime = app.ctx.runtime_service
    runtime.mark_started(task_id)
    runtime.set_execution(task_id, None, 0)
    if agent is not None:
        runtime.attach_running_agent(task_id, cast("Any", agent))


async def wait_chat_contains(
    pilot: Pilot,
    selector: str,
    expected_text: str,
    *,
    timeout: float = 3.0,
) -> str:
    rendered = ""

    async def _has_expected_text() -> bool:
        nonlocal rendered
        await pilot.pause()
        rendered = pilot.app.screen.query_one(selector, ChatPanel).output.get_text_content()
        return expected_text in rendered

    await wait_until_async(
        _has_expected_text,
        timeout=timeout,
        check_interval=0.05,
        description=f"chat panel {selector} to contain '{expected_text}'",
    )
    return rendered


async def wait_static_contains(
    pilot: Pilot,
    selector: str,
    expected_text: str,
    *,
    timeout: float = 3.0,
) -> str:
    rendered = ""

    async def _has_expected_text() -> bool:
        nonlocal rendered
        await pilot.pause()
        rendered = str(pilot.app.screen.query_one(selector, Static).render())
        return expected_text in rendered

    await wait_until_async(
        _has_expected_text,
        timeout=timeout,
        check_interval=0.05,
        description=f"static widget {selector} to contain '{expected_text}'",
    )
    return rendered


async def open_kanban(pilot: Pilot) -> KanbanScreen:
    return cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
