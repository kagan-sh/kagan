"""Mock factories for tests."""

from __future__ import annotations

import re
from pathlib import Path
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any
from unittest.mock import AsyncMock, MagicMock

from acp.schema import ToolCall

from kagan.acp.buffers import AgentBuffers
from kagan.tmux import TmuxError

if TYPE_CHECKING:
    from collections.abc import Callable

    from kagan.config import AgentConfig, KaganConfig


def create_mock_workspace_service() -> MagicMock:
    """Create a mock WorkspaceService with async methods."""
    manager = MagicMock()
    workspace_stub = SimpleNamespace(id="workspace-test")
    manager.get_path = AsyncMock(return_value=Path("/tmp/worktree"))
    manager.create = AsyncMock(return_value=Path("/tmp/worktree"))
    manager.delete = AsyncMock()
    manager.list_workspaces = AsyncMock(return_value=[workspace_stub])
    manager.get_workspace_repos = AsyncMock(return_value=[])
    manager.get_commit_log = AsyncMock(return_value=["feat: initial"])
    manager.get_diff_stats = AsyncMock(return_value="1 file changed")
    manager.prepare_merge_conflicts = AsyncMock(return_value=(True, "Merge conflicts prepared"))
    manager.get_merge_worktree_path = AsyncMock(return_value=Path("/tmp/merge-worktree"))
    manager.get_files_changed_on_base = AsyncMock(return_value=[])
    manager.rebase_onto_base = AsyncMock(return_value=(True, "", []))
    return manager


def create_mock_agent(response: str = "Done! <complete/>") -> MagicMock:
    """Create a mock ACP agent with configurable response."""
    agent = MagicMock()
    buffers = AgentBuffers()
    buffers.append_response(response)
    agent._read_only = False
    agent.set_auto_approve = MagicMock()
    agent.start = MagicMock()
    agent.wait_ready = AsyncMock()
    agent.send_prompt = AsyncMock()
    agent.get_response_text = MagicMock(side_effect=buffers.get_response_text)
    agent.get_messages = MagicMock(side_effect=lambda: list(buffers.messages))
    agent.stop = AsyncMock()
    return agent


def create_mock_process(pid: int = 12345, returncode: int | None = None) -> MagicMock:
    """Create a mock asyncio subprocess."""
    proc = MagicMock()
    proc.pid = pid
    proc.returncode = returncode
    proc.stdout = MagicMock()
    proc.stdout.readline = AsyncMock(return_value=b"")
    proc.stderr = MagicMock()
    proc.stderr.readline = AsyncMock(return_value=b"")
    proc.wait = AsyncMock(return_value=0)
    proc.terminate = MagicMock()
    proc.kill = MagicMock()
    proc.communicate = AsyncMock(return_value=(b"", b""))
    return proc


def create_test_agent_config(
    identity: str = "test.agent",
    name: str = "Test Agent",
    short_name: str = "test",
    run_command: str = "echo test",
):
    """Create a minimal AgentConfig for testing."""
    from kagan.config import AgentConfig

    return AgentConfig(
        identity=identity,
        name=name,
        short_name=short_name,
        run_command={"*": run_command},
    )


def create_test_config(
    auto_review: bool = True,
    max_concurrent: int = 2,
) -> KaganConfig:
    """Create a KaganConfig for testing."""
    from kagan.config import AgentConfig, GeneralConfig, KaganConfig

    return KaganConfig(
        general=GeneralConfig(
            auto_review=auto_review,
            max_concurrent_agents=max_concurrent,
            default_worker_agent="test",
            default_base_branch="main",
            default_pair_terminal_backend="tmux",
        ),
        agents={
            "test": AgentConfig(
                identity="test.agent",
                name="Test Agent",
                short_name="test",
                run_command={"*": "echo test"},
            )
        },
    )


def _coerce_tool_call(tool_call: Any) -> ToolCall:
    if isinstance(tool_call, ToolCall):
        return tool_call
    if not isinstance(tool_call, dict):
        return ToolCall(toolCallId="unknown", title="Tool call")
    data = dict(tool_call)
    if "toolCallId" not in data:
        if "id" in data:
            data["toolCallId"] = data["id"]
        elif "tool_call_id" in data:
            data["toolCallId"] = data["tool_call_id"]
    if not data.get("title"):
        data["title"] = data.get("name") or "Tool call"
    if "rawInput" not in data:
        for key in ("arguments", "input", "params", "args"):
            if key in data:
                data["rawInput"] = data[key]
                break
    return ToolCall.model_validate(data)


