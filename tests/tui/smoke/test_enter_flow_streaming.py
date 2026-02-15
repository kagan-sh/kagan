from __future__ import annotations

import asyncio
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, cast

import pytest
from tests.helpers.mocks import NoopMessageAgent
from tests.helpers.wait import wait_for_modal, wait_until_async
from tests.tui.smoke._enter_flow_support import (
    focus_task_card,
    make_execution_log_entry,
    make_execution_stub,
    mark_auto_runtime_running,
    open_kanban,
    open_review_for_task,
    seed_enter_flow_app,
    wait_chat_contains,
    wait_static_contains,
)
from textual.widgets import RichLog, Static, TabbedContent

from kagan.core.acp import messages
from kagan.core.events import AutomationAgentAttached, AutomationReviewAgentAttached
from kagan.core.models.enums import ExecutionRunReason, ExecutionStatus, SessionType, TaskStatus
from kagan.core.services.diffs import FileDiff
from kagan.core.services.sessions import SessionServiceImpl
from kagan.tui.ui.modals.confirm import ConfirmModal
from kagan.tui.ui.widgets.chat_panel import ChatPanel

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.tui.app import KaganApp

pytestmark = pytest.mark.usefixtures("global_mock_tmux")


@pytest.mark.asyncio
async def test_enter_prompts_for_unready_tasks(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    async def _fast_attach(self: SessionServiceImpl, session_name: str) -> bool:
        del self, session_name
        return True

    monkeypatch.setattr(SessionServiceImpl, "_attach_tmux_session", _fast_attach)
    monkeypatch.setattr(
        "kagan.tui.terminals.installer.shutil.which",
        lambda _cmd, *_a, **_kw: "/usr/bin/mock",
    )

    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        for task_id in (task_ids.auto_backlog, task_ids.pair_in_progress):
            await focus_task_card(pilot, task_id)
            await pilot.press("enter")
            await wait_for_modal(pilot, ConfirmModal, timeout=5.0)
            await pilot.press("escape")

        await focus_task_card(pilot, task_ids.auto_in_progress)
        await pilot.press("enter")
        with pytest.raises(TimeoutError):
            await wait_for_modal(pilot, ConfirmModal, timeout=0.4)
        assert any(
            note.message == "No active AUTO run detected. Press 'a' to start the agent."
            for note in app._notifications
        )


@pytest.mark.asyncio
async def test_enter_auto_in_progress_opens_review_workspace_output_tab(tmp_path: Path) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress, agent=NoopMessageAgent())

        review = await open_review_for_task(pilot, task_ids.auto_in_progress)
        tabs = review.query_one("#review-tabs", TabbedContent)
        assert tabs.active == "review-agent-output"
        assert not review.query_one("#review-agent-output-chat").has_class("queue-disabled")
        review.query_one("#review-agent-output-chat .chat-input")


