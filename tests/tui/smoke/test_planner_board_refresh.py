from __future__ import annotations

import sys
from pathlib import Path
from typing import Any, cast

import pytest
from tests.helpers.config import write_test_config
from tests.helpers.git import init_git_repo_with_commit
from tests.helpers.mock_responses import make_plan_submit_tool_call
from tests.helpers.mocks import MockAgent
from tests.helpers.wait import (
    type_text,
    wait_for_planner_ready,
    wait_for_screen,
    wait_for_widget,
    wait_until,
)
from textual.widgets import Input, Label

from kagan.core.acp import messages
from kagan.core.adapters.db.repositories import RepoRepository, TaskRepository
from kagan.tui.app import KaganApp
from kagan.tui.ui.screens.kanban import KanbanScreen
from kagan.tui.ui.screens.planner import PlannerInput, PlannerScreen
from kagan.tui.ui.screens.repo_picker import RepoPickerScreen
from kagan.tui.ui.widgets import StatusBar, StreamingOutput
from kagan.tui.ui.widgets.header import KaganHeader
from kagan.tui.ui.widgets.plan_approval import PlanApprovalWidget
from kagan.tui.ui.widgets.slash_complete import SlashComplete

pytestmark = pytest.mark.usefixtures("global_mock_tmux")


def _header_stats_text(screen: PlannerScreen) -> str:
    header = screen.query_one(KaganHeader)
    return str(header.query_one("#header-stats", Label).content)


class RepoEchoAgent(MockAgent):
    """Mock agent that echoes its working folder in the response."""

    def __init__(self, project_root: Path, agent_config: Any, *, read_only: bool = False) -> None:
        super().__init__(project_root, agent_config, read_only=read_only)
        self.set_response(f"I'm currently working in: {project_root}")


class RepoEchoAgentFactory:
    """Factory that returns RepoEchoAgent instances."""

    def __call__(self, project_root: Path, agent_config: Any, *, read_only: bool = False) -> Any:
        return RepoEchoAgent(project_root, agent_config, read_only=read_only)


def _read_planner_output(screen: PlannerScreen) -> str:
    from kagan.tui.ui.widgets.streaming_markdown import StreamingMarkdown

    return "\n".join(widget.content for widget in screen.query(StreamingMarkdown))


async def _bootstrap_planner_app(
    tmp_path: Path,
    *,
    repos: list[Path],
    project_name: str,
) -> KaganApp:
    if not repos:
        raise ValueError("Expected at least one repo path")

    config_path = write_test_config(tmp_path / "config.toml", auto_review=False)
    db_path = tmp_path / "kagan.db"

    task_repo = TaskRepository(db_path, project_root=repos[0])
    await task_repo.initialize()
    project_id = await task_repo.ensure_test_project(project_name)
    assert task_repo._session_factory is not None
    repo_repo = RepoRepository(task_repo._session_factory)
    for index, repo_path in enumerate(repos):
        repo_row, _ = await repo_repo.get_or_create(repo_path, default_branch="main")
        if repo_row.id:
            await repo_repo.update_default_branch(repo_row.id, "main", mark_configured=True)
            await repo_repo.add_to_project(project_id, repo_row.id, is_primary=index == 0)
    await task_repo.close()

    return KaganApp(
        db_path=str(db_path),
        config_path=str(config_path),
        project_root=repos[0],
        agent_factory=RepoEchoAgentFactory(),
    )