class MockAgent:
    """Mock ACP agent with controllable responses for snapshot / E2E testing.

    Simulates the Agent interface without spawning real processes.
    Responses are controlled via ``set_response`` / ``set_tool_calls``.
    """

    def __init__(
        self,
        project_root: Path,
        agent_config: AgentConfig,
        *,
        read_only: bool = False,
    ) -> None:
        self.project_root = project_root
        self._agent_config = agent_config
        self._read_only = read_only
        self._buffers = AgentBuffers()
        self._tool_calls: dict[str, ToolCall] = {}
        self._thinking_text: str = ""
        self._ready = False
        self._stopped = False
        self._auto_approve = False
        self._model_override: str | None = None
        self._message_target: Any = None

        self._buffers.append_response("Done. <complete/>")

    def _stream_text(self) -> str:
        text = self._buffers.get_response_text()
        if not text:
            return ""
        text = re.sub(r"<complete\\s*/?>", "", text, flags=re.IGNORECASE)
        text = re.sub(r"<continue\\s*/?>", "", text, flags=re.IGNORECASE)
        text = re.sub(r"<blocked\\s+reason=\"[^\"]+\"\\s*/?>", "", text, flags=re.IGNORECASE)
        text = re.sub(r"<approve\\s*[^>]*?/?>", "", text, flags=re.IGNORECASE)
        text = re.sub(r"<reject\\s+reason=\"[^\"]+\"\\s*/?>", "", text, flags=re.IGNORECASE)
        return text.strip()

    def set_response(self, text: str) -> None:
        self._buffers.clear_response()
        self._buffers.append_response(text)

    def set_tool_calls(self, tool_calls: dict[str, Any]) -> None:
        self._tool_calls = {
            tool_call_id: _coerce_tool_call(tool_call)
            for tool_call_id, tool_call in tool_calls.items()
        }

    def set_thinking_text(self, text: str) -> None:
        self._thinking_text = text

    def set_message_target(self, target: Any) -> None:
        self._message_target = target

    def set_auto_approve(self, enabled: bool) -> None:
        self._auto_approve = enabled

    def set_model_override(self, model_id: str | None) -> None:
        self._model_override = model_id

    def start(self, message_target: Any = None) -> None:
        self._message_target = message_target
        self._ready = True
        if self._message_target:
            from kagan.acp import messages

            self._message_target.post_message(messages.AgentReady())

    async def wait_ready(self, timeout: float = 30.0) -> None:
        self._ready = True
        if self._message_target:
            from kagan.acp import messages

            self._message_target.post_message(messages.AgentReady())

    async def send_prompt(self, prompt: str) -> str | None:
        import asyncio

        if self._message_target:
            from kagan.acp import messages

            if self._thinking_text:
                self._message_target.post_message(messages.Thinking("text", self._thinking_text))
                await asyncio.sleep(0.05)

            stream_text = self._stream_text()
            if stream_text:
                self._message_target.post_message(messages.AgentUpdate("text", stream_text))
                await asyncio.sleep(0.1)

            for tool_call in self._tool_calls.values():
                self._message_target.post_message(messages.ToolCall(tool_call))
                await asyncio.sleep(0.05)

            await asyncio.sleep(0.05)
            self._message_target.post_message(messages.AgentComplete())
            await asyncio.sleep(0.05)

        return "end_turn"

    def get_response_text(self) -> str:
        return self._buffers.get_response_text()

    def get_messages(self) -> list[Any]:
        return list(self._buffers.messages)

    def get_tool_calls(self) -> dict[str, ToolCall]:
        return self._tool_calls

    @property
    def tool_calls(self) -> dict[str, ToolCall]:
        return self._tool_calls

    def get_thinking_text(self) -> str:
        return self._thinking_text

    def clear_tool_calls(self) -> None:
        self._tool_calls.clear()

    async def stop(self) -> None:
        self._stopped = True
        self._buffers.clear_all()

    async def cancel(self) -> bool:
        return True


