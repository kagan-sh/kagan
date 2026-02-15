from __future__ import annotations

import asyncio
import json
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, cast
from unittest.mock import AsyncMock

from tests.helpers.mocks import create_test_config

from kagan.core.acp import messages
from kagan.core.services.automation.runner import AutomationEngine, AutomationReviewer

if TYPE_CHECKING:
    from kagan.core.agents.agent_factory import AgentFactory
    from kagan.core.config import AgentConfig
    from kagan.core.services.runtime import RuntimeService
    from kagan.core.services.tasks import TaskService
    from kagan.core.services.types import TaskLike
    from kagan.core.services.workspaces import WorkspaceService


class _FakeAgent:
    def __init__(self) -> None:
        self.task_id: str | None = None
        self.auto_approve: bool | None = None
        self.tool_calls: dict[str, object] = {}
        self._response_text = "<continue/>"

    def set_task_id(self, task_id: str | None) -> None:
        self.task_id = task_id

    def set_auto_approve(self, enabled: bool) -> None:
        self.auto_approve = enabled

    def set_model_override(self, model_id: str | None) -> None:
        del model_id

    def start(self) -> None:
        return

    async def wait_ready(self, timeout: float = 30.0) -> None:
        del timeout

    async def send_prompt(self, prompt: str) -> None:
        del prompt

    def clear_tool_calls(self) -> None:
        self.tool_calls.clear()

    def get_response_text(self) -> str:
        return self._response_text

    def get_messages(self) -> list[object]:
        return []

    async def stop(self) -> None:
        return


async def test_auto_execution_sets_task_scope_on_agent(monkeypatch, tmp_path) -> None:
    fake_agent = _FakeAgent()

    def _factory(project_root, agent_config, *, read_only: bool = False):
        del project_root
        del agent_config
        del read_only
        return fake_agent

    task_service = SimpleNamespace(
        get_scratchpad=AsyncMock(return_value=""),
        update_scratchpad=AsyncMock(return_value=None),
    )
    runtime_service = SimpleNamespace(
        get=lambda _task_id: None,
        mark_ended=lambda _task_id: None,
        attach_running_agent=lambda _task_id, _agent: None,
        attach_review_agent=lambda _task_id, _agent: None,
        clear_review_agent=lambda _task_id: None,
    )

    engine = AutomationEngine(
        task_service=cast("TaskService", task_service),
        workspace_service=cast("WorkspaceService", SimpleNamespace()),
        config=create_test_config(),
        runtime_service=cast("RuntimeService", runtime_service),
        agent_factory=cast("AgentFactory", _factory),
    )

    monkeypatch.setattr(
        "kagan.core.services.automation.runner.build_prompt",
        lambda **_kwargs: "prompt",
    )

    task = SimpleNamespace(id="AUTO-123")
    agent_config = SimpleNamespace(identity="test.agent", name="Test")

    _signal, _agent = await engine._run_execution(
        task=cast("TaskLike", task),
        wt_path=tmp_path,
        agent_config=cast("AgentConfig", agent_config),
        run_count=1,
        execution_id="exec-1",
    )

    assert fake_agent.task_id == "AUTO-123"
    assert fake_agent.auto_approve is True


async def test_auto_review_sets_task_scope_on_agent(monkeypatch, tmp_path) -> None:
    fake_agent = _FakeAgent()
    fake_agent._response_text = "<approve/>"

    def _factory(project_root, agent_config, *, read_only: bool = False):
        del project_root
        del agent_config
        del read_only
        return fake_agent

    reviewer = AutomationReviewer(
        task_service=cast("TaskService", SimpleNamespace()),
        workspace_service=cast("WorkspaceService", SimpleNamespace()),
        config=create_test_config(),
        execution_service=None,
        notifier=None,
        agent_factory=cast("AgentFactory", _factory),
        git_adapter=None,
        runtime_service=cast(
            "RuntimeService",
            SimpleNamespace(clear_review_agent=lambda _task_id: None),
        ),
        get_agent_config=lambda _task: cast(
            "AgentConfig", SimpleNamespace(identity="test.agent", name="Test")
        ),
        apply_model_override=lambda _agent, _config, _context: None,
        set_review_agent=AsyncMock(return_value=None),
        notify_task_changed=lambda: None,
    )

    monkeypatch.setattr(reviewer, "_build_review_prompt", AsyncMock(return_value="prompt"))

    task = SimpleNamespace(
        id="AUTO-456",
        title="Review task",
        description="",
        base_branch="main",
    )
    passed, _summary = await reviewer.run_review(
        task=cast("TaskLike", task),
        wt_path=tmp_path,
        execution_id="exec-2",
    )

    assert passed is True
    assert fake_agent.task_id == "AUTO-456"
    assert fake_agent.auto_approve is True


