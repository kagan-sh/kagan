"""Unit tests for Kanban review controller edge-case handling."""

from __future__ import annotations

from types import SimpleNamespace


async def test_resolve_base_branch_handles_runtime_error_from_core_api() -> None:
    """Kanban review flow should surface core transport errors as UI notifications."""
    from kagan.tui.ui.screens.kanban.review_controller import KanbanReviewController

    notifications: list[tuple[str, str | None]] = []

    class _FailingApi:
        async def resolve_task_base_branch(self, task: object) -> str:
            del task
            raise RuntimeError("no default branch")

    screen = SimpleNamespace(
        ctx=SimpleNamespace(api=_FailingApi()),
        notify=lambda message, severity=None: notifications.append((message, severity)),
    )
    controller = KanbanReviewController(screen)

    resolved = await controller._resolve_base_branch(SimpleNamespace(id="task-1"))

    assert resolved is None
    assert notifications == [("no default branch", "error")]
