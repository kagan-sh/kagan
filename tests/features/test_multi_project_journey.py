"""UI-driven multi-project journey with AUTO + PAIR tasks per repo."""

from __future__ import annotations

import asyncio
import platform
from pathlib import Path
from typing import TYPE_CHECKING, Any, cast

import pytest
from tests.helpers.git import _run_git, configure_git_user, init_git_repo_with_commit
from tests.helpers.mock_responses import make_propose_plan_tool_call
from tests.helpers.mocks import MockAgent, SmartMockAgent, create_fake_tmux
from tests.helpers.wait import (
    type_text,
    wait_for_planner_ready,
    wait_for_screen,
    wait_for_task_status,
    wait_for_widget,
)
from textual.widgets import Button, Input, ListView, Switch

from kagan.app import KaganApp
from kagan.core.models.enums import TaskStatus, TaskType
from kagan.mcp.tools import KaganMCPServer
from kagan.services.sessions import SessionServiceImpl
from kagan.ui.modals.folder_picker import FolderPickerModal
from kagan.ui.modals.new_project import NewProjectModal
from kagan.ui.modals.task_details_modal import TaskDetailsModal
from kagan.ui.screens.kanban import KanbanScreen
from kagan.ui.screens.onboarding import OnboardingScreen
from kagan.ui.screens.planner import PlannerScreen
from kagan.ui.screens.repo_picker import RepoPickerScreen
from kagan.ui.screens.welcome import WelcomeScreen
from kagan.ui.widgets.card import TaskCard
from kagan.ui.widgets.plan_approval import PlanApprovalWidget

if TYPE_CHECKING:
    from kagan.adapters.db.schema import Task


PLAN_RESPONSE = """\
I've created a plan for this change.
"""

IMPLEMENTATION_RESPONSE = """\
Implemented the requested changes.

<complete/>
"""

REVIEW_RESPONSE = """\
Reviewed changes.

<approve summary="Looks good"/>
"""


class _JourneyAgent(SmartMockAgent):
    def __init__(
        self,
        project_root: Path,
        agent_config: Any,
        *,
        plan_tool_calls_by_repo: dict[str, dict[str, Any]],
        read_only: bool = False,
    ) -> None:
        super().__init__(
            project_root,
            agent_config,
            read_only=read_only,
            routes={
                "Code Review Specialist": (REVIEW_RESPONSE, {}),
            },
            default=(IMPLEMENTATION_RESPONSE, {}),
        )
        self._plan_tool_calls_by_repo = plan_tool_calls_by_repo
        self._commit_idx = 0

    async def _commit_change(self) -> None:
        await configure_git_user(self.project_root)
        self._commit_idx += 1
        file_path = self.project_root / f"auto_change_{self._commit_idx}.txt"
        file_path.write_text(f"auto change {self._commit_idx}\n")
        await _run_git(self.project_root, "add", file_path.name)
        await _run_git(self.project_root, "commit", "-m", f"test: auto change {self._commit_idx}")

    async def send_prompt(self, prompt: str) -> str | None:
        if "propose_plan" in prompt:
            self.set_response(PLAN_RESPONSE)
            tool_calls = self._plan_tool_calls_by_repo.get(str(self.project_root), {})
            self.set_tool_calls(tool_calls)
            return await MockAgent.send_prompt(self, prompt)

        if "Code Review Specialist" not in prompt:
            await self._commit_change()

        return await super().send_prompt(prompt)


class _JourneyAgentFactory:
    def __init__(self, plan_tool_calls_by_repo: dict[str, dict[str, Any]]) -> None:
        self._plan_tool_calls_by_repo = plan_tool_calls_by_repo

    def __call__(self, project_root: Path, agent_config: Any, *, read_only: bool = False) -> Any:
        return _JourneyAgent(
            project_root,
            agent_config,
            plan_tool_calls_by_repo=self._plan_tool_calls_by_repo,
            read_only=read_only,
        )


async def _wait_for_workspace_path(
    app: KaganApp,
    task_id: str,
    *,
    timeout: float = 10.0,
) -> Path:
    from tests.helpers.wait import _ci_timeout

    timeout = _ci_timeout(timeout)
    elapsed = 0.0
    while elapsed < timeout:
        wt_path = await app.ctx.workspace_service.get_path(task_id)
        if wt_path is not None:
            return wt_path
        await asyncio.sleep(0.1)
        elapsed += 0.1
    raise TimeoutError(f"Workspace not ready for task {task_id} within {timeout}s")