@pytest.mark.asyncio
async def test_enter_auto_in_progress_waiting_mode_with_execution_id_opens_output(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        automation = kagan_app.ctx.automation_service
        project_id = kagan_app.ctx.active_project_id
        assert project_id is not None

        workspace_id = await kagan_app.ctx.workspace_service.provision_for_project(
            task_ids.auto_in_progress,
            project_id,
        )
        session_record = await kagan_app.ctx.task_service.create_session_record(
            workspace_id=workspace_id,
            session_type=SessionType.SCRIPT,
            external_id=f"task:{task_ids.auto_in_progress}",
        )
        execution = await kagan_app.ctx.execution_service.create_execution(
            session_id=session_record.id,
            run_reason=ExecutionRunReason.CODINGAGENT,
        )

        async def _wait_for_running_agent(_task_id: str, **_kwargs) -> Any:
            del _kwargs
            return None

        monkeypatch.setattr(automation, "wait_for_running_agent", _wait_for_running_agent)

        review = await open_review_for_task(pilot, task_ids.auto_in_progress)
        tabs = review.query_one("#review-tabs", TabbedContent)
        assert tabs.active == "review-agent-output"

        rendered = await wait_chat_contains(
            pilot,
            "#review-agent-output-chat",
            "Waiting for live agent stream...",
        )
        assert "Waiting for live agent stream..." in rendered

        await kagan_app.ctx.execution_service.append_execution_log(
            execution.id,
            '{"messages":[{"type":"response","content":"persisted output from external run"}]}',
        )
        rendered = await wait_chat_contains(
            pilot,
            "#review-agent-output-chat",
            "persisted output from external run",
        )
        assert "persisted output from external run" in rendered


@pytest.mark.asyncio
async def test_enter_auto_backlog_blocked_opens_task_output_with_context(tmp_path: Path) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        kagan_app.ctx.runtime_service.mark_blocked(
            task_ids.auto_backlog,
            reason="Waiting on overlapping task changes.",
            blocked_by_task_ids=(task_ids.auto_in_progress,),
            overlap_hints=("src/calculator/core.py",),
        )

        review = await open_review_for_task(pilot, task_ids.auto_backlog)
        tabs = review.query_one("#review-tabs", TabbedContent)
        assert tabs.active == "review-agent-output"
        note = str(review.query_one("#agent-output-state-note", Static).render())
        assert "Blocked:" in note
        assert f"#{task_ids.auto_in_progress[:8]}" in note


@pytest.mark.asyncio
async def test_enter_review_and_done_modes_render_expected_controls(tmp_path: Path) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)

        review_auto = await open_review_for_task(pilot, task_ids.auto_review)
        review_auto.query_one("#review-agent-output-chat")
        await pilot.press("escape")

        review_pair = await open_review_for_task(pilot, task_ids.pair_review)
        review_pair.query_one("#attach-btn")
        await pilot.press("escape")

        review_done = await open_review_for_task(pilot, task_ids.auto_done)
        assert review_done.query_one(".button-row").has_class("hidden")


@pytest.mark.asyncio
async def test_enter_pair_review_auto_starts_ai_review_when_enabled(tmp_path: Path) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        review = await open_review_for_task(pilot, task_ids.pair_review)

        async def _ai_review_visible() -> bool:
            await pilot.pause()
            return not review.query_one("#ai-review-chat").has_class("hidden")

        await wait_until_async(
            _ai_review_visible,
            timeout=5.0,
            check_interval=0.05,
            description="AI review panel to become visible",
        )
        assert not review.query_one("#ai-review-chat").has_class("hidden")


@pytest.mark.asyncio
async def test_task_output_review_tab_shows_state_for_in_progress(tmp_path: Path) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress)

        review = await open_review_for_task(pilot, task_ids.auto_in_progress)
        tabs = review.query_one("#review-tabs", TabbedContent)
        tabs.active = "review-ai"

        rendered = await wait_static_contains(
            pilot,
            "#review-state-note",
            "Task is in IN_PROGRESS",
        )
        assert "Task is in IN_PROGRESS" in rendered


@pytest.mark.asyncio
async def test_task_output_preserves_last_review_result_outside_review_status(
    tmp_path: Path,
) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        project_id = kagan_app.ctx.active_project_id
        assert project_id is not None

        workspace_id = await kagan_app.ctx.workspace_service.provision_for_project(
            task_ids.auto_in_progress,
            project_id,
        )
        session_record = await kagan_app.ctx.task_service.create_session_record(
            workspace_id=workspace_id,
            session_type=SessionType.SCRIPT,
            external_id=f"task:{task_ids.auto_in_progress}",
        )
        execution = await kagan_app.ctx.execution_service.create_execution(
            session_id=session_record.id,
            run_reason=ExecutionRunReason.CODINGAGENT,
            metadata={
                "review_result": {
                    "status": "rejected",
                    "summary": "Add regression coverage before merge.",
                }
            },
        )
        await kagan_app.ctx.execution_service.update_execution(
            execution.id,
            status=ExecutionStatus.COMPLETED,
        )

        mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress)

        review = await open_review_for_task(pilot, task_ids.auto_in_progress)
        tabs = review.query_one("#review-tabs", TabbedContent)
        tabs.active = "review-ai"

        rendered = await wait_chat_contains(
            pilot,
            "#ai-review-chat",
            "Add regression coverage before merge.",
        )
        assert "Add regression coverage before merge." in rendered

        state_note = await wait_static_contains(
            pilot,
            "#review-state-note",
            "previous review cycle",
        )
        assert "Task is in IN_PROGRESS" in state_note


