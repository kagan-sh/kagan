from __future__ import annotations

import asyncio
from contextlib import nullcontext
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import TYPE_CHECKING, cast
from unittest.mock import AsyncMock, Mock, call

import pytest
from tests.helpers.wait import wait_for_modal, wait_for_screen, wait_for_widget, wait_until
from textual.css.query import NoMatches
from textual.widgets import Input, Label

from kagan.core.constants import COLUMN_ORDER
from kagan.core.events import AutomationTaskStarted
from kagan.core.models.enums import (
    CardIndicator,
    ExecutionRunReason,
    ExecutionStatus,
    SessionType,
    TaskStatus,
    TaskType,
)
from kagan.core.services.jobs import JobRecord, JobStatus
from kagan.core.time import utc_now
from kagan.tui.ui.modals.tmux_gateway import PairInstructionsModal
from kagan.tui.ui.screens.kanban import KanbanScreen
from kagan.tui.ui.screens.kanban.board_controller import (
    BoardSyncState,
    compute_board_task_diff,
    count_blocked_backlog_tasks,
    group_tasks_by_status,
    task_content_hash,
    transition_board_sync_state,
)
from kagan.tui.ui.widgets.card import TaskCard
from kagan.tui.ui.widgets.column import KanbanColumn
from kagan.tui.ui.widgets.peek_overlay import PeekOverlay

if TYPE_CHECKING:
    from pytest import MonkeyPatch


def _job_record(
    *,
    job_id: str,
    task_id: str,
    action: str,
    status: JobStatus,
    result: dict[str, object] | None = None,
    message: str | None = None,
) -> JobRecord:
    now = utc_now()
    return JobRecord(
        job_id=job_id,
        task_id=task_id,
        action=action,
        status=status,
        created_at=now,
        updated_at=now,
        params={"task_id": task_id},
        result=result,
        message=message,
    )


@dataclass(frozen=True)
class _TaskStub:
    id: str
    status: TaskStatus
    title: str = "task"
    description: str = ""
    updated_at: datetime = datetime(2026, 1, 1, tzinfo=UTC)


def _column_has_task(screen: KanbanScreen, status: TaskStatus, task_id: str) -> bool:
    column = screen.query_one(f"#column-{status.value.lower()}", KanbanColumn)
    return any(
        card.task_model is not None and card.task_model.id == task_id for card in column.get_cards()
    )


def _card_indicator(screen: KanbanScreen, task_id: str) -> CardIndicator | None:
    try:
        card = screen.query_one(f"#card-{task_id}", TaskCard)
    except NoMatches:
        return None
    return card.indicator


def _card_missing(screen: KanbanScreen, selector: str) -> bool:
    try:
        screen.query_one(selector, TaskCard)
    except NoMatches:
        return True
    return False


