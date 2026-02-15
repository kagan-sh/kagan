from __future__ import annotations

from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest

from kagan.tui.core_client_api import CoreBackedApi


@pytest.mark.asyncio
async def test_reconcile_running_tasks_refreshes_runtime_cache(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    api._runtime_by_task = {
        "task-1": {"is_running": False},
        "task-2": {"is_running": True},
    }

    call_core = AsyncMock(
        return_value=[
            {"task_id": "task-1", "runtime": {"is_running": True}},
            {"task_id": "task-2", "runtime": {"is_running": False}},
        ]
    )
    monkeypatch.setattr(api, "_call_core", call_core)

    await api.reconcile_running_tasks(["task-1", "task-2"])

    assert api.get_runtime_view("task-1") == {"is_running": True}
    assert api.get_runtime_view("task-2") == {"is_running": False}
    assert api.get_running_task_ids() == {"task-1"}


@pytest.mark.asyncio
async def test_reconcile_running_tasks_accepts_runtime_payloads_with_id_key(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    call_core = AsyncMock(
        return_value=[
            {"id": "task-3", "runtime": {"is_running": True}},
        ]
    )
    monkeypatch.setattr(api, "_call_core", call_core)

    await api.reconcile_running_tasks(["task-3"])

    assert api.get_runtime_view("task-3") == {"is_running": True}


@pytest.mark.asyncio
async def test_core_backed_api_invoke_plugin_forwards_payload(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    call_core = AsyncMock(return_value={"success": True, "code": "CONNECTED"})
    monkeypatch.setattr(api, "_call_core", call_core)

    result = await api.invoke_plugin(
        "kagan_github", "connect_repo", {"project_id": "project-1", "repo_id": "repo-1"}
    )

    assert result["success"] is True
    call_core.assert_awaited_once_with(
        "invoke_plugin",
        kwargs={
            "capability": "kagan_github",
            "method": "connect_repo",
            "params": {"project_id": "project-1", "repo_id": "repo-1"},
        },
    )


@pytest.mark.asyncio
async def test_core_backed_api_invoke_plugin_rejects_non_dict_response(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    call_core = AsyncMock(return_value="not-a-dict")
    monkeypatch.setattr(api, "_call_core", call_core)

    with pytest.raises(RuntimeError, match="Core returned invalid plugin payload"):
        await api.invoke_plugin("kagan_github", "sync_issues", {"project_id": "project-1"})
