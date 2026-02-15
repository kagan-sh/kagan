"""E2E tests for core onboarding, AUTO, and PAIR flows."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

import pytest
from tests.helpers.config import write_test_config
from tests.helpers.git import _run_git, configure_git_user, init_git_repo_with_commit
from tests.helpers.mock_responses import (
    SIMPLE_IMPLEMENTATION_TEXT,
    SIMPLE_PLAN_TEXT,
    make_plan_submit_tool_call,
)
from tests.helpers.mocks import build_smart_agent_factory
from tests.helpers.wait import (
    wait_for_planner_ready,
    wait_for_screen,
    wait_for_task_status,
    wait_for_widget,
)
from textual.widgets import Button, Input, Switch

from kagan.core.adapters.db.repositories import RepoRepository, TaskRepository
from kagan.core.adapters.db.schema import Task
from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType
from kagan.core.services.sessions import SessionServiceImpl
from kagan.tui.app import KaganApp
from kagan.tui.ui.modals.new_project import NewProjectModal
from kagan.tui.ui.modals.review import ReviewModal
from kagan.tui.ui.screens.kanban import KanbanScreen
from kagan.tui.ui.screens.onboarding import OnboardingScreen
from kagan.tui.ui.screens.planner import PlannerInput, PlannerScreen
from kagan.tui.ui.screens.welcome import WelcomeScreen
from kagan.tui.ui.widgets.card import TaskCard
from kagan.tui.ui.widgets.plan_approval import PlanApprovalWidget

if TYPE_CHECKING:
    from pathlib import Path

pytestmark = pytest.mark.usefixtures("global_mock_tmux")


async def _seed_pair_app(tmp_path: Path) -> tuple[KaganApp, str]:
    repo_path = tmp_path / "pair_repo"
    repo_path.mkdir()
    await init_git_repo_with_commit(repo_path)

    config_path = write_test_config(
        tmp_path / "config.toml",
        auto_review=False,
        skip_pair_instructions=True,
        default_pair_terminal_backend="tmux",
    )

    db_path = tmp_path / "kagan.db"
    manager = TaskRepository(db_path, project_root=repo_path)
    await manager.initialize()
    project_id = await manager.ensure_test_project("Pair Flow")

    repo_repo = RepoRepository(manager.session_factory)
    repo, _ = await repo_repo.get_or_create(repo_path, default_branch="main")
    if repo.id:
        await repo_repo.update_default_branch(repo.id, "main", mark_configured=True)
        await repo_repo.add_to_project(project_id, repo.id, is_primary=True)

    task = Task(
        id="pair0001",
        project_id=project_id,
        title="PAIR flow",
        description="",
        priority=TaskPriority.MEDIUM,
        status=TaskStatus.BACKLOG,
        task_type=TaskType.PAIR,
        terminal_backend="tmux",
    )
    await manager.create(task)
    await manager.close()

    app = KaganApp(
        db_path=str(db_path),
        config_path=str(config_path),
        project_root=repo_path,
    )
    return app, task.id


@pytest.mark.asyncio
async def test_full_e2e_flow(tmp_path: Path) -> None:
    repo_path = tmp_path / "hello_repo"
    repo_path.mkdir()
    await init_git_repo_with_commit(repo_path)

    cwd_path = tmp_path / "cwd"
    cwd_path.mkdir()

    config_path = tmp_path / "kagan-config" / "config.toml"
    db_path = tmp_path / "kagan-data" / "kagan.db"

    plan_tool_calls = make_plan_submit_tool_call(
        tool_call_id="tc-hello-001",
        tasks=[
            {
                "title": "Create hello world script",
                "type": "AUTO",
                "description": "Add hello_world.py that prints Hello, World!",
                "acceptance_criteria": [
                    "Script exists at repo root",
                    "Running it prints Hello, World!",
                ],
                "priority": "low",
            }
        ],
        todos=[
            {"content": "Define task scope", "status": "completed"},
            {"content": "Propose minimal implementation", "status": "completed"},
        ],
    )

    app = KaganApp(
        db_path=str(db_path),
        config_path=str(config_path),
        project_root=cwd_path,
        agent_factory=build_smart_agent_factory(
            routes={"plan_submit": (SIMPLE_PLAN_TEXT, plan_tool_calls)},
            default=(SIMPLE_IMPLEMENTATION_TEXT, {}),
        ),
    )

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, OnboardingScreen)
        pilot.app.screen.query_one("#auto-review-switch", Switch).value = False
        await pilot.pause()
        pilot.app.screen.query_one("#btn-continue", Button).press()

        await wait_for_screen(pilot, WelcomeScreen)
        await pilot.press("n")
        await wait_for_screen(pilot, NewProjectModal)
        modal = pilot.app.screen
        modal.query_one("#name-input", Input).value = "Hello World"
        modal.query_one("#path-input", Input).value = str(repo_path)
        await pilot.pause()
        modal.query_one("#btn-create", Button).press()

        await wait_for_screen(pilot, PlannerScreen, timeout=10.0)
        await wait_for_planner_ready(pilot, timeout=10.0)
        planner_screen = cast("PlannerScreen", pilot.app.screen)
        planner_input = planner_screen.query_one("#planner-input", PlannerInput)
        planner_input.text = "Create a hello world script"
        await pilot.pause()
        await pilot.press("enter")
        await wait_for_widget(pilot, "PlanApprovalWidget", timeout=20.0)
        plan_widget = pilot.app.screen.query_one(PlanApprovalWidget)
        plan_widget.focus()
        await pilot.pause()
        plan_widget.action_approve()

        await wait_for_screen(pilot, KanbanScreen, timeout=20.0)
        tasks = await app.ctx.task_service.list_tasks()
        auto_task = next(task for task in tasks if task.task_type == TaskType.AUTO)
        await wait_for_widget(pilot, f"#card-{auto_task.id}", timeout=10.0)
        card = pilot.app.screen.query_one(f"#card-{auto_task.id}", TaskCard)
        card.focus()
        await pilot.pause()
        await pilot.press("a")

        await wait_for_task_status(app, auto_task.id, TaskStatus.REVIEW, timeout=30.0)

        final_task = await app.ctx.task_service.get_task(auto_task.id)
        assert final_task is not None
        assert final_task.status == TaskStatus.REVIEW


@pytest.mark.asyncio
async def test_pair_flow_review_to_done(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    async def _fast_attach(self: SessionServiceImpl, _session_name: str) -> bool:
        return True

    monkeypatch.setattr(SessionServiceImpl, "_attach_tmux_session", _fast_attach)
    monkeypatch.setattr(
        "kagan.tui.terminals.installer.shutil.which",
        lambda _cmd, *_a, **_kw: "/usr/bin/mock",
    )

    app, task_id = await _seed_pair_app(tmp_path)

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=20.0)
        await wait_for_widget(pilot, f"#card-{task_id}", timeout=10.0)

        card = pilot.app.screen.query_one(f"#card-{task_id}", TaskCard)
        card.focus()
        await pilot.pause()
        await pilot.press("enter")

        await wait_for_task_status(app, task_id, TaskStatus.IN_PROGRESS, timeout=20.0, pilot=pilot)
        worktree_path = await app.ctx.workspace_service.get_path(task_id)
        assert worktree_path is not None

        await configure_git_user(worktree_path)
        (worktree_path / "onboarding.md").write_text("Updated onboarding copy\n")
        await _run_git(worktree_path, "add", "onboarding.md")
        await _run_git(worktree_path, "commit", "-m", "docs: refine onboarding copy")

        await app.ctx.task_service.set_status(task_id, TaskStatus.REVIEW, reason="Ready for review")
        await wait_for_task_status(app, task_id, TaskStatus.REVIEW, timeout=20.0, pilot=pilot)

        await wait_for_widget(pilot, f"#card-{task_id}", timeout=10.0)
        review_card = pilot.app.screen.query_one(f"#card-{task_id}", TaskCard)
        review_card.focus()
        await pilot.pause()
        await pilot.press("enter")
        await wait_for_screen(pilot, ReviewModal, timeout=20.0)
        await pilot.press("enter")

        await wait_for_task_status(app, task_id, TaskStatus.DONE, timeout=30.0, pilot=pilot)

        final_task = await cast("KaganApp", pilot.app).ctx.task_service.get_task(task_id)
        assert final_task is not None
        assert final_task.status == TaskStatus.DONE