@pytest.mark.asyncio
async def test_task_output_reacts_to_external_status_change_without_event(tmp_path: Path) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress)

        review = await open_review_for_task(pilot, task_ids.auto_in_progress)
        tabs = review.query_one("#review-tabs", TabbedContent)
        tabs.active = "review-ai"

        repo = kagan_app.ctx._task_repo
        assert repo is not None
        await repo.update(task_ids.auto_in_progress, status=TaskStatus.REVIEW)

        rendered = await wait_static_contains(
            pilot,
            "#review-state-note",
            "Task is in REVIEW",
            timeout=6.0,
        )
        assert "Task is in REVIEW" in rendered


@pytest.mark.asyncio
async def test_task_output_agent_output_tab_start_stop_keybindings(
    tmp_path: Path,
) -> None:
    """Start/stop agent actions are available via 'a'/'s' keybindings in the review modal."""
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress)

        review = await open_review_for_task(pilot, task_ids.auto_in_progress)
        tabs = review.query_one("#review-tabs", TabbedContent)
        tabs.active = "review-agent-output"
        await pilot.pause()

        # When agent is running, the state note reflects it
        state_note = review.query_one("#agent-output-state-note", Static)
        note_text = str(state_note.render()).lower()
        assert "live" in note_text or "active" in note_text

        # Verify keybindings are registered (actions exist on the modal)
        assert hasattr(review, "action_start_agent_output")
        assert hasattr(review, "action_stop_agent_output")


@pytest.mark.asyncio
async def test_diff_file_picker_selection_scrolls_to_selected_file_start(tmp_path: Path) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)

        review = await open_review_for_task(pilot, task_ids.pair_review)
        tabs = review.query_one("#review-tabs", TabbedContent)
        tabs.active = "review-diff"
        await pilot.pause()

        review._file_diffs = {
            "alpha": FileDiff(
                path="alpha.py",
                additions=10,
                deletions=1,
                status="M",
                diff_content="diff --git a/alpha.py b/alpha.py\n"
                + "\n".join(f"+alpha-{i}" for i in range(120)),
            ),
            "beta": FileDiff(
                path="beta.py",
                additions=8,
                deletions=2,
                status="M",
                diff_content="diff --git a/beta.py b/beta.py\n"
                + "\n".join(f"+beta-{i}" for i in range(120)),
            ),
        }

        review._diff_show_file("alpha")
        await pilot.pause()

        diff_log = review.query_one("#diff-log", RichLog)
        diff_log.scroll_to(y=25, animate=False, immediate=True)
        assert diff_log.lines[0].text.startswith("diff --git a/alpha.py")
        # On Windows, RichLog scroll position may lag a render cycle.
        async def _diff_scrolled_down() -> bool:
            await pilot.pause()
            return diff_log.scroll_y > 0

        await wait_until_async(
            _diff_scrolled_down,
            timeout=2.0,
            check_interval=0.05,
            description="Diff log to scroll down",
        )

        review.on_diff_file_selected(
            cast("Any", SimpleNamespace(row_key=SimpleNamespace(value="beta")))
        )
        async def _diff_scrolled_to_top() -> bool:
            await pilot.pause()
            return diff_log.scroll_y == 0

        await wait_until_async(
            _diff_scrolled_to_top,
            timeout=2.0,
            check_interval=0.05,
            description="Diff log to scroll to top",
        )
        assert diff_log.lines[0].text.startswith("diff --git a/beta.py")

        diff_log.scroll_to(y=35, animate=False, immediate=True)
        review.on_diff_file_cell_selected(
            cast(
                "Any",
                SimpleNamespace(
                    cell_key=SimpleNamespace(row_key=SimpleNamespace(value="alpha")),
                ),
            )
        )
        await wait_until_async(
            _diff_scrolled_to_top,
            timeout=2.0,
            check_interval=0.05,
            description="Diff log to scroll to top after file cell selection",
        )
        assert diff_log.lines[0].text.startswith("diff --git a/alpha.py")

        diff_log.scroll_to(y=40, animate=False, immediate=True)
        review.on_diff_file_cell_highlighted(
            cast(
                "Any",
                SimpleNamespace(
                    cell_key=SimpleNamespace(row_key=SimpleNamespace(value="beta")),
                ),
            )
        )
        await wait_until_async(
            _diff_scrolled_to_top,
            timeout=2.0,
            check_interval=0.05,
            description="Diff log to scroll to top after file cell highlight",
        )
        assert diff_log.lines[0].text.startswith("diff --git a/beta.py")


