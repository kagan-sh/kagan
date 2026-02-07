"""E2E test for the full onboarding -> planner -> AUTO lifecycle flow."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import pytest
from tests.helpers.git import _run_git, configure_git_user, init_git_repo_with_commit
from tests.helpers.mock_responses import make_propose_plan_tool_call
from tests.helpers.mocks import SmartMockAgent
from tests.helpers.wait import (
    type_text,
    wait_for_planner_ready,
    wait_for_screen,
    wait_for_task_status,
    wait_for_widget,
)
from textual.widgets import Button, Input, Switch

from kagan.app import KaganApp
from kagan.core.models.enums import TaskStatus, TaskType
from kagan.ui.modals.new_project import NewProjectModal
from kagan.ui.screens.kanban import KanbanScreen
from kagan.ui.screens.onboarding import OnboardingScreen
from kagan.ui.screens.planner import PlannerScreen
from kagan.ui.screens.welcome import WelcomeScreen
from kagan.ui.widgets.card import TaskCard
from kagan.ui.widgets.plan_approval import PlanApprovalWidget

if TYPE_CHECKING:
    from pathlib import Path

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


async def _commit_hello_world(agent: SmartMockAgent) -> None:
    await configure_git_user(agent.project_root)
    script_path = agent.project_root / "hello_world.py"
    script_path.write_text('print("Hello, World!")\n')
    await _run_git(agent.project_root, "add", "hello_world.py")
    await _run_git(agent.project_root, "commit", "-m", "feat: add hello world script")


async def _wait_for_agent_logs(app: KaganApp, task_id: str, timeout: float = 20.0) -> None:
    import asyncio

    from tests.helpers.wait import _ci_timeout

    timeout = _ci_timeout(timeout)
    elapsed = 0.0
    while elapsed < timeout:
        execution = await app.ctx.execution_service.get_latest_execution_for_task(task_id)
        if execution:
            logs = await app.ctx.execution_service.get_logs(execution.id)
            if logs and logs.logs:
                return
        await asyncio.sleep(0.1)
        elapsed += 0.1
    raise TimeoutError(f"Task {task_id} produced no logs within {timeout}s")


@pytest.mark.asyncio
async def test_full_e2e_flow(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    repo_path = tmp_path / "hello_repo"
    repo_path.mkdir()
    await init_git_repo_with_commit(repo_path)

    cwd_path = tmp_path / "cwd"
    cwd_path.mkdir()

    config_path = tmp_path / "kagan-config" / "config.toml"
    db_path = tmp_path / "kagan-data" / "kagan.db"

    plan_tool_calls = make_propose_plan_tool_call(
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

    def _agent_factory(project_root: Path, agent_config: Any, *, read_only: bool = False) -> Any:
        return SmartMockAgent(
            project_root,
            agent_config,
            read_only=read_only,
            routes={
                "propose_plan": (PLAN_RESPONSE, plan_tool_calls),
                "Code Review Specialist": (REVIEW_RESPONSE, {}),
            },
            default=(IMPLEMENTATION_RESPONSE, {}),
            on_default=_commit_hello_world,
        )

    async def fake_run_tmux(*_args: str) -> str:
        return ""

    monkeypatch.setattr("kagan.tmux.run_tmux", fake_run_tmux)
    monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_run_tmux)

    app = KaganApp(
        db_path=str(db_path),
        config_path=str(config_path),
        project_root=cwd_path,
        agent_factory=_agent_factory,
    )

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, OnboardingScreen)
        pilot.app.screen.query_one("#auto-review-switch", Switch).value = True
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
        await type_text(pilot, "Create a hello world python script")
        await pilot.press("enter")
        await wait_for_widget(pilot, "PlanApprovalWidget", timeout=20.0)
        plan_widget = pilot.app.screen.query_one(PlanApprovalWidget)
        plan_widget.focus()
        await pilot.pause()
        plan_widget.action_approve()
        for _ in range(10):
            await pilot.pause()

        await wait_for_screen(pilot, KanbanScreen, timeout=20.0)
        tasks = await app.ctx.task_service.list_tasks()
        auto_task = next(task for task in tasks if task.task_type == TaskType.AUTO)
        await wait_for_widget(pilot, f"#card-{auto_task.id}", timeout=10.0)
        card = pilot.app.screen.query_one(f"#card-{auto_task.id}", TaskCard)
        card.focus()
        await pilot.pause()
        await pilot.press("a")

        await _wait_for_agent_logs(app, auto_task.id, timeout=20.0)
        await wait_for_task_status(app, auto_task.id, TaskStatus.REVIEW, timeout=30.0)

        final_task = await app.ctx.task_service.get_task(auto_task.id)
        assert final_task is not None
        assert final_task.status == TaskStatus.REVIEW