async def _wait_for_active_repo(app: KaganApp, repo_path: Path, *, timeout: float = 5.0) -> None:
    from tests.helpers.wait import _ci_timeout

    timeout = _ci_timeout(timeout)
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

    elapsed = 0.0
    while elapsed < timeout:
        if app.ctx.active_repo_id == target.id:
            return
        await asyncio.sleep(0.1)
        elapsed += 0.1
    raise TimeoutError(f"Active repo not updated to {repo_path} within {timeout}s")


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

    from tests.helpers.wait import _ci_timeout

    _open_timeout = _ci_timeout(10.0)
    elapsed = 0.0
    while elapsed < _open_timeout:
        if isinstance(pilot.app.screen, (RepoPickerScreen, KanbanScreen)):
            break
        await pilot.pause()
        await asyncio.sleep(0.1)
        elapsed += 0.1

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


async def _open_project_selector(pilot) -> None:
    await pilot.app.action_open_project_selector()
    await wait_for_screen(pilot, WelcomeScreen, timeout=10.0)


async def _wait_for_project_item(pilot, name: str, *, timeout: float = 10.0) -> None:
    from tests.helpers.wait import _ci_timeout

    timeout = _ci_timeout(timeout)
    elapsed = 0.0
    while elapsed < timeout:
        screen = pilot.app.screen
        if isinstance(screen, WelcomeScreen):
            if any(item.project_name == name for item in screen._project_items):
                return
        await pilot.pause()
        await asyncio.sleep(0.1)
        elapsed += 0.1
    raise TimeoutError(f"Project not found in welcome list: {name}")


async def _create_auto_task_from_planner(pilot, prompt: str) -> None:
    await wait_for_planner_ready(pilot, timeout=10.0)
    await type_text(pilot, prompt)
    await pilot.press("enter")
    await pilot.pause()
    await wait_for_widget(pilot, "PlanApprovalWidget", timeout=30.0)

    plan_widget = pilot.app.screen.query_one(PlanApprovalWidget)
    plan_widget.focus()
    await pilot.pause()
    plan_widget.action_approve()
    await pilot.pause()
    await pilot.pause()
    await wait_for_screen(pilot, KanbanScreen, timeout=20.0)


async def _open_planner(pilot) -> None:
    screen = cast("KanbanScreen", pilot.app.screen)
    screen.action_open_planner()
    await wait_for_screen(pilot, PlannerScreen, timeout=10.0)


async def _focus_task_card(pilot, task_id: str) -> TaskCard:
    await wait_for_widget(pilot, f"#card-{task_id}", timeout=10.0)
    card = pilot.app.screen.query_one(f"#card-{task_id}", TaskCard)
    card.focus()
    await pilot.pause()
    return card


async def _start_auto_task(pilot, task_id: str) -> None:
    await _focus_task_card(pilot, task_id)
    await pilot.press("a")
    for _ in range(5):
        await pilot.pause()


async def _approve_task(pilot, app: KaganApp, task_id: str) -> None:
    await wait_for_task_status(app, task_id, TaskStatus.REVIEW, timeout=40.0, pilot=pilot)
    task = await app.ctx.task_service.get_task(task_id)
    assert task is not None, f"Task {task_id} not found"
    assert app.ctx.merge_service is not None, "merge_service is None"

    # Rebase worktree onto latest main to handle sequential merges on same repo
    worktree_path = await app.ctx.workspace_service.get_path(task_id)
    if worktree_path is not None:
        await _run_git(worktree_path, "rebase", "main")

    success, message = await app.ctx.merge_service.merge_task(task)
    assert success, f"Merge failed for {task_id}: {message}"
    for _ in range(5):
        await pilot.pause()


async def _create_pair_task(pilot, title: str) -> None:
    await pilot.press("n")
    await wait_for_screen(pilot, TaskDetailsModal)

    modal = cast("TaskDetailsModal", pilot.app.screen)
    modal.query_one("#title-input", Input).value = title
    await pilot.pause()
    modal.query_one("#save-btn", Button).press()

    await wait_for_screen(pilot, KanbanScreen, timeout=10.0)