@pytest.mark.asyncio
async def test_enter_auto_in_progress_backfill_with_empty_payload_shows_fallback_note(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        execution_service = kagan_app.ctx.execution_service

        async def _get_latest_execution_for_task(_task_id: str) -> Any:
            return make_execution_stub(
                "exec-empty",
                status=ExecutionStatus.COMPLETED,
                metadata={},
            )

        async def _get_execution_log_entries(_execution_id: str) -> list[Any]:
            return [make_execution_log_entry('{"messages":[]}')]

        async def _get_execution(_execution_id: str) -> Any:
            return make_execution_stub(
                "exec-empty",
                status=ExecutionStatus.COMPLETED,
                metadata={},
            )

        monkeypatch.setattr(
            execution_service,
            "get_latest_execution_for_task",
            _get_latest_execution_for_task,
        )
        monkeypatch.setattr(
            execution_service,
            "get_execution_log_entries",
            _get_execution_log_entries,
        )
        monkeypatch.setattr(
            execution_service,
            "get_execution",
            _get_execution,
        )

        await open_review_for_task(pilot, task_ids.auto_in_progress)
        rendered = await wait_chat_contains(
            pilot,
            "#review-agent-output-chat",
            "contains no displayable output yet",
        )
        assert "contains no displayable output yet" in rendered
        assert rendered.strip() != ""


@pytest.mark.asyncio
async def test_enter_auto_modal_refreshes_when_task_moves_to_review(
    tmp_path: Path,
) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress, agent=NoopMessageAgent())

        review = await open_review_for_task(pilot, task_ids.auto_in_progress)
        tabs = review.query_one("#review-tabs", TabbedContent)
        assert tabs.active == "review-agent-output"

        await kagan_app.ctx.task_service.update_fields(
            task_ids.auto_in_progress,
            status=TaskStatus.REVIEW,
        )

        await pilot.pause()
        await pilot.pause()
        assert tabs.active == "review-agent-output"
        assert not review.query_one(".button-row").has_class("hidden")
        review_note = await wait_static_contains(
            pilot,
            "#review-state-note",
            "Task is in REVIEW.",
        )
        assert "Task is in REVIEW." in review_note


@pytest.mark.asyncio
async def test_enter_auto_modal_refreshes_when_external_stream_attach_is_slow(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        automation = kagan_app.ctx.automation_service
        mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress)

        async def _wait_for_running_agent(_task_id: str, **_kwargs) -> Any:
            del _task_id, _kwargs
            await asyncio.sleep(2.0)
            return None

        monkeypatch.setattr(automation, "wait_for_running_agent", _wait_for_running_agent)

        review = await open_review_for_task(pilot, task_ids.auto_in_progress)
        tabs = review.query_one("#review-tabs", TabbedContent)
        assert tabs.active == "review-agent-output"

        await kagan_app.ctx.task_service.update_fields(
            task_ids.auto_in_progress,
            status=TaskStatus.REVIEW,
        )

        review_note = await wait_static_contains(
            pilot,
            "#review-state-note",
            "Task is in REVIEW.",
            timeout=4.0,
        )
        assert "Task is in REVIEW." in review_note
        assert not review.query_one(".button-row").has_class("hidden")