@pytest.mark.asyncio
async def test_planner_approval_returns_to_fresh_board(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    created_title = "Planner created task: board refresh regression"
    mock_agent_factory.set_default_response("Plan ready.")
    mock_agent_factory.set_default_tool_calls(
        make_plan_submit_tool_call(
            tool_call_id="tc-board-refresh-001",
            tasks=[
                {
                    "title": created_title,
                    "type": "AUTO",
                    "description": "Ensure board is refreshed when returning from planner.",
                    "acceptance_criteria": ["Task is visible on Kanban after approve"],
                    "priority": "low",
                }
            ],
            todos=[
                {"content": "Build a plan", "status": "completed"},
                {"content": "Submit task proposal", "status": "completed"},
            ],
        )
    )

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        assert isinstance(kanban, KanbanScreen)

        kanban.action_toggle_search()
        await pilot.pause()
        kanban.query_one("#search-input", Input).value = "query-that-matches-nothing"
        await pilot.pause()

        kanban.action_open_planner()
        await wait_for_screen(pilot, PlannerScreen, timeout=10.0)
        await wait_for_planner_ready(pilot, timeout=10.0)
        await type_text(pilot, "Create one task")
        await pilot.press("enter")
        await wait_for_widget(pilot, "PlanApprovalWidget", timeout=20.0)

        planner = pilot.app.screen
        assert isinstance(planner, PlannerScreen)
        plan_widget = planner.query_one(PlanApprovalWidget)
        plan_widget.focus()
        await pilot.pause()
        plan_widget.action_approve()

        await wait_for_screen(pilot, KanbanScreen, timeout=20.0)
        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        created_task = next(task for task in tasks if task.title == created_title)
        await wait_for_widget(pilot, f"#card-{created_task.id}", timeout=10.0)


@pytest.mark.asyncio
async def test_planner_header_task_count_refreshes_after_external_task_create(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        assert isinstance(kanban, KanbanScreen)
        initial_tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        initial_count = len(initial_tasks)

        kanban.action_open_planner()
        planner = await wait_for_screen(pilot, PlannerScreen, timeout=10.0)
        assert isinstance(planner, PlannerScreen)
        await wait_for_planner_ready(pilot, timeout=10.0)

        await app.ctx.task_service.create_task(
            "External planner-count refresh task",
            "created outside planner UI",
            project_id=app.ctx.active_project_id,
            created_by=None,
        )

        await wait_until(
            lambda: (
                f"📋 {initial_count + 1} tasks"
                in str(planner.query_one("#header-stats", Label).render())
            ),
            timeout=8.0,
            description="planner header task count updates after external create",
        )


@pytest.mark.asyncio
async def test_planner_empty_hints_persist_until_first_message(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        kanban.action_open_planner()
        await wait_for_screen(pilot, PlannerScreen, timeout=10.0)
        await wait_for_planner_ready(pilot, timeout=10.0)

        await pilot.pause(0.6)

        planner = cast("PlannerScreen", pilot.app.screen)
        output = planner.query_one("#planner-output", StreamingOutput)
        assert output.has_class("visible") is False, (
            "planner output became visible before first user submit "
            f"(has_pending={planner._state.has_pending_plan}, "
            f"history={len(planner._state.conversation_history)}, "
            f"has_output={planner._state.has_output})"
        )
        assert planner.query_one(".planner-empty-state").has_class("hidden") is False


@pytest.mark.asyncio
async def test_planner_header_task_count_updates_after_external_create_and_delete(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        kanban.action_open_planner()
        planner = cast("PlannerScreen", await wait_for_screen(pilot, PlannerScreen, timeout=10.0))

        await wait_until(
            lambda: "📋 3 tasks" in _header_stats_text(planner),
            timeout=8.0,
            description="planner header starts with current task count",
        )

        created = await app.ctx.task_service.create_task(
            "Externally created task",
            "Task created outside planner flow to validate live header count sync.",
            project_id=app.ctx.active_project_id,
        )

        await wait_until(
            lambda: "📋 4 tasks" in _header_stats_text(planner),
            timeout=8.0,
            description="planner header count updates after external task creation",
        )

        deleted = await app.ctx.task_service.delete_task(created.id)
        assert deleted is True

        await wait_until(
            lambda: "📋 3 tasks" in _header_stats_text(planner),
            timeout=8.0,
            description="planner header count updates after external task deletion",
        )


@pytest.mark.skipif(sys.platform == "win32", reason="Timing-sensitive; flaky on Windows CI")
@pytest.mark.asyncio
async def test_planner_uses_active_repo_context(tmp_path: Path) -> None:
    repo_a = tmp_path / "repo-a"
    repo_b = tmp_path / "repo-b"
    repo_a.mkdir()
    repo_b.mkdir()
    await init_git_repo_with_commit(repo_a)
    await init_git_repo_with_commit(repo_b)

    app = await _bootstrap_planner_app(
        tmp_path,
        repos=[repo_a, repo_b],
        project_name="Planner Repo Context",
    )

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, PlannerScreen, timeout=10.0)
        await wait_for_planner_ready(pilot, timeout=10.0)

        screen = cast("PlannerScreen", pilot.app.screen)
        screen.query_one(PlannerInput).focus()
        await type_text(pilot, "where")
        await pilot.press("enter")
        await pilot.pause()
        await pilot.pause()
        assert str(repo_a) in _read_planner_output(screen)

        await pilot.press("ctrl+r")
        await wait_for_screen(pilot, RepoPickerScreen)
        from textual.widgets import ListView

        picker = cast("RepoPickerScreen", pilot.app.screen)
        list_view = picker.query_one("#repo-list", ListView)
        await wait_until(
            lambda: len(list_view.children) >= 2,
            timeout=5.0,
            description="repo picker list to load",
        )
        target_idx = next(
            idx
            for idx, item in enumerate(picker._repo_items)
            if Path(item.repo.path).resolve() == repo_b.resolve()
        )
        picker.dismiss(picker._repo_items[target_idx].repo.id)
        await pilot.pause()
        await wait_for_screen(pilot, PlannerScreen)
        await wait_for_planner_ready(pilot, timeout=10.0)
        assert pilot.app.project_root.resolve() == repo_b.resolve()
        await wait_until(
            lambda: (
                isinstance(pilot.app.screen, PlannerScreen)
                and str(
                    getattr(
                        pilot.app.screen._state.agent,
                        "project_root",
                        "",
                    )
                )
                == str(pilot.app.project_root)
            ),
            timeout=5.0,
            description="planner agent context to switch repos",
        )

        screen = cast("PlannerScreen", pilot.app.screen)
        screen.query_one(PlannerInput).focus()
        await type_text(pilot, "where")
        await pilot.press("enter")
        await pilot.pause()
        await pilot.pause()
        assert str(repo_b) in _read_planner_output(screen)


@pytest.mark.asyncio
async def test_planner_sigterm_stream_end_is_not_rendered_as_error(
    tmp_path: Path,
) -> None:
    repo = tmp_path / "repo"
    repo.mkdir()
    await init_git_repo_with_commit(repo)

    app = await _bootstrap_planner_app(
        tmp_path,
        repos=[repo],
        project_name="Planner SIGTERM",
    )

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, PlannerScreen, timeout=10.0)
        await wait_for_planner_ready(pilot, timeout=10.0)

        screen = cast("PlannerScreen", pilot.app.screen)
        screen.post_message(messages.AgentFail("Agent exited with code -15"))
        await pilot.pause()
        await pilot.pause()

        rendered = screen.query_one("#planner-output", StreamingOutput).get_text_content()
        status = screen.query_one("#planner-status-bar", StatusBar)
        planner_input = screen.query_one(PlannerInput)
        assert "Agent stream ended by cancellation (SIGTERM)." in rendered
        assert "Error: Agent exited with code -15" not in rendered
        assert status.status == "ready"
        assert planner_input.disabled is False


@pytest.mark.asyncio
async def test_planner_slash_complete_opens_without_reactive_error(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        kanban.action_open_planner()
        await wait_for_screen(pilot, PlannerScreen, timeout=10.0)
        await wait_for_planner_ready(pilot, timeout=10.0)

        planner = cast("PlannerScreen", pilot.app.screen)
        planner.query_one(PlannerInput).focus()
        await type_text(pilot, "/")
        await wait_for_widget(pilot, "#slash-complete", timeout=5.0)
        planner.query_one("#slash-complete", SlashComplete)


@pytest.mark.asyncio
async def test_planner_submit_uses_grouped_exclusive_worker(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        kanban.action_open_planner()
        await wait_for_screen(pilot, PlannerScreen, timeout=10.0)
        await wait_for_planner_ready(pilot, timeout=10.0)

        planner = cast("PlannerScreen", pilot.app.screen)
        calls: list[dict[str, object]] = []

        def _capture_run_worker(work, **kwargs):
            calls.append(kwargs)
            if hasattr(work, "close"):
                work.close()
            return None

        original_run_worker = planner.run_worker
        cast("Any", planner).run_worker = _capture_run_worker
        try:
            await planner._submit_prompt("Create one task from planner")
        finally:
            cast("Any", planner).run_worker = original_run_worker

        assert calls
        assert calls[0].get("group") == "planner-send-to-agent"
        assert calls[0].get("exclusive") is True
        assert calls[0].get("exit_on_error") is False