async def _get_task_by_title(app: KaganApp, project_id: str, title: str) -> Task:
    tasks = await app.ctx.task_service.list_tasks(project_id=project_id)
    for task in tasks:
        if task.title == title:
            return task
    raise RuntimeError(f"Task not found: {title}")


async def _complete_pair_task(
    pilot,
    app: KaganApp,
    project_id: str,
    repo_path: Path,
    title: str,
) -> str:
    await _select_repo(pilot, repo_path)
    await _create_pair_task(pilot, title)
    task = await _get_task_by_title(app, project_id, title)

    await _focus_task_card(pilot, task.id)
    await pilot.press("enter")
    await wait_for_task_status(app, task.id, TaskStatus.IN_PROGRESS, timeout=20.0, pilot=pilot)

    worktree_path = await _wait_for_workspace_path(app, task.id, timeout=10.0)
    await configure_git_user(worktree_path)
    file_path = worktree_path / f"pair_change_{task.short_id}.txt"
    file_path.write_text("pair change\n")
    await _run_git(worktree_path, "add", file_path.name)
    await _run_git(worktree_path, "commit", "-m", f"feat: pair change {task.short_id}")

    mcp = KaganMCPServer(
        app.ctx.task_service,
        workspace_service=app.ctx.workspace_service,
        project_service=app.ctx.project_service,
    )
    await mcp.request_review(task.id, "Ready for review")

    await _approve_task(pilot, app, task.id)
    return task.id


