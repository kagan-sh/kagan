from __future__ import annotations

import json
import platform
from dataclasses import dataclass, field
from pathlib import Path
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, Literal, cast
from unittest.mock import AsyncMock

import pytest

from kagan.core.api import (
    InvalidWorktreePathError,
    KaganAPI,
    SessionCreateFailedError,
)
from kagan.core.config import AgentConfig, KaganConfig
from kagan.core.models.enums import PairTerminalBackend, SessionType, TaskStatus, TaskType
from kagan.core.request_handlers import handle_session_create
from kagan.core.services.session_bundle import (
    build_external_launcher_command,
    bundle_json_path,
    bundle_prompt_path,
    write_startup_bundle,
)
from kagan.core.services.sessions import SessionServiceImpl

if TYPE_CHECKING:
    from pytest import MonkeyPatch

    from kagan.core.services.tasks import TaskService
    from kagan.core.services.workspaces import WorkspaceService


@dataclass
class _TaskStub:
    id: str = "task-1"
    title: str = "Task Title"
    description: str = "Task description"
    status: TaskStatus = TaskStatus.BACKLOG
    task_type: TaskType = TaskType.PAIR
    terminal_backend: PairTerminalBackend | None = None
    agent_backend: str | None = None
    base_branch: str | None = None
    acceptance_criteria: list[str] = field(default_factory=list)
    agent_short_name: str = "copilot"

    def get_agent_config(self, config: KaganConfig) -> Any:
        del config
        return AgentConfig(
            identity=f"{self.agent_short_name}.example.com",
            name=self.agent_short_name.title(),
            short_name=self.agent_short_name,
            run_command={"*": self.agent_short_name},
            interactive_command={"*": self.agent_short_name},
            active=True,
            model_env_var="",
        )


class _Proc:
    async def wait(self) -> int:
        return 0


_IS_WINDOWS = platform.system() == "Windows"


def _build_service(
    default_backend: Literal["tmux", "vscode", "cursor"] | None = None,
) -> tuple[SessionServiceImpl, SimpleNamespace, SimpleNamespace]:
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
    service = SessionServiceImpl(
        project_root=Path("."),
        task_service=cast("Any", task_service),
        workspace_service=cast("Any", workspace_service),
        config=config,
    )
    service._write_context_files = AsyncMock()  # type: ignore[method-assign]
    return service, task_service, workspace_service


def _build_launch_service() -> SessionServiceImpl:
    return SessionServiceImpl(
        project_root=Path("."),
        task_service=cast("TaskService", object()),
        workspace_service=cast("WorkspaceService", object()),
        config=KaganConfig(),
    )


def _api(**services: object) -> KaganAPI:
    ctx = SimpleNamespace(**services)
    return KaganAPI(cast("Any", ctx))


def _agent(short_name: str, interactive_command: str = "agent-cli") -> AgentConfig:
    return AgentConfig(
        identity=f"{short_name}.example.com",
        name=short_name.title(),
        short_name=short_name,
        run_command={"*": "agent-acp"},
        interactive_command={"*": interactive_command},
        active=True,
        model_env_var="",
    )


def _q(s: str) -> str:
    """Return the expected shell-quoted form for the current platform."""
    if _IS_WINDOWS:
        return f'"{s}"'
    return f"'{s}'"


async def test_create_session_prefers_task_override_over_config(monkeypatch: MonkeyPatch) -> None:
    service, task_service, _workspace_service = _build_service(default_backend="tmux")
    task = _TaskStub(terminal_backend=PairTerminalBackend.CURSOR)

    run_tmux_mock = AsyncMock(return_value="")
    exec_mock = AsyncMock(return_value=_Proc())
    monkeypatch.setattr("kagan.core.services.sessions.run_tmux", run_tmux_mock)
    monkeypatch.setattr("kagan.core.services.sessions.spawn_exec", exec_mock)

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
    monkeypatch.setattr("kagan.core.services.sessions.run_tmux", run_tmux_mock)
    monkeypatch.setattr("kagan.core.services.sessions.spawn_exec", exec_mock)

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
    monkeypatch.setattr("kagan.core.services.sessions.run_tmux", run_tmux_mock)
    monkeypatch.setattr("kagan.core.services.sessions.spawn_exec", exec_mock)

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
    monkeypatch.setattr("kagan.core.services.sessions.run_tmux", run_tmux_mock)
    monkeypatch.setattr("kagan.core.services.sessions.spawn_exec", exec_mock)

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
    service._write_context_files = SessionServiceImpl._write_context_files.__get__(
        service, SessionServiceImpl
    )
    task = _TaskStub(terminal_backend=cast("Any", "vscode"))

    run_tmux_mock = AsyncMock(return_value="")
    exec_mock = AsyncMock(return_value=_Proc())
    monkeypatch.setattr("kagan.core.services.sessions.run_tmux", run_tmux_mock)
    monkeypatch.setattr("kagan.core.services.sessions.spawn_exec", exec_mock)

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
    vscode_content = vscode_mcp_file.read_text(encoding="utf-8")
    assert '"servers"' in vscode_content
    assert "--session-id" in vscode_content
    assert "task:" in vscode_content
    assert "--capability" in vscode_content
    assert "pair_worker" in vscode_content
    assert "--identity" in vscode_content
    assert "kagan" in vscode_content

    call = task_service.create_session_record.await_args.kwargs
    assert call["session_type"] == SessionType.SCRIPT


