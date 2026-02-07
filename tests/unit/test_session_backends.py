from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, Literal, cast
from unittest.mock import AsyncMock

from kagan.config import AgentConfig, KaganConfig
from kagan.core.models.enums import PairTerminalBackend, SessionType, TaskStatus, TaskType
from kagan.services.sessions import SessionService

if TYPE_CHECKING:
    from pytest import MonkeyPatch


@dataclass
class _TaskStub:
    id: str = "task-1"
    title: str = "Task Title"
    description: str = "Task description"
    status: TaskStatus = TaskStatus.BACKLOG
    task_type: TaskType = TaskType.PAIR
    terminal_backend: PairTerminalBackend | None = None
    assigned_hat: str | None = None
    agent_backend: str | None = None
    base_branch: str | None = None
    acceptance_criteria: list[str] = field(default_factory=list)

    def get_agent_config(self, config: KaganConfig) -> Any:
        del config
        return AgentConfig(
            identity="copilot.github.com",
            name="Copilot",
            short_name="copilot",
            run_command={"*": "copilot"},
            interactive_command={"*": "copilot"},
            active=True,
            model_env_var="",
        )


class _Proc:
    async def wait(self) -> int:
        return 0


def _build_service(
    default_backend: Literal["tmux", "vscode", "cursor"] | None = None,
) -> tuple[SessionService, SimpleNamespace, SimpleNamespace]:
    config = KaganConfig()
    if default_backend is not None:
        config.general.default_pair_terminal_backend = default_backend

    task_service: Any = SimpleNamespace(
        create_session_record=AsyncMock(),
        get_task=AsyncMock(),
        close_session_by_external_id=AsyncMock(),
    )
    workspace_service: Any = SimpleNamespace(
        list_workspaces=AsyncMock(return_value=[SimpleNamespace(id="ws-1")]),
        get_path=AsyncMock(return_value=None),
    )
    service = SessionService(
        project_root=Path("."),
        task_service=cast("Any", task_service),
        workspace_service=cast("Any", workspace_service),
        config=config,
    )
    service._write_context_files = AsyncMock()  # type: ignore[method-assign]
    return service, task_service, workspace_service


async def test_create_session_prefers_task_override_over_config(monkeypatch: MonkeyPatch) -> None:
    service, task_service, _workspace_service = _build_service(default_backend="tmux")
    task = _TaskStub(terminal_backend=PairTerminalBackend.CURSOR)

    run_tmux_mock = AsyncMock(return_value="")
    exec_mock = AsyncMock(return_value=_Proc())
    monkeypatch.setattr("kagan.services.sessions.run_tmux", run_tmux_mock)
    monkeypatch.setattr("kagan.services.sessions.asyncio.create_subprocess_exec", exec_mock)

    await service.create_session(task, Path("."))

    assert exec_mock.await_count == 1
    assert run_tmux_mock.await_count == 0
    call = task_service.create_session_record.await_args.kwargs
    assert call["session_type"] == SessionType.SCRIPT


async def test_create_session_uses_config_default_for_task_without_backend(
    monkeypatch: MonkeyPatch,
) -> None:
    service, task_service, _workspace_service = _build_service(default_backend="tmux")
    task = _TaskStub(terminal_backend=None)

    run_tmux_mock = AsyncMock(return_value="")
    exec_mock = AsyncMock(return_value=_Proc())
    monkeypatch.setattr("kagan.services.sessions.run_tmux", run_tmux_mock)
    monkeypatch.setattr("kagan.services.sessions.asyncio.create_subprocess_exec", exec_mock)

    await service.create_session(task, Path("."))

    assert run_tmux_mock.await_count >= 1
    assert exec_mock.await_count == 0
    call = task_service.create_session_record.await_args.kwargs
    assert call["session_type"] == SessionType.TMUX


async def test_create_session_uses_config_default_when_task_has_no_backend_field(
    monkeypatch: MonkeyPatch,
) -> None:
    service, task_service, _workspace_service = _build_service(default_backend="tmux")
    task = _TaskStub(id="legacy-1", title="Legacy Task", description="Legacy task")
    delattr(task, "terminal_backend")

    run_tmux_mock = AsyncMock(return_value="")
    exec_mock = AsyncMock(return_value=_Proc())
    monkeypatch.setattr("kagan.services.sessions.run_tmux", run_tmux_mock)
    monkeypatch.setattr("kagan.services.sessions.asyncio.create_subprocess_exec", exec_mock)

    await service.create_session(task, Path("."))

    assert run_tmux_mock.await_count >= 1
    assert exec_mock.await_count == 0
    call = task_service.create_session_record.await_args.kwargs
    assert call["session_type"] == SessionType.TMUX