class SmartMockAgent(MockAgent):
    """Route-based mock agent that adapts responses based on prompt keywords.

    Replaces the ad-hoc PairFlowAgent, FullFlowAgent, JourneyAgent classes.

    Usage::

        routes = {
            "propose_plan": (PLAN_RESPONSE, plan_tool_calls),
            "Code Review Specialist": (REVIEW_RESPONSE, {}),
        }
        agent = SmartMockAgent(root, config, routes=routes)

    If no route matches, the *default* response/tool_calls pair is used.
    An optional *on_default* async callback is called when the default
    route fires (e.g. to commit files in the worktree).
    """

    def __init__(
        self,
        project_root: Path,
        agent_config: AgentConfig,
        *,
        routes: dict[str, tuple[str, dict[str, Any]]] | None = None,
        default: tuple[str, dict[str, Any]] | None = None,
        on_default: Callable[..., Any] | None = None,
        read_only: bool = False,
    ) -> None:
        super().__init__(project_root, agent_config, read_only=read_only)
        self._routes = routes or {}
        self._default = default or ("Done. <complete/>", {})
        self._on_default = on_default

    async def send_prompt(self, prompt: str) -> str | None:
        for keyword, (response, tool_calls) in self._routes.items():
            if keyword in prompt:
                self.set_response(response)
                self.set_tool_calls(tool_calls)
                return await super().send_prompt(prompt)

        response, tool_calls = self._default
        if self._on_default is not None:
            import inspect

            result = self._on_default(self)
            if inspect.isawaitable(result):
                await result
        self.set_response(response)
        self.set_tool_calls(tool_calls)
        return await super().send_prompt(prompt)


class MockAgentFactory:
    """Factory for creating MockAgent instances with controllable behavior.

    Works as a generic factory — pass ``agent_cls`` to use SmartMockAgent
    or any other MockAgent subclass.
    """

    def __init__(
        self,
        *,
        agent_cls: type[MockAgent] | None = None,
        agent_kwargs: dict[str, Any] | None = None,
    ) -> None:
        self._agent_cls = agent_cls or MockAgent
        self._agent_kwargs = agent_kwargs or {}
        self._default_response = "Done. <complete/>"
        self._default_tool_calls: dict[str, Any] = {}
        self._default_thinking = ""
        self._agents: list[MockAgent] = []

    def set_default_response(self, text: str) -> None:
        self._default_response = text

    def set_default_tool_calls(self, tool_calls: dict[str, Any]) -> None:
        self._default_tool_calls = tool_calls

    def set_default_thinking(self, text: str) -> None:
        self._default_thinking = text

    def get_last_agent(self) -> MockAgent | None:
        return self._agents[-1] if self._agents else None

    def get_all_agents(self) -> list[MockAgent]:
        return list(self._agents)

    def __call__(
        self,
        project_root: Path,
        agent_config: AgentConfig,
        *,
        read_only: bool = False,
    ) -> Any:
        agent = self._agent_cls(
            project_root,
            agent_config,
            read_only=read_only,
            **self._agent_kwargs,
        )
        agent.set_response(self._default_response)
        agent.set_tool_calls(dict(self._default_tool_calls))
        agent.set_thinking_text(self._default_thinking)
        self._agents.append(agent)
        return agent


def create_fake_tmux(
    sessions: dict[str, dict[str, Any]], *, strict: bool = False
) -> Callable[..., Any]:
    """Create a fake tmux run function for testing.

    Args:
        sessions: Dictionary to track created sessions
        strict: If True, raise TmuxError for invalid operations like
            kill-session or send-keys on non-existent sessions
    """

    async def fake_run_tmux(*args: str) -> str:
        if not args:
            return ""
        command, args_list = args[0], list(args)
        if command == "new-session" and "-s" in args_list:
            idx = args_list.index("-s")
            name = args_list[idx + 1] if idx + 1 < len(args_list) else None
            if name:
                cwd = args_list[args_list.index("-c") + 1] if "-c" in args_list else ""
                env: dict[str, str] = {}
                for i, val in enumerate(args_list):
                    if val == "-e" and i + 1 < len(args_list):
                        key, _, env_value = args_list[i + 1].partition("=")
                        env[key] = env_value
                sessions[name] = {"cwd": cwd, "env": env, "sent_keys": []}
        elif command == "kill-session" and "-t" in args_list:
            idx = args_list.index("-t")
            name = args_list[idx + 1] if idx + 1 < len(args_list) else None
            if strict and name and name not in sessions:
                raise TmuxError(f"session not found: {name}")
            if name:
                sessions.pop(name, None)
        elif command == "send-keys" and "-t" in args_list:
            idx = args_list.index("-t")
            name = args_list[idx + 1]
            if strict and name not in sessions:
                raise TmuxError(f"session not found: {name}")
            keys = args_list[idx + 2] if idx + 2 < len(args_list) else ""
            if name in sessions:
                sessions[name]["sent_keys"].append(keys)
        elif command == "list-sessions":
            return "\n".join(sorted(sessions.keys()))
        return ""

    return fake_run_tmux