async def test_auto_review_does_not_drop_review_log_start_index_when_setting_review_result(
    monkeypatch, tmp_path
) -> None:
    """Regression: avoid read-after-write metadata loss when persisting review_result."""
    config = create_test_config()
    config.general.auto_review = True

    task_service = SimpleNamespace(
        update_fields=AsyncMock(return_value=None),
        get_scratchpad=AsyncMock(return_value=""),
        update_scratchpad=AsyncMock(return_value=None),
    )
    workspace_service = SimpleNamespace(get_path=AsyncMock(return_value=tmp_path))
    runtime_service = SimpleNamespace(get=lambda _task_id: SimpleNamespace(execution_id="exec-1"))

    execution_service = SimpleNamespace(
        get_execution_log_entries=AsyncMock(
            return_value=[
                SimpleNamespace(logs="a"),
                SimpleNamespace(logs="b"),
            ]
        ),
        get_execution=AsyncMock(return_value=SimpleNamespace(metadata_={})),
        update_execution=AsyncMock(return_value=None),
    )

    reviewer = AutomationReviewer(
        task_service=cast("TaskService", task_service),
        workspace_service=cast("WorkspaceService", workspace_service),
        config=config,
        execution_service=cast("Any", execution_service),
        notifier=None,
        agent_factory=cast("AgentFactory", lambda *_a, **_kw: None),
        git_adapter=None,
        runtime_service=cast("RuntimeService", runtime_service),
        get_agent_config=lambda _task: cast("AgentConfig", SimpleNamespace(identity="x", name="x")),
        apply_model_override=lambda *_a, **_kw: None,
        set_review_agent=AsyncMock(return_value=None),
        notify_task_changed=lambda: None,
    )
    monkeypatch.setattr(reviewer, "run_review", AsyncMock(return_value=(True, "LGTM")))

    task = SimpleNamespace(
        id="AUTO-999",
        title="Task",
        description="",
        base_branch="main",
    )
    await reviewer._handle_complete(cast("TaskLike", task))

    # Second update should retain the review_log_start_index set earlier.
    final_metadata = execution_service.update_execution.await_args_list[-1].kwargs["metadata"]
    assert final_metadata["review_log_start_index"] == 2
    assert "review_result" in final_metadata


async def test_auto_execution_persists_incremental_output_during_run(monkeypatch, tmp_path) -> None:
    class _StreamingAgent(_FakeAgent):
        def __init__(self) -> None:
            super().__init__()
            self._messages: list[object] = []
            self._response_text = ""

        async def send_prompt(self, prompt: str) -> None:
            del prompt
            self._messages.append(messages.AgentUpdate("text", "Hello"))
            self._response_text += "Hello"
            await asyncio.sleep(0.05)
            self._messages.append(messages.AgentUpdate("text", " world"))
            self._response_text += " world"

        def get_messages(self) -> list[object]:
            return list(self._messages)

    fake_agent = _StreamingAgent()

    def _factory(project_root, agent_config, *, read_only: bool = False):
        del project_root, agent_config, read_only
        return fake_agent

    execution_service = SimpleNamespace(
        append_execution_log=AsyncMock(return_value=None),
        append_agent_turn=AsyncMock(return_value=None),
    )
    task_service = SimpleNamespace(
        get_scratchpad=AsyncMock(return_value=""),
        update_scratchpad=AsyncMock(return_value=None),
    )
    runtime_service = SimpleNamespace(
        get=lambda _task_id: None,
        mark_ended=lambda _task_id: None,
        attach_running_agent=lambda _task_id, _agent: None,
        attach_review_agent=lambda _task_id, _agent: None,
        clear_review_agent=lambda _task_id: None,
    )

    engine = AutomationEngine(
        task_service=cast("TaskService", task_service),
        workspace_service=cast("WorkspaceService", SimpleNamespace()),
        config=create_test_config(),
        runtime_service=cast("RuntimeService", runtime_service),
        execution_service=cast("Any", execution_service),
        agent_factory=cast("AgentFactory", _factory),
    )

    monkeypatch.setattr(
        "kagan.core.services.automation.runner.build_prompt",
        lambda **_kwargs: "prompt",
    )

    task = SimpleNamespace(id="AUTO-789")
    agent_config = SimpleNamespace(identity="test.agent", name="Test")

    _signal, _agent = await engine._run_execution(
        task=cast("TaskLike", task),
        wt_path=tmp_path,
        agent_config=cast("AgentConfig", agent_config),
        run_count=1,
        execution_id="exec-3",
    )

    assert execution_service.append_execution_log.await_count >= 1
    persisted_payloads = [
        json.loads(call.args[1]) for call in execution_service.append_execution_log.await_args_list
    ]
    response_chunks = [
        message.get("content", "")
        for payload in persisted_payloads
        for message in payload.get("messages", [])
        if message.get("type") == "response"
    ]
    assert "Hello" in response_chunks
    assert " world" in response_chunks
    assert "Hello world" not in response_chunks