async def test_create_session_falls_back_to_tmux_when_default_is_invalid(
    monkeypatch: MonkeyPatch,
) -> None:
    service, task_service, _workspace_service = _build_service(default_backend=None)
    service._config.general.default_pair_terminal_backend = cast("Any", "invalid")
    task = _TaskStub(terminal_backend=None)

    run_tmux_mock = AsyncMock(return_value="")
    exec_mock = AsyncMock(return_value=_Proc())
    monkeypatch.setattr("kagan.services.sessions.run_tmux", run_tmux_mock)
    monkeypatch.setattr("kagan.services.sessions.asyncio.create_subprocess_exec", exec_mock)

    await service.create_session(task, Path("."))

    assert exec_mock.await_count == 0
    assert run_tmux_mock.await_count >= 1
    call = task_service.create_session_record.await_args.kwargs
    assert call["session_type"] == SessionType.TMUX


async def test_create_session_vscode_launches_external_command_and_writes_bundle(
    monkeypatch: MonkeyPatch,
    tmp_path: Path,
) -> None:
    service, task_service, _workspace_service = _build_service(default_backend=None)
    service._write_context_files = SessionService._write_context_files.__get__(
        service, SessionService
    )
    task = _TaskStub(terminal_backend=cast("Any", "vscode"))

    run_tmux_mock = AsyncMock(return_value="")
    exec_mock = AsyncMock(return_value=_Proc())
    monkeypatch.setattr("kagan.services.sessions.run_tmux", run_tmux_mock)
    monkeypatch.setattr("kagan.services.sessions.asyncio.create_subprocess_exec", exec_mock)

    await service.create_session(task, tmp_path)

    assert run_tmux_mock.await_count == 0

    await_args = exec_mock.await_args
    assert await_args is not None
    assert await_args.args == (
        "code",
        "--new-window",
        str(tmp_path),
        str(tmp_path / ".kagan" / "start_prompt.md"),
    )
    assert await_args.kwargs["cwd"] == str(tmp_path)

    session_json = tmp_path / ".kagan" / "session.json"
    prompt_file = tmp_path / ".kagan" / "start_prompt.md"
    vscode_mcp_file = tmp_path / ".vscode" / "mcp.json"
    assert session_json.exists()
    assert prompt_file.exists()
    assert vscode_mcp_file.exists()
    assert task.id in prompt_file.read_text(encoding="utf-8")
    assert '"servers"' in vscode_mcp_file.read_text(encoding="utf-8")

    call = task_service.create_session_record.await_args.kwargs
    assert call["session_type"] == SessionType.SCRIPT


async def test_create_session_cursor_writes_cursor_mcp_config(
    monkeypatch: MonkeyPatch,
    tmp_path: Path,
) -> None:
    service, _task_service, _workspace_service = _build_service(default_backend=None)
    service._write_context_files = SessionService._write_context_files.__get__(
        service, SessionService
    )
    task = _TaskStub(terminal_backend=cast("Any", "cursor"))

    exec_mock = AsyncMock(return_value=_Proc())
    monkeypatch.setattr("kagan.services.sessions.asyncio.create_subprocess_exec", exec_mock)

    await service.create_session(task, tmp_path)

    await_args = exec_mock.await_args
    assert await_args is not None
    assert await_args.args == (
        "cursor",
        "--new-window",
        str(tmp_path),
        str(tmp_path / ".kagan" / "start_prompt.md"),
    )
    cursor_mcp_file = tmp_path / ".cursor" / "mcp.json"
    assert cursor_mcp_file.exists()
    assert '"mcpServers"' in cursor_mcp_file.read_text(encoding="utf-8")


async def test_attach_session_relaunches_external_launcher(
    monkeypatch: MonkeyPatch,
    tmp_path: Path,
) -> None:
    service, task_service, workspace_service = _build_service(default_backend=None)
    task_service.get_task = AsyncMock(
        return_value=_TaskStub(terminal_backend=cast("Any", "cursor"))
    )
    workspace_service.get_path = AsyncMock(return_value=tmp_path)

    bundle_dir = tmp_path / ".kagan"
    bundle_dir.mkdir(parents=True)
    (bundle_dir / "start_prompt.md").write_text("prompt", encoding="utf-8")

    exec_mock = AsyncMock(return_value=_Proc())
    monkeypatch.setattr("kagan.services.sessions.asyncio.create_subprocess_exec", exec_mock)

    attached = await service.attach_session("task-1")

    assert attached is True
    await_args = exec_mock.await_args
    assert await_args is not None
    assert await_args.args == (
        "cursor",
        "--new-window",
        str(tmp_path),
        str(tmp_path / ".kagan" / "start_prompt.md"),
    )


async def test_session_exists_for_external_checks_startup_bundle(tmp_path: Path) -> None:
    service, task_service, workspace_service = _build_service(default_backend=None)
    task_service.get_task = AsyncMock(
        return_value=_TaskStub(terminal_backend=cast("Any", "cursor"))
    )
    workspace_service.get_path = AsyncMock(return_value=tmp_path)

    assert await service.session_exists("task-1") is False

    bundle_dir = tmp_path / ".kagan"
    bundle_dir.mkdir(parents=True)
    (bundle_dir / "session.json").write_text("{}", encoding="utf-8")

    assert await service.session_exists("task-1") is True