@pytest.mark.asyncio
@pytest.mark.skipif(
    platform.system() == "Windows",
    reason="Heavy E2E journey unreliable on Windows CI (AppContext timing)",
)
async def test_multi_project_ui_journey(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """UI-driven journey: 2 projects, 2 repos each, AUTO+PAIR per repo."""
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path / "xdg-config"))
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path / "xdg-data"))

    project_a_repo1 = tmp_path / "project-a-repo-1"
    project_a_repo2 = tmp_path / "project-a-repo-2"
    project_b_repo1 = tmp_path / "project-b-repo-1"
    project_b_repo2 = tmp_path / "project-b-repo-2"
    for repo in (project_a_repo1, project_a_repo2, project_b_repo1, project_b_repo2):
        repo.mkdir()
        await init_git_repo_with_commit(repo)

    config_path = tmp_path / "xdg-config" / "kagan" / "config.toml"
    db_path = tmp_path / "xdg-data" / "kagan" / "kagan.db"

    def build_plan(repo_path: Path) -> dict[str, Any]:
        return make_propose_plan_tool_call(
            tool_call_id=f"tc-plan-{repo_path.name}",
            tasks=[
                {
                    "title": f"AUTO: {repo_path.name}",
                    "type": "AUTO",
                    "description": f"Auto task for {repo_path.name}",
                    "acceptance_criteria": ["Creates a commit", "Moves to review"],
                    "priority": "low",
                }
            ],
            todos=[
                {"content": "Define task", "status": "completed"},
                {"content": "Propose plan", "status": "completed"},
            ],
        )

    plan_tool_calls_by_repo = {
        str(project_a_repo1): build_plan(project_a_repo1),
        str(project_a_repo2): build_plan(project_a_repo2),
        str(project_b_repo1): build_plan(project_b_repo1),
        str(project_b_repo2): build_plan(project_b_repo2),
    }

    agent_factory = _JourneyAgentFactory(plan_tool_calls_by_repo)

    sessions: dict[str, Any] = {}
    fake_tmux = create_fake_tmux(sessions)
    monkeypatch.setattr("kagan.tmux.run_tmux", fake_tmux)
    monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)

    async def _fast_attach(self: SessionServiceImpl, _session_name: str) -> bool:
        return True

    monkeypatch.setattr(SessionServiceImpl, "_attach_tmux_session", _fast_attach)

    app = KaganApp(
        db_path=str(db_path),
        config_path=str(config_path),
        project_root=tmp_path,
        agent_factory=agent_factory,
    )

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, OnboardingScreen)
        pilot.app.screen.query_one("#auto-review-switch", Switch).value = False
        await pilot.pause()
        pilot.app.screen.query_one("#btn-continue", Button).press()

        await wait_for_screen(pilot, WelcomeScreen, timeout=10.0)

        kagan_app = cast("KaganApp", pilot.app)
        kagan_app.config.general.max_concurrent_agents = 8
        kagan_app.ctx.config.general.max_concurrent_agents = 8
        kagan_app.config.general.default_pair_terminal_backend = "tmux"
        kagan_app.ctx.config.general.default_pair_terminal_backend = "tmux"
        kagan_app.config.ui.skip_pair_instructions = True
        kagan_app.ctx.config.ui.skip_pair_instructions = True

        project_a_id = await _create_project(pilot, "Project A", project_a_repo1)
        await _create_auto_task_from_planner(pilot, "Auto task for repo A1")
        auto_a1 = await _get_task_by_title(kagan_app, project_a_id, f"AUTO: {project_a_repo1.name}")
        await _start_auto_task(pilot, auto_a1.id)

        await _add_repo(pilot, project_a_repo2)
        await _select_repo(pilot, project_a_repo2)
        await _open_planner(pilot)
        await _create_auto_task_from_planner(pilot, "Auto task for repo A2")
        auto_a2 = await _get_task_by_title(kagan_app, project_a_id, f"AUTO: {project_a_repo2.name}")
        await _start_auto_task(pilot, auto_a2.id)

        await _open_project_selector(pilot)
        project_b_id = await _create_project(pilot, "Project B", project_b_repo1)
        await _create_auto_task_from_planner(pilot, "Auto task for repo B1")
        auto_b1 = await _get_task_by_title(kagan_app, project_b_id, f"AUTO: {project_b_repo1.name}")
        await _start_auto_task(pilot, auto_b1.id)

        await _add_repo(pilot, project_b_repo2)
        await _select_repo(pilot, project_b_repo2)
        await _open_planner(pilot)
        await _create_auto_task_from_planner(pilot, "Auto task for repo B2")
        auto_b2 = await _get_task_by_title(kagan_app, project_b_id, f"AUTO: {project_b_repo2.name}")
        await _start_auto_task(pilot, auto_b2.id)

        await _open_project(pilot, "Project A", project_a_repo1)
        pair_a1_title = f"PAIR: {project_a_repo1.name}"
        pair_a2_title = f"PAIR: {project_a_repo2.name}"
        await _complete_pair_task(pilot, kagan_app, project_a_id, project_a_repo1, pair_a1_title)
        await _complete_pair_task(pilot, kagan_app, project_a_id, project_a_repo2, pair_a2_title)

        await _open_project(pilot, "Project B", project_b_repo1)
        pair_b1_title = f"PAIR: {project_b_repo1.name}"
        pair_b2_title = f"PAIR: {project_b_repo2.name}"
        await _complete_pair_task(pilot, kagan_app, project_b_id, project_b_repo1, pair_b1_title)
        await _complete_pair_task(pilot, kagan_app, project_b_id, project_b_repo2, pair_b2_title)

        await _open_project(pilot, "Project A", project_a_repo1)
        await _approve_task(pilot, kagan_app, auto_a1.id)
        await _approve_task(pilot, kagan_app, auto_a2.id)

        await _open_project(pilot, "Project B", project_b_repo1)
        await _approve_task(pilot, kagan_app, auto_b1.id)
        await _approve_task(pilot, kagan_app, auto_b2.id)

        expected_done_titles = {
            f"AUTO: {project_a_repo1.name}",
            f"AUTO: {project_a_repo2.name}",
            f"AUTO: {project_b_repo1.name}",
            f"AUTO: {project_b_repo2.name}",
            pair_a1_title,
            pair_a2_title,
            pair_b1_title,
            pair_b2_title,
        }
        all_tasks = await kagan_app.ctx.task_service.list_tasks()
        done_titles = {task.title for task in all_tasks if task.status == TaskStatus.DONE}
        assert expected_done_titles.issubset(done_titles)

        for task in (auto_a1, auto_a2, auto_b1, auto_b2):
            latest = await kagan_app.ctx.task_service.get_task(task.id)
            assert latest is not None
            assert latest.status == TaskStatus.DONE
            assert latest.task_type == TaskType.AUTO