@pytest.mark.asyncio
async def test_enter_auto_in_progress_attaches_live_stream_immediately_from_waited_agent(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        automation = kagan_app.ctx.automation_service

        mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress)
        agent = NoopMessageAgent()

        async def _wait_for_running_agent(task_id: str, **_kwargs) -> Any:
            del _kwargs
            if task_id == task_ids.auto_in_progress:
                return agent
            return None

        monkeypatch.setattr(automation, "wait_for_running_agent", _wait_for_running_agent)

        await open_review_for_task(pilot, task_ids.auto_in_progress)
        rendered = await wait_chat_contains(
            pilot,
            "#review-agent-output-chat",
            "Connected to live agent stream",
        )
        assert "Connected to live agent stream" in rendered


@pytest.mark.asyncio
async def test_enter_auto_in_progress_attaches_stream_when_agent_appears_after_modal_open(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        automation = kagan_app.ctx.automation_service

        mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress)

        async def _wait_for_running_agent(_task_id: str, **_kwargs) -> Any:
            del _kwargs
            return None

        monkeypatch.setattr(automation, "wait_for_running_agent", _wait_for_running_agent)

        await open_review_for_task(pilot, task_ids.auto_in_progress)

        live_agent = NoopMessageAgent()
        kagan_app.ctx.runtime_service.attach_running_agent(
            task_ids.auto_in_progress,
            cast("Any", live_agent),
        )
        await kagan_app.ctx.event_bus.publish(
            AutomationAgentAttached(task_id=task_ids.auto_in_progress)
        )

        rendered = await wait_chat_contains(
            pilot,
            "#review-agent-output-chat",
            "Connected to live agent stream",
        )
        assert "Connected to live agent stream" in rendered


@pytest.mark.asyncio
async def test_enter_auto_in_progress_waiting_mode_shows_informative_output(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        automation = kagan_app.ctx.automation_service

        mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress)

        async def _wait_for_running_agent(_task_id: str, **_kwargs) -> Any:
            del _kwargs
            return None

        monkeypatch.setattr(automation, "wait_for_running_agent", _wait_for_running_agent)

        await open_review_for_task(pilot, task_ids.auto_in_progress)
        rendered = await wait_chat_contains(
            pilot,
            "#review-agent-output-chat",
            "Waiting for live agent stream...",
        )
        assert "Waiting for live agent stream..." in rendered
        assert rendered.strip() != ""


@pytest.mark.asyncio
async def test_enter_auto_in_progress_recovers_stale_running_execution_without_logs(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        automation = kagan_app.ctx.automation_service
        execution_service = kagan_app.ctx.execution_service

        state: dict[str, Any] = {"running": False, "agent": None}

        async def _wait_for_running_agent(task_id: str, **_kwargs) -> Any:
            del _kwargs
            if task_id != task_ids.auto_in_progress:
                return None
            return state["agent"]

        monkeypatch.setattr(automation, "wait_for_running_agent", _wait_for_running_agent)

        async def _spawn_for_task(task) -> bool:
            if task.id == task_ids.auto_in_progress:
                state["running"] = True
                kagan_app.ctx.runtime_service.mark_started(task_ids.auto_in_progress)
                kagan_app.ctx.runtime_service.set_execution(task_ids.auto_in_progress, None, 0)
                return True
            return False

        monkeypatch.setattr(automation, "spawn_for_task", _spawn_for_task)

        async def _get_latest_execution_for_task(task_id: str) -> Any:
            if task_id == task_ids.auto_in_progress:
                return make_execution_stub(
                    "stale0001",
                    status=ExecutionStatus.RUNNING,
                )
            return None

        async def _get_execution_log_entries(_execution_id: str) -> list[Any]:
            return []

        async def _update_execution(execution_id: str, **_kwargs) -> Any:
            del execution_id, _kwargs
            return None

        monkeypatch.setattr(
            execution_service,
            "get_latest_execution_for_task",
            _get_latest_execution_for_task,
        )
        monkeypatch.setattr(
            execution_service,
            "get_execution_log_entries",
            _get_execution_log_entries,
        )
        monkeypatch.setattr(execution_service, "update_execution", _update_execution)

        review = await open_review_for_task(pilot, task_ids.auto_in_progress)
        rendered = review.query_one(
            "#review-agent-output-chat", ChatPanel
        ).output.get_text_content()

        if (
            "Connected to live agent stream" not in rendered
            and "Waiting for live agent stream..." not in rendered
        ):
            rendered = await wait_chat_contains(
                pilot,
                "#review-agent-output-chat",
                "Waiting for live agent stream...",
            )

        assert rendered.strip() != ""
        assert (
            "Connected to live agent stream" in rendered
            or "Waiting for live agent stream..." in rendered
        )
        assert state["running"] is True


@pytest.mark.asyncio
async def test_enter_auto_review_attaches_live_review_stream_when_agent_appears_late(
    tmp_path: Path,
) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)

        await open_review_for_task(pilot, task_ids.auto_review)

        kagan_app.ctx.runtime_service.attach_review_agent(
            task_ids.auto_review,
            cast("Any", NoopMessageAgent()),
        )
        await kagan_app.ctx.event_bus.publish(
            AutomationReviewAgentAttached(task_id=task_ids.auto_review)
        )

        rendered = await wait_chat_contains(
            pilot,
            "#ai-review-chat",
            "Connected to live review stream",
        )
        assert "Connected to live review stream" in rendered


