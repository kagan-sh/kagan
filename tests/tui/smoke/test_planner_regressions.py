from __future__ import annotations

from types import SimpleNamespace
from typing import Any, cast
from unittest.mock import AsyncMock

import pytest
from tests.helpers.wait import (
    type_text,
    wait_for_planner_ready,
    wait_for_screen,
    wait_until,
)
from textual.css.query import NoMatches
from textual.widgets import Label

from kagan.tui.ui.screens.kanban import KanbanScreen
from kagan.tui.ui.screens.planner import PlannerInput, PlannerScreen
from kagan.tui.ui.screens.planner.state import PlannerEvent, PlannerPhase
from kagan.tui.ui.widgets.offline_banner import OfflineBanner
from kagan.tui.ui.widgets.plan_approval import PlanApprovalWidget
from kagan.tui.ui.widgets.streaming_output import StreamingOutput


@pytest.mark.asyncio
async def test_planner_restore_state_recovers_draft_and_pending_proposal_after_reopen(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        project_id = app.ctx.active_project_id
        assert project_id is not None

        kanban.action_open_planner()
        planner = cast("PlannerScreen", await wait_for_screen(pilot, PlannerScreen, timeout=10.0))
        await wait_for_planner_ready(pilot, timeout=10.0)

        pending_task = (await app.ctx.api.list_tasks(project_id=project_id))[0]
        planner._state.pending_plan = [pending_task]
        planner._state.has_pending_plan = True
        planner._show_output()
        await planner._get_output().post_plan_approval([pending_task])
        await wait_until(
            lambda: _has_plan_approval_widget(planner),
            timeout=8.0,
            description="planner renders pending proposal before leaving screen",
        )

        expected_draft = "draft text that must survive screen reopen"
        persistent = planner._persistent_state(expected_draft)
        assert planner._state.has_pending_plan is True

        await planner.action_to_board()
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        app.planner_state = persistent

        kanban = cast("KanbanScreen", pilot.app.screen)
        kanban.action_open_planner()
        planner = cast("PlannerScreen", await wait_for_screen(pilot, PlannerScreen, timeout=10.0))
        await wait_for_planner_ready(pilot, timeout=10.0)
        await wait_until(
            lambda: _has_plan_approval_widget(planner),
            timeout=8.0,
            description="planner restores pending proposal after reopen",
        )

        planner.query_one(PlanApprovalWidget)
        restored_input = planner.query_one("#planner-input", PlannerInput)
        assert restored_input.text == expected_draft
        assert planner._state.has_pending_plan is True
        assert planner._state.pending_plan


@pytest.mark.asyncio
async def test_planner_refine_action_updates_plan_and_preserves_user_context(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        kanban.action_open_planner()
        planner = cast("PlannerScreen", await wait_for_screen(pilot, PlannerScreen, timeout=10.0))
        await wait_for_planner_ready(pilot, timeout=10.0)

        planner_input = planner.query_one("#planner-input", PlannerInput)
        planner_input.focus()
        await type_text(pilot, "Create a minimal task")
        await pilot.press("enter")
        await wait_until(
            lambda: planner._state.phase == PlannerPhase.IDLE
            and len(planner._state.conversation_history) >= 2,
            timeout=8.0,
            description="planner finishes first prompt before refine action",
        )

        history_before = [
            (message.role, message.content) for message in planner._state.conversation_history
        ]

        refined_prompt = (
            "Build a staged migration plan for session orchestration reliability "
            "with rollback criteria."
        )
        planner_input.clear()
        planner_input.insert(refined_prompt)
        await pilot.pause()

        class _FakePromptRefiner:
            def __init__(self, *_args: object, **_kwargs: object) -> None:
                return None

            async def refine(self, text: str) -> str:
                return f"{text}\n\nRefined: include measurable rollback checkpoints."

            async def stop(self) -> None:
                return None

        monkeypatch.setattr("kagan.core.agents.refiner.PromptRefiner", _FakePromptRefiner)
        notify_calls: list[tuple[str, str]] = []
        original_notify = planner.notify

        def _capture_notify(
            message: str, *, severity: str = "information", **kwargs: object
        ) -> None:
            notify_calls.append((message, severity))
            original_notify(message, severity=severity, **kwargs)

        monkeypatch.setattr(planner, "notify", _capture_notify)

        await planner.action_refine()

        assert "Refined: include measurable rollback checkpoints." in planner_input.text
        history_after = [
            (message.role, message.content) for message in planner._state.conversation_history
        ]
        assert history_after == history_before
        assert planner._state.phase == PlannerPhase.IDLE
        assert any(
            message == "Prompt enhanced - review and press Enter" and severity == "information"
            for message, severity in notify_calls
        )


def _has_plan_approval_widget(planner: PlannerScreen) -> bool:
    try:
        planner.query_one(PlanApprovalWidget)
    except NoMatches:
        return False
    return True


@pytest.mark.asyncio
async def test_planner_without_active_repo_shows_offline_banner_and_disables_input(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        app.ctx.active_repo_id = None
        kanban.action_open_planner()
        planner = cast("PlannerScreen", await wait_for_screen(pilot, PlannerScreen, timeout=10.0))

        banner = planner.query_one(OfflineBanner)
        message = banner.query_one("#offline-message", Label)
        assert "Select a repository to start planning" in str(message.content)
        assert planner.query_one("#planner-input", PlannerInput).has_class("-disabled") is True
        assert planner.planner_status == "offline"


@pytest.mark.asyncio
async def test_planner_agent_unavailable_shows_status_banner_and_blocks_input(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        monkeypatch.setattr(app.ctx.api, "is_agent_available", lambda: False)
        monkeypatch.setattr(
            app.ctx.api,
            "get_agent_status_message",
            lambda: "Agent unavailable in test",
        )
        kanban.action_open_planner()
        planner = cast("PlannerScreen", await wait_for_screen(pilot, PlannerScreen, timeout=10.0))

        banner = planner.query_one(OfflineBanner)
        message = banner.query_one("#offline-message", Label)
        assert "Agent unavailable in test" in str(message.content)
        assert planner.query_one("#planner-input", PlannerInput).has_class("-disabled") is True
        assert planner.planner_status == "offline"


@pytest.mark.asyncio
async def test_planner_cancel_when_idle_clears_input_only(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        kanban.action_open_planner()
        planner = cast("PlannerScreen", await wait_for_screen(pilot, PlannerScreen, timeout=10.0))
        await wait_for_planner_ready(pilot, timeout=10.0)

        planner_input = planner.query_one("#planner-input", PlannerInput)
        planner_input.insert("draft text that should be cleared")
        await pilot.pause()
        await planner.action_cancel()

        assert planner_input.text == ""
        assert planner._state.phase == PlannerPhase.IDLE


@pytest.mark.asyncio
async def test_planner_cancel_during_processing_interrupts_agent_and_preserves_partial_response(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        kanban.action_open_planner()
        planner = cast("PlannerScreen", await wait_for_screen(pilot, PlannerScreen, timeout=10.0))
        await wait_for_planner_ready(pilot, timeout=10.0)

        fake_agent = SimpleNamespace(
            cancel=AsyncMock(),
            set_message_target=lambda _target: None,
            stop=AsyncMock(),
        )
        planner._state.agent = fake_agent
        planner._state = planner._state.with_agent_ready(True).transition(PlannerEvent.SUBMIT)
        planner._state.accumulated_response.append("Partial response from planner")
        planner.query_one("#planner-input", PlannerInput).add_class("-disabled")

        await planner.action_cancel()

        assert fake_agent.cancel.await_count == 1
        assert planner._state.phase == PlannerPhase.IDLE
        assert planner._state.conversation_history
        assert "*[interrupted]*" in planner._state.conversation_history[-1].content
        assert "Interrupted by user" in planner.query_one(StreamingOutput).get_text_content()


@pytest.mark.asyncio
async def test_planner_load_pending_proposal_restores_latest_valid_draft(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        kanban.action_open_planner()
        planner = cast("PlannerScreen", await wait_for_screen(pilot, PlannerScreen, timeout=10.0))
        await wait_for_planner_ready(pilot, timeout=10.0)

        proposal = SimpleNamespace(
            id="proposal-1",
            project_id=app.ctx.active_project_id or "project-1",
            tasks_json=[
                {
                    "title": "Persisted draft task",
                    "description": "Loaded from saved planner draft",
                    "priority": "HIGH",
                    "task_type": "AUTO",
                    "acceptance_criteria": ["Draft is restored on reopen"],
                },
                "invalid-entry",
            ],
        )
        monkeypatch.setattr(
            app.ctx.api,
            "list_pending_planner_drafts",
            AsyncMock(return_value=[proposal]),
        )

        planner._state.pending_plan = None
        planner._state.has_pending_plan = False
        await planner._load_pending_proposals()

        assert planner._pending_proposal_id == "proposal-1"
        assert planner._state.has_pending_plan is True
        assert planner._state.pending_plan is not None
        assert planner._state.pending_plan[0].title == "Persisted draft task"
        assert _has_plan_approval_widget(planner)


@pytest.mark.asyncio
async def test_planner_dismissed_plan_with_active_agent_prompts_for_clarification(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        kanban.action_open_planner()
        planner = cast("PlannerScreen", await wait_for_screen(pilot, PlannerScreen, timeout=10.0))
        await wait_for_planner_ready(pilot, timeout=10.0)

        project_id = app.ctx.active_project_id
        assert project_id is not None
        pending_task = (await app.ctx.api.list_tasks(project_id=project_id))[0]
        planner._state.pending_plan = [pending_task]
        planner._state.has_pending_plan = True
        await planner._get_output().post_plan_approval([pending_task])
        await wait_until(
            lambda: _has_plan_approval_widget(planner),
            timeout=8.0,
            description="planner renders pending proposal before dismissal",
        )

        run_worker_calls: list[dict[str, Any]] = []

        def _capture_run_worker(*args: Any, **kwargs: Any) -> None:
            if args:
                maybe_coro = args[0]
                close = getattr(maybe_coro, "close", None)
                if callable(close):
                    close()
            run_worker_calls.append(kwargs)
            return None

        monkeypatch.setattr(planner, "run_worker", _capture_run_worker)
        planner.query_one(PlanApprovalWidget).action_dismiss()
        await wait_until(
            lambda: bool(run_worker_calls),
            timeout=8.0,
            description="planner schedules clarification follow-up after dismissal",
        )

        assert planner._state.has_pending_plan is False
        assert planner._state.phase == PlannerPhase.PROCESSING
        assert planner.planner_status == "thinking"
        assert "Plan dismissed" in planner.query_one(StreamingOutput).get_text_content()
        assert run_worker_calls[0].get("group") == "planner-send-to-agent"
        assert run_worker_calls[0].get("exclusive") is True
