from __future__ import annotations

from typing import TYPE_CHECKING
from unittest.mock import AsyncMock

import pytest
from tests.tui.smoke._enter_flow_support import (
    open_kanban,
    open_review_for_task,
    seed_enter_flow_app,
)
from textual.widgets import Static

from kagan.core.models.enums import StreamPhase, TaskStatus
from kagan.tui.ui.widgets.chat_panel import ChatPanel

if TYPE_CHECKING:
    from pathlib import Path

pytestmark = pytest.mark.usefixtures("global_mock_tmux")


@pytest.mark.asyncio
async def test_review_modal_start_cancel_output_does_not_mutate_task_state_on_cancel(
    tmp_path: Path,
) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)

    class _StoppableAgent:
        def __init__(self) -> None:
            self.stop_calls = 0

        async def stop(self) -> None:
            self.stop_calls += 1

    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        review = await open_review_for_task(pilot, task_ids.auto_review)

        agent = _StoppableAgent()
        review._phase = StreamPhase.STREAMING
        review._agent = agent

        await review.action_cancel_review()

        task = await app.ctx.api.get_task(task_ids.auto_review)
        assert task is not None
        assert task.status == TaskStatus.REVIEW
        assert agent.stop_calls == 1
        assert review._phase == StreamPhase.IDLE
        rendered = review.query_one("#ai-review-chat", ChatPanel).output.get_text_content()
        assert "Review cancelled" in rendered


@pytest.mark.asyncio
async def test_review_modal_diff_parse_totals_handles_empty_and_malformed_diff_gracefully(
    tmp_path: Path,
) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)

    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        review = await open_review_for_task(pilot, task_ids.pair_review)

        assert review._diff_parse_totals("") == (0, 0, 0)
        assert review._diff_parse_totals("not-a-diff-stats-line") == (0, 0, 0)
        assert review._diff_parse_totals("total: +x -y (z file changed)") == (0, 0, 0)

        additions, deletions, files = review._diff_parse_totals("still malformed")
        review._diff_set_stats(additions, deletions, files)
        assert str(review.query_one("#stat-additions", Static).render()) == "+ 0 Additions"
        assert str(review.query_one("#stat-deletions", Static).render()) == "- 0 Deletions"
        assert str(review.query_one("#stat-files", Static).render()) == "0 Files Changed"


@pytest.mark.asyncio
async def test_review_modal_diff_open_modal_handles_missing_workspace_diff(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    app, task_ids = await seed_enter_flow_app(tmp_path)

    async with app.run_test(size=(120, 40)) as pilot:
        await open_kanban(pilot)
        review = await open_review_for_task(pilot, task_ids.pair_review)

        get_workspace_diff = AsyncMock(return_value="diff --git a/file.py b/file.py\n+line")
        push_screen = AsyncMock(return_value=None)
        monkeypatch.setattr(review.ctx.api, "list_workspaces", AsyncMock(return_value=[]))
        monkeypatch.setattr(review.ctx.api, "get_workspace_diff", get_workspace_diff)
        monkeypatch.setattr(review.app, "push_screen", push_screen)
        review._diff_text = ""

        await review._diff_open_modal()

        get_workspace_diff.assert_awaited_once_with(
            task_ids.pair_review, base_branch=review._base_branch
        )
        push_screen.assert_awaited_once()
        modal = push_screen.await_args.args[0]
        assert modal.__class__.__name__ == "DiffModal"
        assert getattr(modal, "_diff_text", "") == "diff --git a/file.py b/file.py\n+line"