@pytest.mark.asyncio
async def test_enter_auto_backlog_backfills_history_when_live_agent_unavailable(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        automation = kagan_app.ctx.automation_service

        async def _spawn_for_task(_task) -> bool:
            kagan_app.ctx.runtime_service.mark_started(task_ids.auto_backlog)
            kagan_app.ctx.runtime_service.set_execution(task_ids.auto_backlog, "exec-backfill", 1)
            return True

        async def _wait_for_running_agent(task_id: str, **_kwargs) -> Any:
            del task_id, _kwargs
            return None

        monkeypatch.setattr(automation, "spawn_for_task", _spawn_for_task)
        monkeypatch.setattr(
            automation,
            "is_running",
            lambda task_id: task_id == task_ids.auto_backlog,
        )
        monkeypatch.setattr(automation, "get_running_agent", lambda _task_id: None)
        monkeypatch.setattr(automation, "wait_for_running_agent", _wait_for_running_agent)

        async def _get_execution_log_entries(_execution_id: str) -> list[Any]:
            return [
                make_execution_log_entry(
                    '{"messages":[{"type":"response","content":"history backfill line"}]}'
                )
            ]

        async def _get_execution(_execution_id: str) -> Any:
            return None

        async def _get_latest_execution_for_task(_task_id: str) -> Any:
            return make_execution_stub("exec-backfill", status=ExecutionStatus.COMPLETED)

        monkeypatch.setattr(
            kagan_app.ctx.execution_service,
            "get_execution_log_entries",
            _get_execution_log_entries,
        )
        monkeypatch.setattr(kagan_app.ctx.execution_service, "get_execution", _get_execution)
        monkeypatch.setattr(
            kagan_app.ctx.execution_service,
            "get_latest_execution_for_task",
            _get_latest_execution_for_task,
        )

        await open_review_for_task(pilot, task_ids.auto_backlog)

        rendered = await wait_chat_contains(
            pilot,
            "#review-agent-output-chat",
            "history backfill line",
        )
        assert "history backfill line" in rendered


@pytest.mark.asyncio
async def test_review_modal_sigterm_stream_end_is_not_rendered_as_error(tmp_path: Path) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)
    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        kagan_app = cast("KaganApp", pilot.app)
        mark_auto_runtime_running(kagan_app, task_ids.auto_in_progress, agent=NoopMessageAgent())

        review = await open_review_for_task(pilot, task_ids.auto_in_progress)
        review.post_message(messages.AgentFail("Agent exited with code -15"))
        await pilot.pause()
        await pilot.pause()

        review_text = review.query_one("#ai-review-chat", ChatPanel).output.get_text_content()
        agent_output_text = review.query_one(
            "#review-agent-output-chat", ChatPanel
        ).output.get_text_content()
        rendered = f"{review_text}\n{agent_output_text}"
        assert "Agent stream ended by cancellation (SIGTERM)." in rendered
        assert "Error: Agent exited with code -15" not in rendered
