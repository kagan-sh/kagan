"""UI-driven multi-project journey focused on project/repo routing contracts."""

from __future__ import annotations

import platform
from contextlib import suppress
from pathlib import Path
from typing import TYPE_CHECKING, Any, cast

import pytest
from tests.helpers.config import write_test_config
from tests.helpers.git import init_git_repo_with_commit
from tests.helpers.mock_responses import SIMPLE_PLAN_TEXT, make_plan_submit_tool_call
from tests.helpers.mocks import build_repo_routed_smart_agent_factory
from tests.helpers.wait import (
    wait_for_planner_ready,
    wait_for_screen,
    wait_for_widget,
    wait_until_async,
)
from textual.widgets import Button, Input, ListView

from kagan.core.models.enums import TaskStatus, TaskType
from kagan.tui.app import KaganApp
from kagan.tui.ui.modals.folder_picker import FolderPickerModal
from kagan.tui.ui.modals.new_project import NewProjectModal
from kagan.tui.ui.screens.kanban import KanbanScreen
from kagan.tui.ui.screens.planner import PlannerInput, PlannerScreen
from kagan.tui.ui.screens.repo_picker import RepoPickerScreen
from kagan.tui.ui.screens.welcome import WelcomeScreen
from kagan.tui.ui.widgets.plan_approval import PlanApprovalWidget

if TYPE_CHECKING:
    from kagan.core.adapters.db.schema import Task


async def _wait_for_active_repo(app: KaganApp, repo_path: Path, *, timeout: float = 5.0) -> None:
    project_id = app.ctx.active_project_id
    if project_id is None:
        raise RuntimeError("Active project not set")
    repos = await app.ctx.project_service.get_project_repos(project_id)
    target = next(
        (repo for repo in repos if Path(repo.path).resolve() == repo_path.resolve()),
        None,
    )
    if target is None:
        raise RuntimeError(f"Repo not found for active project: {repo_path}")

    async def _is_active_repo() -> bool:
        return app.ctx.active_repo_id == target.id

    await wait_until_async(
        _is_active_repo,
        timeout=timeout,
        check_interval=0.1,
        description=f"active repo to update to {repo_path}",
    )


async def _select_repo(pilot, repo_path: Path) -> None:
    pilot.app.action_open_repo_selector()
    await wait_for_screen(pilot, RepoPickerScreen, timeout=10.0)
    await pilot.pause()

    screen = cast("RepoPickerScreen", pilot.app.screen)
    list_view = screen.query_one("#repo-list", ListView)
    target_index = 0
    for idx, item in enumerate(screen._repo_items):
        if Path(item.repo.path).resolve() == repo_path.resolve():
            target_index = idx
            break
    list_view.index = target_index
    await pilot.pause()
    await pilot.press("enter")
    await wait_for_screen(pilot, KanbanScreen)
    await pilot.pause()
    await _wait_for_active_repo(cast("KaganApp", pilot.app), repo_path)


async def _add_repo(pilot, repo_path: Path) -> None:
    pilot.app.action_open_repo_selector()
    await wait_for_screen(pilot, RepoPickerScreen, timeout=10.0)

    screen = cast("RepoPickerScreen", pilot.app.screen)
    screen.query_one("#btn-add-repo", Button).press()
    await wait_for_screen(pilot, FolderPickerModal)

    modal = cast("FolderPickerModal", pilot.app.screen)
    modal.query_one("#path-input", Input).value = str(repo_path)
    await pilot.pause()
    modal.query_one("#btn-open", Button).press()

    await wait_for_screen(pilot, KanbanScreen)
    await pilot.pause()


async def _create_project(pilot, name: str, repo_path: Path) -> str:
    await wait_for_screen(pilot, WelcomeScreen)
    screen = cast("WelcomeScreen", pilot.app.screen)
    screen.action_new_project()
    await wait_for_screen(pilot, NewProjectModal)

    modal = cast("NewProjectModal", pilot.app.screen)
    modal.query_one("#name-input", Input).value = name
    modal.query_one("#path-input", Input).value = str(repo_path)
    await pilot.pause()
    modal.query_one("#btn-create", Button).press()

    await wait_for_screen(pilot, PlannerScreen, timeout=10.0)
    app = cast("KaganApp", pilot.app)
    project_id = app.ctx.active_project_id
    if project_id is None:
        raise RuntimeError("Active project not set after creation")
    return project_id


async def _wait_for_project_item(pilot, name: str, *, timeout: float = 10.0) -> None:
    async def _project_exists() -> bool:
        await pilot.pause()
        screen = pilot.app.screen
        if isinstance(screen, WelcomeScreen):
            return any(item.project_name == name for item in screen._project_items)
        return False

    try:
        await wait_until_async(
            _project_exists,
            timeout=timeout,
            check_interval=0.1,
            description=f"project item {name} to appear",
        )
    except TimeoutError as exc:
        raise TimeoutError(f"Project not found in welcome list: {name}") from exc