async def test_create_session_cursor_writes_cursor_mcp_config(
    monkeypatch: MonkeyPatch,
    tmp_path: Path,
) -> None:
    service, _task_service, _workspace_service = _build_service(default_backend=None)
    service._write_context_files = SessionServiceImpl._write_context_files.__get__(
        service, SessionServiceImpl
    )
    task = _TaskStub(terminal_backend=cast("Any", "cursor"))

    exec_mock = AsyncMock(return_value=_Proc())
    monkeypatch.setattr("kagan.core.services.sessions.spawn_exec", exec_mock)

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
    cursor_content = cursor_mcp_file.read_text(encoding="utf-8")
    assert '"mcpServers"' in cursor_content
    assert "--session-id" in cursor_content
    assert "task:" in cursor_content
    assert "--capability" in cursor_content
    assert "pair_worker" in cursor_content
    assert "--identity" in cursor_content
    assert "kagan" in cursor_content


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
    monkeypatch.setattr("kagan.core.services.sessions.spawn_exec", exec_mock)

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


async def test_create_session_writes_gemini_project_mcp_config(
    monkeypatch: MonkeyPatch,
    tmp_path: Path,
) -> None:
    service, _task_service, _workspace_service = _build_service(default_backend="tmux")
    service._write_context_files = SessionServiceImpl._write_context_files.__get__(
        service, SessionServiceImpl
    )
    task = _TaskStub(agent_short_name="gemini")

    run_tmux_mock = AsyncMock(return_value="")
    monkeypatch.setattr("kagan.core.services.sessions.run_tmux", run_tmux_mock)

    await service.create_session(task, tmp_path)

    gemini_settings = tmp_path / ".gemini" / "settings.json"
    assert gemini_settings.exists()
    content = gemini_settings.read_text(encoding="utf-8")
    assert '"mcpServers"' in content
    assert '"kagan"' in content
    assert "--session-id" in content
    assert "task:task-1" in content
    assert "--capability" in content
    assert "pair_worker" in content
    assert "--identity" in content


async def test_create_session_writes_kimi_local_mcp_config_and_uses_it(
    monkeypatch: MonkeyPatch,
    tmp_path: Path,
) -> None:
    service, _task_service, _workspace_service = _build_service(default_backend="tmux")
    service._write_context_files = SessionServiceImpl._write_context_files.__get__(
        service, SessionServiceImpl
    )
    task = _TaskStub(agent_short_name="kimi")

    run_tmux_mock = AsyncMock(return_value="")
    monkeypatch.setattr("kagan.core.services.sessions.run_tmux", run_tmux_mock)

    await service.create_session(task, tmp_path)

    kimi_mcp = tmp_path / ".kagan" / "kimi-mcp.json"
    assert kimi_mcp.exists()
    content = kimi_mcp.read_text(encoding="utf-8")
    assert '"mcpServers"' in content
    assert "--session-id" in content
    assert "task:task-1" in content
    assert "--capability" in content
    assert "pair_worker" in content
    assert "--identity" in content

    send_keys_calls = [call for call in run_tmux_mock.await_args_list if "send-keys" in call.args]
    assert send_keys_calls
    last_send = send_keys_calls[-1]
    command_payload = str(last_send.args[3])
    assert "--mcp-config-file" in command_payload
    assert str(kimi_mcp) in command_payload


async def test_write_startup_bundle_persists_prompt_and_metadata(tmp_path: Path) -> None:
    await write_startup_bundle(
        task_id="task-42",
        worktree_path=tmp_path,
        session_name="kagan-task-42",
        backend="cursor",
        startup_prompt="hello from kagan",
    )

    prompt_file = bundle_prompt_path(tmp_path)
    session_file = bundle_json_path(tmp_path)
    assert prompt_file.exists()
    assert prompt_file.read_text(encoding="utf-8") == "hello from kagan"
    assert session_file.exists()

    payload = json.loads(session_file.read_text(encoding="utf-8"))
    assert payload["task_id"] == "task-42"
    assert payload["session_name"] == "kagan-task-42"
    assert payload["backend"] == "cursor"
    assert payload["worktree"] == str(tmp_path)
    assert payload["prompt_file"] == str(prompt_file)


def test_build_external_launcher_command_for_vscode(tmp_path: Path) -> None:
    assert build_external_launcher_command("vscode", tmp_path) == [
        "code",
        "--new-window",
        str(tmp_path),
        str(bundle_prompt_path(tmp_path)),
    ]


