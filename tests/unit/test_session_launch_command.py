from __future__ import annotations

import platform
from pathlib import Path
from typing import TYPE_CHECKING, cast

import pytest

from kagan.config import AgentConfig, KaganConfig
from kagan.services.sessions import SessionService

if TYPE_CHECKING:
    from kagan.services.tasks import TaskService
    from kagan.services.workspaces import WorkspaceService

_IS_WINDOWS = platform.system() == "Windows"


def _build_service() -> SessionService:
    return SessionService(
        project_root=Path("."),
        task_service=cast("TaskService", object()),
        workspace_service=cast("WorkspaceService", object()),
        config=KaganConfig(),
    )


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
    service = _build_service()

    cmd = service._build_launch_command(_agent(short_name), "hello world")

    expected = expected_fmt.format(q=_q("hello world"))
    assert cmd == expected


def test_build_launch_command_opencode_uses_prompt_flag_and_model() -> None:
    service = _build_service()

    cmd = service._build_launch_command(_agent("opencode"), "hello world", model="gpt-5")

    assert cmd == f"agent-cli --model gpt-5 --prompt {_q('hello world')}"


def test_build_launch_command_claude_uses_positional_prompt_and_model() -> None:
    service = _build_service()

    cmd = service._build_launch_command(_agent("claude"), "hello world", model="sonnet")

    assert cmd == f"agent-cli --model sonnet {_q('hello world')}"
