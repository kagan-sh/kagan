"""E2E test for onboarding -> planner (PAIR) -> review -> done flow."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, cast

import pytest
from tests.helpers.git import _run_git, configure_git_user, init_git_repo_with_commit
from tests.helpers.mock_responses import make_propose_plan_tool_call
from tests.helpers.mocks import SmartMockAgent, create_fake_tmux
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
from kagan.mcp.tools import KaganMCPServer
from kagan.services.sessions import SessionService
from kagan.ui.modals.new_project import NewProjectModal
from kagan.ui.modals.review import ReviewModal
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

REVIEW_RESPONSE = """\
Reviewed changes.

<approve summary="Looks good"/>
"""


@pytest.mark.asyncio
async def test_pair_flow_review_to_done(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    repo_path = tmp_path / "pair_repo"
    repo_path.mkdir()
    await init_git_repo_with_commit(repo_path)

    config_path = tmp_path / "kagan-config" / "config.toml"
    db_path = tmp_path / "kagan-data" / "kagan.db"

    plan_tool_calls = make_propose_plan_tool_call(
        tasks=[
            {
                "title": "Improve onboarding copy",
                "type": "PAIR",
                "description": "Refine onboarding text and copy for clarity.",
                "acceptance_criteria": [
                    "Updated onboarding copy is committed",
                    "No regressions in onboarding flow",
                ],
                "priority": "medium",
            }
        ],
        todos=[
            {"content": "Draft copy changes", "status": "completed"},
            {"content": "Apply updates", "status": "in_progress"},
        ],
    )

    def _agent_factory(project_root: Path, agent_config: Any, *, read_only: bool = False) -> Any:
        return SmartMockAgent(
            project_root,
            agent_config,
            read_only=read_only,
            routes={
                "Code Review Specialist": (REVIEW_RESPONSE, {}),
            },
            default=(PLAN_RESPONSE, plan_tool_calls),
        )

    sessions: dict[str, dict[str, Any]] = {}
    fake_tmux = create_fake_tmux(sessions)
    monkeypatch.setattr("kagan.tmux.run_tmux", fake_tmux)
    monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)

    async def _fast_attach(self: SessionService, _session_name: str) -> bool:
        return True

    monkeypatch.setattr(SessionService, "_attach_tmux_session", _fast_attach)

    app = KaganApp(
        db_path=str(db_path),
        config_path=str(config_path),
        project_root=repo_path,
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
        modal.query_one("#name-input", Input).value = "Pair Flow"
        modal.query_one("#path-input", Input).value = str(repo_path)
        await pilot.pause()
        modal.query_one("#btn-create", Button).press()

        await wait_for_screen(pilot, PlannerScreen, timeout=10.0)
        await wait_for_planner_ready(pilot, timeout=10.0)
        await type_text(pilot, "Refine onboarding copy")
        await pilot.press("enter")
        await wait_for_widget(pilot, "PlanApprovalWidget", timeout=20.0)
        plan_widget = pilot.app.screen.query_one(PlanApprovalWidget)
        plan_widget.focus()
        await pilot.pause()
        plan_widget.action_approve()
        await pilot.pause()

        await wait_for_screen(pilot, KanbanScreen, timeout=20.0)
        cast("KaganApp", pilot.app).config.general.default_pair_terminal_backend = "tmux"
        cast("KaganApp", pilot.app).ctx.config.general.default_pair_terminal_backend = "tmux"
        cast("KaganApp", pilot.app).config.ui.skip_pair_instructions = True
        tasks = await app.ctx.task_service.list_tasks()
        pair_task = next(task for task in tasks if task.task_type == TaskType.PAIR)
        await app.ctx.task_service.update_fields(pair_task.id, terminal_backend="tmux")
        refreshed_task = await app.ctx.task_service.get_task(pair_task.id)
        assert refreshed_task is not None
        pair_task = refreshed_task
        await wait_for_widget(pilot, f"#card-{pair_task.id}", timeout=10.0)
        card = pilot.app.screen.query_one(f"#card-{pair_task.id}", TaskCard)
        card.focus()
        await pilot.pause()
        await pilot.press("enter")

        await wait_for_task_status(app, pair_task.id, TaskStatus.IN_PROGRESS, timeout=20.0)
        worktree_path = await app.ctx.workspace_service.get_path(pair_task.id)
        assert worktree_path is not None

        await configure_git_user(worktree_path)
        (worktree_path / "onboarding.md").write_text("Updated onboarding copy\n")
        await _run_git(worktree_path, "add", "onboarding.md")
        await _run_git(worktree_path, "commit", "-m", "docs: refine onboarding copy")

        mcp = KaganMCPServer(
            app.ctx.task_service,
            workspace_service=app.ctx.workspace_service,
            project_service=app.ctx.project_service,
        )
        await mcp.request_review(pair_task.id, "Ready for review")

        await wait_for_task_status(app, pair_task.id, TaskStatus.REVIEW, timeout=20.0)
        # Give board time to refresh after status change
        for _ in range(5):
            await pilot.pause()
        await wait_for_widget(pilot, f"#card-{pair_task.id}", timeout=10.0)
        card = pilot.app.screen.query_one(f"#card-{pair_task.id}", TaskCard)
        card.focus()
        for _ in range(3):
            await pilot.pause()
        await pilot.press("enter")
        await wait_for_screen(pilot, ReviewModal, timeout=20.0)
        await pilot.press("enter")

        await wait_for_task_status(app, pair_task.id, TaskStatus.DONE, timeout=30.0, pilot=pilot)