async def _open_project(pilot, name: str, repo_path: Path | None = None) -> None:
    await pilot.app.action_open_project_selector()
    await wait_for_screen(pilot, WelcomeScreen)
    await pilot.pause()

    screen = cast("WelcomeScreen", pilot.app.screen)
    list_view = screen.query_one("#project-list", ListView)
    await _wait_for_project_item(pilot, name)
    target_index = 0
    for idx, item in enumerate(screen._project_items):
        if item.project_name == name:
            target_index = idx
            break
    list_view.index = target_index
    await pilot.pause()
    await pilot.press("enter")

    async def _project_opened() -> bool:
        await pilot.pause()
        return isinstance(pilot.app.screen, (RepoPickerScreen, KanbanScreen))

    with suppress(TimeoutError):
        await wait_until_async(
            _project_opened,
            timeout=10.0,
            check_interval=0.1,
            description="project to open to repo picker or kanban",
        )

    if isinstance(pilot.app.screen, RepoPickerScreen):
        rp_screen = pilot.app.screen
        repo_list = rp_screen.query_one("#repo-list", ListView)
        target_index = 0
        if repo_path is not None:
            for idx, item in enumerate(rp_screen._repo_items):
                if Path(item.repo.path).resolve() == repo_path.resolve():
                    target_index = idx
                    break
        repo_list.index = target_index
        await pilot.pause()
        await pilot.press("enter")

    await wait_for_screen(pilot, KanbanScreen, timeout=10.0)


async def _create_auto_task_from_planner(pilot, prompt: str) -> None:
    await wait_for_planner_ready(pilot, timeout=10.0)
    planner = cast("PlannerScreen", pilot.app.screen)
    planner_input = planner.query_one("#planner-input", PlannerInput)
    planner_input.text = prompt
    await pilot.pause()
    await pilot.press("enter")

    await wait_for_widget(pilot, "PlanApprovalWidget", timeout=20.0)

    plan_widget = pilot.app.screen.query_one(PlanApprovalWidget)
    plan_widget.focus()
    await pilot.pause()
    plan_widget.action_approve()
    await wait_for_screen(pilot, KanbanScreen, timeout=20.0)


async def _open_planner(pilot) -> None:
    screen = cast("KanbanScreen", pilot.app.screen)
    screen.action_open_planner()
    await wait_for_screen(pilot, PlannerScreen, timeout=10.0)


async def _get_task_by_title(app: KaganApp, project_id: str, title: str) -> Task:
    tasks = await app.ctx.task_service.list_tasks(project_id=project_id)
    for task in tasks:
        if task.title == title:
            return task
    raise RuntimeError(f"Task not found: {title}")


@pytest.mark.asyncio
@pytest.mark.skipif(
    platform.system() == "Windows",
    reason="Heavy E2E journey unreliable on Windows CI (AppContext timing)",
)
async def test_multi_project_ui_journey(tmp_path: Path) -> None:
    """Validate project/repo switching and planner task routing across projects."""
    xdg_config = tmp_path / "xdg-config" / "kagan"
    xdg_data = tmp_path / "xdg-data" / "kagan"
    xdg_config.mkdir(parents=True)
    xdg_data.mkdir(parents=True)

    project_a_repo1 = tmp_path / "project-a-repo-1"
    project_a_repo2 = tmp_path / "project-a-repo-2"
    project_b_repo1 = tmp_path / "project-b-repo-1"
    for repo in (project_a_repo1, project_a_repo2, project_b_repo1):
        repo.mkdir()
        await init_git_repo_with_commit(repo)

    config_path = write_test_config(xdg_config / "config.toml", auto_review=False)
    db_path = xdg_data / "kagan.db"

    def build_plan(repo_path: Path) -> dict[str, Any]:
        return make_plan_submit_tool_call(
            tool_call_id=f"tc-plan-{repo_path.name}",
            tasks=[
                {
                    "title": f"AUTO: {repo_path.name}",
                    "type": "AUTO",
                    "description": f"Auto task for {repo_path.name}",
                    "acceptance_criteria": ["Task created in backlog"],
                    "priority": "low",
                }
            ],
            todos=[{"content": "Propose plan", "status": "completed"}],
        )

    routes_by_repo = {
        str(project_a_repo1): {"plan_submit": (SIMPLE_PLAN_TEXT, build_plan(project_a_repo1))},
        str(project_a_repo2): {"plan_submit": (SIMPLE_PLAN_TEXT, build_plan(project_a_repo2))},
        str(project_b_repo1): {"plan_submit": (SIMPLE_PLAN_TEXT, build_plan(project_b_repo1))},
    }

    app = KaganApp(
        db_path=str(db_path),
        config_path=str(config_path),
        project_root=tmp_path,
        agent_factory=build_repo_routed_smart_agent_factory(
            routes_by_repo,
            default=(SIMPLE_PLAN_TEXT, {}),
        ),
    )

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, WelcomeScreen, timeout=10.0)

        kagan_app = cast("KaganApp", pilot.app)

        project_a_id = await _create_project(pilot, "Project A", project_a_repo1)
        await _create_auto_task_from_planner(pilot, "Auto task for repo A1")

        await _add_repo(pilot, project_a_repo2)
        await _select_repo(pilot, project_a_repo2)
        await _open_planner(pilot)
        await _create_auto_task_from_planner(pilot, "Auto task for repo A2")

        await pilot.app.action_open_project_selector()
        project_b_id = await _create_project(pilot, "Project B", project_b_repo1)
        await _create_auto_task_from_planner(pilot, "Auto task for repo B1")

        await _open_project(pilot, "Project A", project_a_repo2)
        await _wait_for_active_repo(kagan_app, project_a_repo2)

        await _open_project(pilot, "Project B", project_b_repo1)
        await _wait_for_active_repo(kagan_app, project_b_repo1)

        task_a1 = await _get_task_by_title(kagan_app, project_a_id, f"AUTO: {project_a_repo1.name}")
        task_a2 = await _get_task_by_title(kagan_app, project_a_id, f"AUTO: {project_a_repo2.name}")
        task_b1 = await _get_task_by_title(kagan_app, project_b_id, f"AUTO: {project_b_repo1.name}")

        for task in (task_a1, task_a2, task_b1):
            assert task.status == TaskStatus.BACKLOG
            assert task.task_type == TaskType.AUTO