def test_build_external_launcher_command_rejects_unknown_backend(tmp_path: Path) -> None:
    with pytest.raises(RuntimeError, match="Unsupported external PAIR launcher"):
        build_external_launcher_command("tmux", tmp_path)


async def test_session_create_rejects_mismatched_worktree_path(tmp_path: Path) -> None:
    expected_worktree = tmp_path / "worktrees" / "task-1"
    expected_worktree.mkdir(parents=True)
    mismatched_worktree = tmp_path / "repo-root"
    mismatched_worktree.mkdir(parents=True)

    async def _create_session(task_id, *, worktree_path=None, reuse_if_exists=True):
        raise InvalidWorktreePathError(
            task_id,
            f"worktree_path must point to the task workspace. Expected: {expected_worktree}",
        )

    f = _api()
    f.create_session = AsyncMock(side_effect=_create_session)

    result = await handle_session_create(
        f,
        {"task_id": "task-1", "worktree_path": str(mismatched_worktree)},
    )

    assert result["success"] is False
    assert result["code"] == "INVALID_WORKTREE_PATH"
    assert result["next_tool"] == "sessions_exists"
    assert result["next_arguments"] == {"task_id": "task-1"}


async def test_session_create_returns_structured_error_when_backend_fails(
    tmp_path: Path,
) -> None:
    async def _create_session(task_id, *, worktree_path=None, reuse_if_exists=True):
        raise SessionCreateFailedError(task_id, RuntimeError("tmux unavailable"))

    f = _api()
    f.create_session = AsyncMock(side_effect=_create_session)

    result = await handle_session_create(f, {"task_id": "task-1"})

    assert result["success"] is False
    assert result["code"] == "SESSION_CREATE_FAILED"
    assert "tmux unavailable" in result["message"]


@pytest.mark.parametrize(
    ("short_name", "expected_fmt"),
    [
        ("codex", "agent-cli {q}"),
        ("gemini", "agent-cli {q}"),
        ("kimi", "agent-cli --prompt {q}"),
        ("copilot", "agent-cli"),
    ],
)
def test_build_launch_command_prompt_style(short_name: str, expected_fmt: str) -> None:
    service = _build_launch_service()

    cmd = service._build_launch_command(_agent(short_name), "hello world")

    expected = expected_fmt.format(q=_q("hello world"))
    assert cmd == expected


def test_build_launch_command_opencode_uses_prompt_flag_and_model() -> None:
    service = _build_launch_service()

    cmd = service._build_launch_command(_agent("opencode"), "hello world", model="gpt-5")

    assert cmd == f"agent-cli --model gpt-5 --prompt {_q('hello world')}"


def test_build_launch_command_claude_uses_positional_prompt_and_model() -> None:
    service = _build_launch_service()

    cmd = service._build_launch_command(_agent("claude"), "hello world", model="sonnet")

    assert cmd == f"agent-cli --model sonnet {_q('hello world')}"


@pytest.mark.parametrize(
    ("short_name", "expected"),
    [
        ("codex", "agent-cli --model gpt-5.2-codex {q}"),
        ("gemini", "agent-cli --model gemini-2.5-flash {q}"),
        ("kimi", "agent-cli --model kimi-k2-turbo --prompt {q}"),
    ],
)
def test_build_launch_command_additional_agents_accept_model_flag(
    short_name: str,
    expected: str,
) -> None:
    service = _build_launch_service()
    model = {
        "codex": "gpt-5.2-codex",
        "gemini": "gemini-2.5-flash",
        "kimi": "kimi-k2-turbo",
    }[short_name]

    cmd = service._build_launch_command(_agent(short_name), "hello world", model=model)

    assert cmd == expected.format(q=_q("hello world"))


def test_build_launch_command_codex_injects_session_scoped_mcp_overrides() -> None:
    service = _build_launch_service()

    cmd = service._build_launch_command(
        _agent("codex"),
        "hello world",
        model="gpt-5.2-codex",
        task_id="TASK-123",
    )

    assert cmd is not None
    assert "agent-cli --model gpt-5.2-codex " in cmd
    assert "-c " in cmd
    assert "mcp_servers.kagan.command" in cmd
    assert "mcp_servers.kagan.args" in cmd
    assert "--session-id" in cmd
    assert "task:TASK-123" in cmd
    assert "--capability" in cmd
    assert "pair_worker" in cmd
    assert "--identity" in cmd
    assert "kagan" in cmd


def test_build_launch_command_kimi_uses_local_mcp_config_file(tmp_path: Path) -> None:
    service = _build_launch_service()

    cmd = service._build_launch_command(
        _agent("kimi"),
        "hello world",
        task_id="TASK-456",
        worktree_path=tmp_path,
    )

    assert cmd is not None
    expected_path = str(tmp_path / ".kagan" / "kimi-mcp.json")
    assert "--mcp-config-file" in cmd
    assert expected_path in cmd
