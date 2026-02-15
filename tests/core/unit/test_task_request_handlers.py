"""Unit tests for task request handlers."""

from __future__ import annotations


async def test_task_create_accepts_string_acceptance_criteria(api_env) -> None:
    from kagan.core.request_handlers import handle_task_create

    _, api, _ = api_env
    result = await handle_task_create(
        api,
        {
            "title": "String acceptance criteria",
            "description": "desc",
            "acceptance_criteria": "Ship with tests",
        },
    )

    assert result["success"] is True
    task = await api.get_task(result["task_id"])
    assert task is not None
    assert task.acceptance_criteria == ["Ship with tests"]


async def test_task_list_include_scratchpad_returns_content(api_env) -> None:
    from kagan.core.request_handlers import handle_task_list

    _, api, _ = api_env
    task = await api.create_task("Scratchpad visibility", "desc")
    await api.update_scratchpad(task.id, "First note")

    result = await handle_task_list(api, {"include_scratchpad": True})
    returned = next(item for item in result["tasks"] if item["id"] == task.id)

    assert result["count"] >= 1
    assert returned["scratchpad"] == "First note"