@pytest.mark.asyncio
async def test_start_agent_updates_card_indicator_immediately(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = await wait_for_screen(pilot, KanbanScreen, timeout=10.0)

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        target = next(task for task in tasks if task.status == TaskStatus.IN_PROGRESS)
        await app.ctx.task_service.update_fields(target.id, task_type=TaskType.AUTO)

        monkeypatch.setattr(
            app.ctx.workspace_service,
            "get_path",
            AsyncMock(return_value=Path(app.project_root)),
        )

        submit_mock = AsyncMock(
            return_value=_job_record(
                job_id="job-start",
                task_id=target.id,
                action="start_agent",
                status=JobStatus.QUEUED,
            )
        )
        wait_result = _job_record(
            job_id="job-start",
            task_id=target.id,
            action="start_agent",
            status=JobStatus.SUCCEEDED,
            result={
                "success": True,
                "code": "STARTED",
                "message": "Agent running",
                "runtime": {"is_running": True},
            },
            message="Agent running",
        )

        async def _wait_side_effect(*_args, **_kwargs):
            # Mirror what the core start_agent handler does in production:
            # mark the task as running in the runtime service so that the
            # background sync_agent_states() keeps the RUNNING indicator.
            app.ctx.runtime_service.mark_started(target.id)
            return wait_result

        wait_mock = AsyncMock(side_effect=_wait_side_effect)
        direct_spawn_mock = AsyncMock(return_value=False)
        monkeypatch.setattr(app.ctx.job_service, "submit", submit_mock)
        monkeypatch.setattr(app.ctx.job_service, "wait", wait_mock)
        monkeypatch.setattr(app.ctx.automation_service, "spawn_for_task", direct_spawn_mock)

        await pilot.pause(0.25)

        card = kanban.query_one(f"#card-{target.id}", TaskCard)
        card.focus()
        await pilot.pause()
        await wait_until(
            lambda: card.task_model is not None and card.task_model.task_type == TaskType.AUTO,
            timeout=5.0,
            description="focused card reflects AUTO task type",
        )
        assert card.indicator == CardIndicator.IDLE

        kanban.action_start_agent()
        await wait_until(
            lambda: submit_mock.await_count > 0,
            timeout=5.0,
            description="start agent action dispatched",
        )
        await wait_until(
            lambda: card.indicator == CardIndicator.RUNNING,
            timeout=5.0,
            description="running indicator after start job result",
        )
        submit_mock.assert_awaited_once_with(
            task_id=target.id,
            action="start_agent",
            params={"task_id": target.id},
        )
        wait_mock.assert_awaited_once_with("job-start", task_id=target.id, timeout_seconds=0.6)
        direct_spawn_mock.assert_not_awaited()


@pytest.mark.asyncio
async def test_stop_agent_non_terminal_job_keeps_running_indicator(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = await wait_for_screen(pilot, KanbanScreen, timeout=10.0)

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        target = next(task for task in tasks if task.status == TaskStatus.IN_PROGRESS)
        await app.ctx.task_service.update_fields(target.id, task_type=TaskType.AUTO)
        app.ctx.runtime_service.mark_started(target.id)

        await pilot.pause(0.25)

        card = kanban.query_one(f"#card-{target.id}", TaskCard)
        card.focus()
        await pilot.pause()
        await wait_until(
            lambda: card.indicator == CardIndicator.RUNNING,
            timeout=5.0,
            description="focused card shows running state before stop request",
        )
        app.clear_notifications()

        submit_mock = AsyncMock(
            return_value=_job_record(
                job_id="job-stop",
                task_id=target.id,
                action="stop_agent",
                status=JobStatus.QUEUED,
            )
        )
        wait_mock = AsyncMock(
            return_value=_job_record(
                job_id="job-stop",
                task_id=target.id,
                action="stop_agent",
                status=JobStatus.RUNNING,
            )
        )
        monkeypatch.setattr(app.ctx.job_service, "submit", submit_mock)
        monkeypatch.setattr(app.ctx.job_service, "wait", wait_mock)

        await kanban.action_stop_agent()
        await wait_until(
            lambda: submit_mock.await_count > 0,
            timeout=5.0,
            description="stop agent action dispatched",
        )
        await wait_until(
            lambda: any(
                note.message == "Agent stop requested; waiting for scheduler."
                for note in app._notifications
            ),
            timeout=5.0,
            description="pending stop status message is shown",
        )

        assert card.indicator == CardIndicator.RUNNING
        wait_mock.assert_awaited_once_with("job-stop", task_id=target.id, timeout_seconds=0.6)


@pytest.mark.asyncio
async def test_done_task_shows_passed_indicator(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = await wait_for_screen(pilot, KanbanScreen, timeout=10.0)

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        target = next(task for task in tasks if task.status == TaskStatus.IN_PROGRESS)
        await app.ctx.task_service.update_fields(target.id, task_type=TaskType.AUTO)
        await app.ctx.task_service.move(target.id, TaskStatus.DONE)

        await pilot.pause(0.25)

        card = kanban.query_one(f"#card-{target.id}", TaskCard)
        assert card.indicator == CardIndicator.PASSED


@pytest.mark.asyncio
async def test_mcp_started_auto_run_updates_card_indicator_from_persisted_runtime(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = await wait_for_screen(pilot, KanbanScreen, timeout=10.0)

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        target = next(task for task in tasks if task.status == TaskStatus.IN_PROGRESS)
        await app.ctx.task_service.update_fields(target.id, task_type=TaskType.AUTO)

        await pilot.pause(0.25)
        card = kanban.query_one(f"#card-{target.id}", TaskCard)
        assert card.indicator == CardIndicator.IDLE

        project_id = app.ctx.active_project_id
        assert project_id is not None
        workspace_id = await app.ctx.workspace_service.provision_for_project(target.id, project_id)
        session_record = await app.ctx.task_service.create_session_record(
            workspace_id=workspace_id,
            session_type=SessionType.SCRIPT,
            external_id=f"task:{target.id}",
        )
        execution = await app.ctx.execution_service.create_execution(
            session_id=session_record.id,
            run_reason=ExecutionRunReason.CODINGAGENT,
        )

        await wait_until(
            lambda: card.indicator == CardIndicator.RUNNING,
            timeout=8.0,
            description="card indicator updates to RUNNING for persisted execution",
        )
        card.focus()
        await pilot.pause()
        assert kanban.check_action("stop_agent", ()) is True

        await app.ctx.execution_service.update_execution(
            execution.id,
            status=ExecutionStatus.COMPLETED,
            completed_at=utc_now(),
        )
        await wait_until(
            lambda: card.indicator == CardIndicator.IDLE,
            timeout=8.0,
            description="card indicator resets after persisted execution completes",
        )


def test_compute_board_task_diff_tracks_changes_and_deletions() -> None:
    previous_tasks = (
        _TaskStub(id="task-1", status=TaskStatus.BACKLOG),
        _TaskStub(id="task-2", status=TaskStatus.IN_PROGRESS),
    )
    previous_hashes = {task.id: task_content_hash(task) for task in previous_tasks}
    previous_status = {task.id: task.status for task in previous_tasks}

    new_tasks = (
        _TaskStub(id="task-1", status=TaskStatus.REVIEW),
        _TaskStub(id="task-3", status=TaskStatus.DONE),
    )
    diff = compute_board_task_diff(
        previous_hashes=previous_hashes,
        previous_status_by_id=previous_status,
        new_tasks=new_tasks,
    )

    assert diff.has_task_mutation is True
    assert diff.changed_ids == {"task-1", "task-3"}
    assert diff.deleted_ids == {"task-2"}
    assert diff.affected_statuses == set(COLUMN_ORDER)


def test_group_tasks_by_status_prioritizes_blocked_backlog_and_counts() -> None:
    tasks = (
        _TaskStub(id="backlog-1", status=TaskStatus.BACKLOG),
        _TaskStub(id="backlog-2", status=TaskStatus.BACKLOG),
        _TaskStub(id="backlog-3", status=TaskStatus.BACKLOG),
        _TaskStub(id="in-progress-1", status=TaskStatus.IN_PROGRESS),
    )

    grouped = group_tasks_by_status(
        tasks,
        blocked_backlog_timestamps={
            "backlog-2": 15.0,
            "backlog-3": 5.0,
        },
    )
    assert [task.id for task in grouped[TaskStatus.BACKLOG]] == [
        "backlog-3",
        "backlog-2",
        "backlog-1",
    ]

    blocked_count = count_blocked_backlog_tasks(
        tasks,
        blocked_backlog_ids={"backlog-2", "backlog-3"},
    )
    assert blocked_count == 2


def test_transition_board_sync_state_moves_between_fast_and_idle() -> None:
    transition = transition_board_sync_state(
        current_state=BoardSyncState.IDLE,
        fast_ticks_remaining=0,
        has_activity=True,
    )
    assert transition.state is BoardSyncState.FAST
    assert transition.fast_ticks_remaining == 5

    transition = transition_board_sync_state(
        current_state=BoardSyncState.FAST,
        fast_ticks_remaining=2,
        has_activity=False,
    )
    assert transition.state is BoardSyncState.FAST
    assert transition.fast_ticks_remaining == 1

    transition = transition_board_sync_state(
        current_state=BoardSyncState.FAST,
        fast_ticks_remaining=1,
        has_activity=False,
    )
    assert transition.state is BoardSyncState.IDLE
    assert transition.fast_ticks_remaining == 0


@pytest.mark.asyncio
async def test_external_task_move_refreshes_column_membership(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        task = next(t for t in tasks if t.status == TaskStatus.BACKLOG)

        assert _column_has_task(kanban, TaskStatus.BACKLOG, task.id)

        await app.ctx.task_service.move(task.id, TaskStatus.IN_PROGRESS)

        await wait_until(
            lambda: (
                _column_has_task(kanban, TaskStatus.IN_PROGRESS, task.id)
                and not _column_has_task(kanban, TaskStatus.BACKLOG, task.id)
            ),
            timeout=5.0,
            description="task moved to in-progress column",
        )


@pytest.mark.asyncio
async def test_external_repo_delete_removes_card_without_manual_refresh(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        task = next(t for t in tasks if t.status == TaskStatus.REVIEW)
        selector = f"#card-{task.id}"
        await wait_for_widget(pilot, selector, timeout=5.0)

        repo = app.ctx._task_repo
        assert repo is not None
        deleted = await repo.delete(task.id)
        assert deleted is True

        await wait_until(
            lambda: _card_missing(kanban, selector),
            timeout=5.0,
            description="task card removed from board after external repo delete",
        )


@pytest.mark.asyncio
async def test_external_agent_start_event_updates_indicator(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        task = next(t for t in tasks if t.status == TaskStatus.IN_PROGRESS)
        await app.ctx.task_service.update_fields(task.id, task_type=TaskType.AUTO)

        await wait_until(
            lambda: _card_indicator(kanban, task.id) == CardIndicator.IDLE,
            timeout=5.0,
            description="auto card idle indicator",
        )

        app.ctx.runtime_service.mark_started(task.id)
        await app.ctx.event_bus.publish(AutomationTaskStarted(task_id=task.id))

        await wait_until(
            lambda: _card_indicator(kanban, task.id) == CardIndicator.RUNNING,
            timeout=5.0,
            description="auto card running indicator after external start event",
        )


@pytest.mark.asyncio
async def test_external_runtime_change_updates_indicator_without_event(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        task = next(t for t in tasks if t.status == TaskStatus.IN_PROGRESS)
        await app.ctx.task_service.update_fields(task.id, task_type=TaskType.AUTO)

        await wait_until(
            lambda: _card_indicator(kanban, task.id) == CardIndicator.IDLE,
            timeout=5.0,
            description="auto card idle indicator before external runtime change",
        )

        app.ctx.runtime_service.mark_started(task.id)

        await wait_until(
            lambda: _card_indicator(kanban, task.id) == CardIndicator.RUNNING,
            timeout=5.0,
            description="auto card running indicator after external runtime change",
        )


@pytest.mark.asyncio
async def test_blocked_backlog_tasks_are_prioritized_and_counted(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        blocked_target = next(task for task in tasks if task.status == TaskStatus.IN_PROGRESS)
        await app.ctx.task_service.update_fields(blocked_target.id, task_type=TaskType.AUTO)
        await app.ctx.task_service.move(blocked_target.id, TaskStatus.BACKLOG)

        app.ctx.runtime_service.mark_blocked(
            blocked_target.id,
            reason="Waiting for overlapping task to merge.",
            blocked_by_task_ids=("dep00001",),
            overlap_hints=("src/calculator/core.py",),
        )

        def _is_prioritized() -> bool:
            backlog = kanban.query_one("#column-backlog", KanbanColumn)
            cards = backlog.get_cards()
            return bool(
                cards and cards[0].task_model and cards[0].task_model.id == blocked_target.id
            )

        await wait_until(
            _is_prioritized,
            timeout=6.0,
            description="blocked backlog task is pinned to top",
        )

        await wait_until(
            lambda: "blocked 1" in str(kanban.query_one("#header-backlog", Label).render()),
            timeout=6.0,
            description="backlog header shows blocked count",
        )


@pytest.mark.asyncio
async def test_search_exclusive_worker_cancels_stale_query(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: MonkeyPatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))

        release_slow_query = asyncio.Event()
        slow_query_started = asyncio.Event()
        cancelled_queries: list[str] = []

        async def fake_search(query: str):
            if query == "slow-query":
                slow_query_started.set()
                try:
                    await release_slow_query.wait()
                except asyncio.CancelledError:
                    cancelled_queries.append(query)
                    raise
            return []

        monkeypatch.setattr(app.ctx.task_service, "search", fake_search)

        kanban.action_toggle_search()
        await pilot.pause()
        search_input = kanban.query_one("#search-input", Input)
        search_input.value = "slow-query"
        await pilot.pause()
        await wait_until(
            lambda: slow_query_started.is_set(),
            timeout=5.0,
            description="slow search query starts",
        )

        search_input.value = "new-query"
        await pilot.pause()
        await wait_until(
            lambda: "slow-query" in cancelled_queries,
            timeout=5.0,
            description="stale search worker cancelled by exclusive replacement",
        )
        await wait_until(
            lambda: kanban._ui_state.filtered_tasks == [],
            timeout=5.0,
            description="latest search result applied",
        )

        release_slow_query.set()


@pytest.mark.asyncio
async def test_hiding_search_cancels_inflight_query_and_resets_filter(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: MonkeyPatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))

        release_query = asyncio.Event()
        query_started = asyncio.Event()
        cancelled_queries: list[str] = []

        async def fake_search(query: str):
            if query == "linger-query":
                query_started.set()
                try:
                    await release_query.wait()
                except asyncio.CancelledError:
                    cancelled_queries.append(query)
                    raise
            return []

        monkeypatch.setattr(app.ctx.task_service, "search", fake_search)

        kanban.action_toggle_search()
        await pilot.pause()
        search_input = kanban.query_one("#search-input", Input)
        search_input.value = "linger-query"
        await pilot.pause()
        await wait_until(
            lambda: query_started.is_set(),
            timeout=5.0,
            description="in-flight search query starts",
        )

        kanban.action_deselect()
        await pilot.pause()
        await wait_until(
            lambda: not kanban.search_visible and kanban._ui_state.filtered_tasks is None,
            timeout=5.0,
            description="search closes and filter is reset",
        )
        await wait_until(
            lambda: "linger-query" in cancelled_queries,
            timeout=5.0,
            description="in-flight search is cancelled when search UI closes",
        )

        release_query.set()


@pytest.mark.asyncio
async def test_kanban_github_sync_flow_surfaces_error_without_state_corruption(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: MonkeyPatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        project_id = app.ctx.active_project_id
        repo_id = app.ctx.active_repo_id
        assert project_id is not None
        assert repo_id is not None

        tasks = await app.ctx.task_service.list_tasks(project_id=project_id)
        task = next(t for t in tasks if t.status == TaskStatus.BACKLOG)
        card = kanban.query_one(f"#card-{task.id}", TaskCard)
        card.focus()
        await pilot.pause()

        refresh_board = AsyncMock()
        plugin_ui_catalog = AsyncMock(
            return_value={
                "schema_version": "1",
                "actions": [
                    {
                        "plugin_id": "official.github",
                        "action_id": "sync_issues",
                        "surface": "kanban.repo_actions",
                        "label": "Sync GitHub Issues",
                        "operation": {"capability": "kagan_github", "method": "sync_issues"},
                    }
                ],
                "forms": [],
                "badges": [],
            }
        )
        monkeypatch.setattr(kanban._board, "refresh_board", refresh_board)
        monkeypatch.setattr(kanban.ctx.api, "plugin_ui_catalog", plugin_ui_catalog)
        plugin_ui_invoke = AsyncMock(
            return_value={
                "ok": False,
                "code": "GH_NOT_CONNECTED",
                "message": "GitHub not connected",
                "data": {"hint": "Run connect repo first"},
                "refresh": {"repo": False, "tasks": False, "sessions": False},
            }
        )
        monkeypatch.setattr(kanban.ctx.api, "plugin_ui_invoke", plugin_ui_invoke)

        notify_calls: list[tuple[str, str]] = []
        original_notify = kanban.notify

        def _capture_notify(
            message: str, *, severity: str = "information", **kwargs: object
        ) -> None:
            notify_calls.append((message, severity))
            original_notify(message, severity=severity, **kwargs)

        monkeypatch.setattr(kanban, "notify", _capture_notify)
        await kanban._refresh_plugin_ui_catalog(force=True)
        kanban.action_github_sync()
        await wait_until(
            lambda: any(message for message, _severity in notify_calls),
            timeout=5.0,
            description="GitHub sync not-connected notification",
        )

        plugin_ui_invoke.assert_awaited_once_with(
            project_id=project_id,
            repo_id=repo_id,
            plugin_id="official.github",
            action_id="sync_issues",
            inputs=None,
        )
        refresh_board.assert_not_awaited()
        focused = kanban.get_focused_card()
        assert focused is not None and focused.task_model is not None
        assert focused.task_model.id == task.id
        assert any(
            "GitHub not connected (Run connect repo first)" in message and severity == "warning"
            for message, severity in notify_calls
        )


@pytest.mark.asyncio
async def test_kanban_peek_toggle_preserves_selection_and_focus(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: MonkeyPatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        task = next(t for t in tasks if t.status == TaskStatus.IN_PROGRESS)
        await app.ctx.task_service.update_fields(task.id, task_type=TaskType.AUTO)
        await kanban._board.refresh_board()
        await wait_for_widget(pilot, f"#card-{task.id}", timeout=6.0)

        card = kanban.query_one(f"#card-{task.id}", TaskCard)
        card.focus()
        await pilot.pause()
        focused_before = pilot.app.focused

        task_model = await app.ctx.task_service.get_task(task.id)
        assert task_model is not None
        monkeypatch.setattr(
            kanban.ctx.api, "get_scratchpad", AsyncMock(return_value="scratchpad text")
        )

        overlay = kanban.query_one("#peek-overlay", PeekOverlay)
        assert overlay.has_class("visible") is False

        await kanban._toggle_peek_flow(task_model)
        assert overlay.has_class("visible") is True
        focused = kanban.get_focused_card()
        assert focused is not None and focused.task_model is not None
        assert focused.task_model.id == task.id
        assert pilot.app.focused is focused_before

        await kanban._toggle_peek_flow(task_model)
        assert overlay.has_class("visible") is False
        focused = kanban.get_focused_card()
        assert focused is not None and focused.task_model is not None
        assert focused.task_model.id == task.id
        assert pilot.app.focused is focused_before


@pytest.mark.asyncio
async def test_external_task_status_change_shows_toast(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        app.clear_notifications()

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        task = next(t for t in tasks if t.status == TaskStatus.BACKLOG)

        await app.ctx.task_service.move(task.id, TaskStatus.IN_PROGRESS)
        await wait_until(
            lambda: any(
                f"#{task.short_id}" in n.message and "BACKLOG -> IN PROGRESS" in n.message
                for n in app._notifications
            ),
            timeout=2.0,
            check_interval=0.05,
            description="status change notification",
        )

        messages = [n.message for n in app._notifications]
        assert any(
            f"#{task.short_id}" in message and "BACKLOG -> IN PROGRESS" in message
            for message in messages
        )


@pytest.mark.asyncio
async def test_switch_pair_to_auto_kills_active_session(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: MonkeyPatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        task = (await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id))[0]

        session_exists = AsyncMock(side_effect=[True, False])
        kill_session = AsyncMock(return_value=None)
        monkeypatch.setattr(app.ctx.session_service, "session_exists", session_exists)
        monkeypatch.setattr(app.ctx.session_service, "kill_session", kill_session)

        await kanban._save_task_modal_changes({"task_type": TaskType.AUTO}, editing_task_id=task.id)

        updated = await app.ctx.task_service.get_task(task.id)
        assert updated is not None
        assert updated.task_type == TaskType.AUTO
        assert updated.terminal_backend is None
        assert session_exists.await_count == 2
        kill_session.assert_awaited_once_with(task.id)


@pytest.mark.asyncio
async def test_switch_auto_to_pair_stops_running_agent_via_job_flow(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: MonkeyPatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        task = (await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id))[0]
        await app.ctx.task_service.update_fields(task.id, task_type=TaskType.AUTO)

        running_state = {"running": True}
        submit_mock = AsyncMock(
            return_value=_job_record(
                job_id="job-stop-type-switch",
                task_id=task.id,
                action="stop_agent",
                status=JobStatus.QUEUED,
            )
        )

        async def _wait(*_args, **_kwargs) -> JobRecord:
            running_state["running"] = False
            return _job_record(
                job_id="job-stop-type-switch",
                task_id=task.id,
                action="stop_agent",
                status=JobStatus.SUCCEEDED,
                result={
                    "success": True,
                    "code": "STOP_QUEUED",
                    "message": "Agent stop queued",
                    "runtime": {"is_running": False},
                },
                message="Agent stop queued",
            )

        wait_mock = AsyncMock(side_effect=_wait)
        direct_stop_mock = AsyncMock(return_value=True)
        monkeypatch.setattr(app.ctx.job_service, "submit", submit_mock)
        monkeypatch.setattr(app.ctx.job_service, "wait", wait_mock)
        monkeypatch.setattr(
            app.ctx.automation_service,
            "is_running",
            lambda _task_id: running_state["running"],
        )
        monkeypatch.setattr(app.ctx.automation_service, "stop_task", direct_stop_mock)

        await kanban._save_task_modal_changes({"task_type": TaskType.PAIR}, editing_task_id=task.id)

        updated = await app.ctx.task_service.get_task(task.id)
        assert updated is not None
        assert updated.task_type == TaskType.PAIR
        submit_mock.assert_awaited_once_with(
            task_id=task.id,
            action="stop_agent",
            params={"task_id": task.id},
        )
        wait_mock.assert_awaited_once_with(
            "job-stop-type-switch",
            task_id=task.id,
            timeout_seconds=0.6,
        )
        direct_stop_mock.assert_not_awaited()


@pytest.mark.asyncio
async def test_switch_auto_to_pair_waits_for_terminal_stop_before_updating_task_type(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: MonkeyPatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        task = (await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id))[0]
        await app.ctx.task_service.update_fields(task.id, task_type=TaskType.AUTO)

        submit_mock = AsyncMock(
            return_value=_job_record(
                job_id="job-stop-pending",
                task_id=task.id,
                action="stop_agent",
                status=JobStatus.QUEUED,
            )
        )
        wait_mock = AsyncMock(
            return_value=_job_record(
                job_id="job-stop-pending",
                task_id=task.id,
                action="stop_agent",
                status=JobStatus.RUNNING,
            )
        )
        monkeypatch.setattr(app.ctx.job_service, "submit", submit_mock)
        monkeypatch.setattr(app.ctx.job_service, "wait", wait_mock)
        monkeypatch.setattr(app.ctx.automation_service, "is_running", lambda _task_id: True)

        app.clear_notifications()
        await kanban._save_task_modal_changes({"task_type": TaskType.PAIR}, editing_task_id=task.id)

        updated = await app.ctx.task_service.get_task(task.id)
        assert updated is not None
        assert updated.task_type == TaskType.AUTO
        submit_mock.assert_awaited_once_with(
            task_id=task.id,
            action="stop_agent",
            params={"task_id": task.id},
        )
        wait_mock.assert_awaited_once_with(
            "job-stop-pending",
            task_id=task.id,
            timeout_seconds=0.6,
        )
        assert any(
            note.message == "Agent stop requested; waiting for scheduler."
            for note in app._notifications
        )


@pytest.mark.asyncio
async def test_open_session_flow_shows_instructions_popup_for_external_launcher(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: MonkeyPatch,
    tmp_path: Path,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        screen = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        screen.kagan_app.config.ui.skip_pair_instructions = False

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        task = next(
            t for t in tasks if t.task_type == TaskType.PAIR and t.status == TaskStatus.BACKLOG
        )

        monkeypatch.setattr("kagan.core.agents.installer.check_agent_installed", lambda _name: True)
        monkeypatch.setattr(app.ctx.workspace_service, "get_path", AsyncMock(return_value=tmp_path))
        monkeypatch.setattr(
            screen._session,
            "resolve_pair_terminal_backend",
            lambda _task: "vscode",
        )

        session_exists = AsyncMock(return_value=True)
        monkeypatch.setattr(app.ctx.session_service, "session_exists", session_exists)

        do_open_pair = AsyncMock()
        monkeypatch.setattr(screen._session, "do_open_pair_session", do_open_pair)

        screen.run_worker(
            screen._session.open_session_flow(task),
            group="test-session-flow-external-launcher",
            exclusive=True,
            exit_on_error=False,
        )
        modal = cast(
            "PairInstructionsModal",
            await wait_for_modal(pilot, PairInstructionsModal, timeout=5.0),
        )
        modal.dismiss(None)
        await pilot.pause()
        do_open_pair.assert_not_awaited()


@pytest.mark.asyncio
async def test_external_launcher_attach_does_not_prompt_session_complete(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: MonkeyPatch,
    tmp_path: Path,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        screen = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        task = next(
            t for t in tasks if t.task_type == TaskType.PAIR and t.status == TaskStatus.IN_PROGRESS
        )

        attach_session = AsyncMock(return_value=True)
        session_exists = AsyncMock(return_value=False)
        monkeypatch.setattr(app.ctx.session_service, "attach_session", attach_session)
        monkeypatch.setattr(app.ctx.session_service, "session_exists", session_exists)

        push_screen = Mock(return_value=True)
        monkeypatch.setattr(screen.app, "push_screen", push_screen)
        monkeypatch.setattr(screen.app, "suspend", lambda: nullcontext())

        notify = Mock()
        monkeypatch.setattr(screen, "notify", notify)

        await screen._session.do_open_pair_session(task, tmp_path, "vscode")

        attach_session.assert_awaited_once_with(task.id)
        push_screen.assert_not_called()
        notify.assert_called_once()
        assert "Workspace opened externally." in notify.call_args.args[0]
        assert "start_prompt.md" in notify.call_args.args[0]


@pytest.mark.asyncio
async def test_tmux_attach_retries_after_missing_session_recreation(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: MonkeyPatch,
    tmp_path: Path,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        screen = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        task = next(
            t for t in tasks if t.task_type == TaskType.PAIR and t.status == TaskStatus.BACKLOG
        )
        await app.ctx.task_service.update_fields(task.id, status=TaskStatus.IN_PROGRESS)
        refreshed = await app.ctx.task_service.get_task(task.id)
        assert refreshed is not None
        task = refreshed

        local_tmux_attach = AsyncMock(side_effect=[False, True])
        core_attach = AsyncMock(return_value=False)
        session_exists = AsyncMock(side_effect=[False, True])
        create_session = AsyncMock(return_value={"session_name": f"kagan-{task.id}"})

        monkeypatch.setattr(screen._session, "_attach_tmux_session_local", local_tmux_attach)
        monkeypatch.setattr(screen.ctx.api, "attach_session", core_attach)
        monkeypatch.setattr(screen.ctx.api, "session_exists", session_exists)
        monkeypatch.setattr(screen.ctx.api, "create_session", create_session)
        monkeypatch.setattr(screen.app, "suspend", lambda: nullcontext())

        notify = Mock()
        monkeypatch.setattr(screen, "notify", notify)

        await screen._session.do_open_pair_session(task, tmp_path, "tmux")

        assert local_tmux_attach.await_count == 2
        core_attach.assert_not_awaited()
        session_exists.assert_has_awaits([call(task.id), call(task.id)])
        create_session.assert_awaited_once_with(
            task.id,
            worktree_path=tmp_path,
            reuse_if_exists=False,
        )
        notify_messages = [str(item.args[0]) for item in notify.call_args_list if item.args]
        assert any(
            "PAIR session missing; recreating session..." in message for message in notify_messages
        )
        assert not any("Failed to open PAIR session" in message for message in notify_messages)


@pytest.mark.asyncio
async def test_merge_exception_shows_error_notification(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: MonkeyPatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))

        tasks = await app.ctx.api.list_tasks(project_id=app.ctx.active_project_id)
        task = next(t for t in tasks if t.status == TaskStatus.REVIEW)

        monkeypatch.setattr(
            kanban.ctx.api,
            "merge_task_direct",
            AsyncMock(side_effect=RuntimeError("push rejected")),
        )

        notify_calls: list[tuple[str, str]] = []
        original_notify = kanban.notify

        def _capture_notify(message: str, *, severity: str = "information", **kw: object) -> None:
            notify_calls.append((message, severity))
            original_notify(message, severity=severity, **kw)

        monkeypatch.setattr(kanban, "notify", _capture_notify)

        result = await kanban._review.execute_merge(task, success_msg="ok", track_failures=True)

        assert result is False
        assert task.id in kanban._merge_failed_tasks
        assert any("Merge error" in msg and sev == "error" for msg, sev in notify_calls)
