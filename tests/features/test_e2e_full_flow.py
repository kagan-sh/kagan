"""E2E test for the full onboarding -> planner -> AUTO lifecycle flow."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Any

import pytest
from tests.helpers.git import _run_git, configure_git_user, init_git_repo_with_commit
from tests.snapshots.conftest import MockAgent
from tests.snapshots.helpers import (
    type_text,
    wait_for_planner_ready,
    wait_for_screen,
    wait_for_widget,
    wait_for_workers,
)
from tests.snapshots.mock_responses import make_propose_plan_tool_call
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


class SmartMockAgent(MockAgent):
    """Mock agent that adapts responses based on prompt type."""

    def __init__(
        self,
        project_root: Path,
        agent_config: Any,
        *,
        plan_tool_calls: dict[str, Any],
        read_only: bool = False,
    ) -> None:
        super().__init__(project_root, agent_config, read_only=read_only)
        self._plan_tool_calls = plan_tool_calls

    async def _commit_hello_world(self) -> None:
        await configure_git_user(self.project_root)
        script_path = self.project_root / "hello_world.py"
        script_path.write_text('print("Hello, World!")\n')
        await _run_git(self.project_root, "add", "hello_world.py")
        await _run_git(self.project_root, "commit", "-m", "feat: add hello world script")

    async def send_prompt(self, prompt: str) -> str | None:
        if "propose_plan" in prompt:
            self.set_response(PLAN_RESPONSE)
            self.set_tool_calls(self._plan_tool_calls)
        elif "Code Review Specialist" in prompt:
            self.set_response(REVIEW_RESPONSE)
            self.set_tool_calls({})
        else:
            await self._commit_hello_world()
            self.set_response(IMPLEMENTATION_RESPONSE)
            self.set_tool_calls({})

        return await super().send_prompt(prompt)


class SmartAgentFactory:
    """Factory for SmartMockAgent instances."""

    def __init__(self, plan_tool_calls: dict[str, Any]) -> None:
        self._plan_tool_calls = plan_tool_calls
        self._agents: list[SmartMockAgent] = []

    def __call__(self, project_root: Path, agent_config: Any, *, read_only: bool = False) -> Any:
        agent = SmartMockAgent(
            project_root,
            agent_config,
            plan_tool_calls=self._plan_tool_calls,
            read_only=read_only,
        )
        self._agents.append(agent)
        return agent


async def _wait_for_task_status(
    app: KaganApp,
    task_id: str,
    status: TaskStatus,
    *,
    timeout: float = 20.0,
) -> None:
    elapsed = 0.0
    last_status = None
    while elapsed < timeout:
        task = await app.ctx.task_service.get_task(task_id)
        if task:
            last_status = task.status
            if task.status == status:
                return
        await asyncio.sleep(0.1)
        elapsed += 0.1
    raise TimeoutError(
        f"Task {task_id} did not reach {status} within {timeout}s (last status: {last_status})"
    )


async def _wait_for_checks_passed(app: KaganApp, task_id: str, timeout: float = 20.0) -> None:
    elapsed = 0.0
    while elapsed < timeout:
        task = await app.ctx.task_service.get_task(task_id)
        if task and task.checks_passed is True:
            return
        await asyncio.sleep(0.1)
        elapsed += 0.1
    raise TimeoutError(f"Task {task_id} did not complete review within {timeout}s")


async def _wait_for_agent_logs(app: KaganApp, task_id: str, timeout: float = 20.0) -> None:
    elapsed = 0.0
    while elapsed < timeout:
        logs = await app.ctx.task_service.get_agent_logs(task_id, log_type="implementation")
        if logs:
            return
        await asyncio.sleep(0.1)
        elapsed += 0.1
    raise TimeoutError(f"Task {task_id} produced no implementation logs within {timeout}s")


def _mock_tmux(monkeypatch: pytest.MonkeyPatch) -> None:
    async def fake_run_tmux(*_args: str) -> str:
        return ""

    monkeypatch.setattr("kagan.tmux.run_tmux", fake_run_tmux)
    monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_run_tmux)


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
    agent_factory = SmartAgentFactory(plan_tool_calls)

    _mock_tmux(monkeypatch)

    app = KaganApp(
        db_path=str(db_path),
        config_path=str(config_path),
        project_root=cwd_path,
        agent_factory=agent_factory,
    )

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, OnboardingScreen)
        pilot.app.screen.query_one("#auto-mode-switch", Switch).value = True
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

        # New project with no tasks goes directly to PlannerScreen
        await wait_for_screen(pilot, PlannerScreen, timeout=10.0)
        await wait_for_planner_ready(pilot, timeout=10.0)
        await type_text(pilot, "Create a hello world python script")
        await pilot.press("enter")
        await wait_for_workers(pilot, timeout=20.0)
        await wait_for_widget(pilot, "PlanApprovalWidget", timeout=20.0)
        plan_widget = pilot.app.screen.query_one(PlanApprovalWidget)
        plan_widget.focus()
        await pilot.pause()
        plan_widget.action_approve()
        await pilot.pause()
        await wait_for_workers(pilot, timeout=20.0)
        await pilot.pause()

        await wait_for_screen(pilot, KanbanScreen, timeout=20.0)
        tasks = await app.ctx.task_service.list_tasks()
        auto_task = next(task for task in tasks if task.task_type == TaskType.AUTO)
        await wait_for_widget(pilot, f"#card-{auto_task.id}", timeout=10.0)
        card = pilot.app.screen.query_one(f"#card-{auto_task.id}", TaskCard)
        card.focus()
        await pilot.pause()
        await pilot.app.screen._start_agent_flow(auto_task)  # type: ignore[attr-defined]

        await _wait_for_task_status(app, auto_task.id, TaskStatus.IN_PROGRESS, timeout=30.0)
        await _wait_for_agent_logs(app, auto_task.id, timeout=20.0)

        # With auto_merge enabled, task should transition through REVIEW to DONE
        # The transition may be fast, so we just wait for the final DONE state
        await _wait_for_task_status(app, auto_task.id, TaskStatus.DONE, timeout=30.0)

        # Verify the task completed successfully
        final_task = await app.ctx.task_service.get_task(auto_task.id)
        assert final_task is not None
        assert final_task.status == TaskStatus.DONE
        assert final_task.checks_passed is True
