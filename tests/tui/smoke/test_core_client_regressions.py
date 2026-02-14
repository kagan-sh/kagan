from __future__ import annotations

from typing import TYPE_CHECKING, cast

import pytest
from tests.helpers.wait import wait_for_screen, wait_for_widget, wait_until_async
from textual.widgets import Button, Switch

from kagan.core.adapters.db.repositories import (
    ExecutionRepository,
    SessionRecordRepository,
    TaskRepository,
)
from kagan.core.models.enums import (
    ExecutionRunReason,
    ExecutionStatus,
    SessionType,
    TaskStatus,
    TaskType,
)
from kagan.core.services.workspaces import RepoWorkspaceInput
from kagan.tui.ui.modals.review import ReviewModal
from kagan.tui.ui.modals.settings import SettingsModal
from kagan.tui.ui.screens.kanban import KanbanScreen
from kagan.tui.ui.widgets.card import TaskCard
from kagan.tui.ui.widgets.chat_panel import ChatPanel

if TYPE_CHECKING:
    from kagan.tui.app import KaganApp


async def _wait_chat_contains(
    review: ReviewModal,
    expected_text: str,
    *,
    timeout: float = 8.0,
) -> str:
    rendered = ""

    async def _has_expected_text() -> bool:
        nonlocal rendered
        rendered = review.query_one(
            "#review-agent-output-chat", ChatPanel
        ).output.get_text_content()
        return expected_text in rendered

    await wait_until_async(
        _has_expected_text,
        timeout=timeout,
        check_interval=0.1,
        description=f"agent output to contain '{expected_text}'",
    )
    return rendered


@pytest.mark.asyncio
async def test_settings_modal_reflects_saved_values_without_restart(e2e_app_with_tasks) -> None:
    app = cast("KaganApp", e2e_app_with_tasks)

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))

        await kanban.action_open_settings()
        settings = cast("SettingsModal", await wait_for_screen(pilot, SettingsModal, timeout=5.0))
        switch = settings.query_one("#auto-review-switch", Switch)
        updated_value = not switch.value
        switch.value = updated_value
        await pilot.pause()
        settings.query_one("#save-btn", Button).press()

        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)

        await kanban.action_open_settings()
        settings = cast("SettingsModal", await wait_for_screen(pilot, SettingsModal, timeout=5.0))
        assert settings.query_one("#auto-review-switch", Switch).value is updated_value


@pytest.mark.asyncio
async def test_kanban_resume_recovers_after_core_client_disconnect(e2e_app_with_tasks) -> None:
    app = cast("KaganApp", e2e_app_with_tasks)

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        client = app._core_client
        assert client is not None

        await client.close()
        assert not client.is_connected

        await kanban.on_screen_resume()
        tasks = await app.ctx.api.list_tasks(project_id=app.ctx.active_project_id)
        assert tasks


@pytest.mark.asyncio
async def test_task_output_streams_incremental_logs_for_external_running_execution(
    e2e_app_with_tasks,
) -> None:
    app = cast("KaganApp", e2e_app_with_tasks)
    repo = TaskRepository(app.db_path, project_root=app.project_root)
    await repo.initialize()
    session_repo = SessionRecordRepository(repo.session_factory)
    execution_repo = ExecutionRepository(repo.session_factory)

    try:
        async with app.run_test(size=(120, 40)) as pilot:
            kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
            project_id = app.ctx.active_project_id
            assert project_id is not None

            task = await app.ctx.api.create_task(
                title="External run stream",
                description="",
                project_id=project_id,
                status=TaskStatus.IN_PROGRESS.value,
                task_type=TaskType.AUTO.value,
            )

            repo_rows = await app.ctx.api.get_project_repo_details(project_id)
            assert repo_rows
            active_repo_id = app.ctx.active_repo_id or str(repo_rows[0]["id"])
            repo_row = next(row for row in repo_rows if str(row["id"]) == active_repo_id)

            workspace_id = await app.ctx.api.provision_workspace(
                task_id=task.id,
                repos=[
                    RepoWorkspaceInput(
                        repo_id=str(repo_row["id"]),
                        repo_path=str(repo_row["path"]),
                        target_branch=str(repo_row["default_branch"]),
                    )
                ],
            )
            session_record = await session_repo.create_session_record(
                workspace_id=workspace_id,
                session_type=SessionType.SCRIPT,
                external_id=f"task:{task.id}",
            )
            execution = await execution_repo.create_execution(
                session_id=session_record.id,
                run_reason=ExecutionRunReason.CODINGAGENT,
            )
            await execution_repo.update_execution(execution.id, status=ExecutionStatus.RUNNING)

            await app.ctx.api.reconcile_running_tasks([task.id])
            await kanban._board.refresh_board()
            await wait_for_widget(pilot, f"#card-{task.id}", timeout=6.0)

            card = kanban.query_one(f"#card-{task.id}", TaskCard)
            card.focus()
            await pilot.pause()
            assert kanban.check_action("stop_agent", ()) is True
            await pilot.press("enter")
            review = cast("ReviewModal", await wait_for_screen(pilot, ReviewModal, timeout=10.0))

            await execution_repo.append_execution_log(
                execution.id,
                '{"messages":[{"type":"response","content":"external log line one"}]}',
            )
            rendered = await _wait_chat_contains(review, "external log line one")
            assert "external log line one" in rendered

            await execution_repo.append_execution_log(
                execution.id,
                '{"messages":[{"type":"response","content":"external log line two"}]}',
            )
            rendered = await _wait_chat_contains(review, "external log line two")
            assert "external log line two" in rendered
    finally:
        await repo.close()
