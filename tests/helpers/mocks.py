"""Mock factories for tests."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, Any
from unittest.mock import AsyncMock, MagicMock

if TYPE_CHECKING:
    from textual.message import Message

    from kagan.config import AgentConfig, KaganConfig

from kagan.agents.worktree import WorktreeManager


class MessageCapture:
    """Capture Textual messages from widgets for testing.

    Usage:
        capture = MessageCapture()
        with patch.object(widget, "post_message", capture):
            widget.action_select()

        assert len(capture.messages) == 1
        assert isinstance(capture.messages[0], MyWidget.Completed)
    """

    def __init__(self) -> None:
        self.messages: list[Any] = []

    def __call__(self, message: Message) -> bool:
        """Capture a message and return True (as post_message expects)."""
        self.messages.append(message)
        return True

    def clear(self) -> None:
        """Clear captured messages."""
        self.messages.clear()

    def get_last(self) -> Any:
        """Get the last captured message."""
        return self.messages[-1] if self.messages else None

    def get_first(self, message_type: type) -> Any:
        """Get the first message of the given type."""
        for msg in self.messages:
            if isinstance(msg, message_type):
                return msg
        return None

    def assert_single(self, message_type: type) -> Any:
        """Assert exactly one message of given type was captured, return it."""
        assert len(self.messages) == 1, f"Expected 1 message, got {len(self.messages)}"
        msg = self.messages[0]
        assert isinstance(msg, message_type), f"Expected {message_type}, got {type(msg)}"
        return msg

    def assert_contains(self, message_type: type) -> Any:
        """Assert at least one message of given type was captured, return it."""
        msg = self.get_first(message_type)
        actual_types = [type(m).__name__ for m in self.messages]
        assert msg is not None, (
            f"Expected message of type {message_type.__name__}, got: {actual_types}"
        )
        return msg


class AgentTestHarness:
    """Test harness for Agent error testing.

    Provides:
    - Pre-configured Agent with mocked message target
    - Message capture via posted_messages list
    - Assertion helpers for common checks
    """

    def __init__(self, tmp_path: Path, agent_config: AgentConfig, *, read_only: bool = False):
        from kagan.acp.agent import Agent

        self.agent = Agent(tmp_path, agent_config, read_only=read_only)
        self.agent._message_target = MagicMock()
        self.agent._message_target.post_message = MagicMock(return_value=True)
        self.posted_messages: list[Any] = []
        self.agent.post_message = MagicMock(
            side_effect=lambda m, **kw: self.posted_messages.append(m) or True
        )

    def assert_posted_fail(self, contains: str | None = None) -> None:
        """Assert that AgentFail was posted, optionally checking message content."""
        from kagan.acp import messages

        assert len(self.posted_messages) == 1
        msg = self.posted_messages[0]
        assert isinstance(msg, messages.AgentFail)
        if contains:
            assert contains in msg.message or contains in (msg.details or "")

    def assert_posted_ready(self) -> None:
        """Assert that AgentReady was posted."""
        from kagan.acp import messages

        assert any(isinstance(m, messages.AgentReady) for m in self.posted_messages)

    def mock_process(self, readline_responses: list[bytes]) -> MagicMock:
        """Create a mock process with specified readline responses."""
        proc = MagicMock()
        proc.pid = 12345
        proc.stdin = MagicMock()
        proc.stdout = MagicMock()
        proc.stderr = MagicMock()
        call_count = 0

        async def mock_readline():
            nonlocal call_count
            if call_count < len(readline_responses):
                result = readline_responses[call_count]
                call_count += 1
                return result
            return b""

        proc.stdout.readline = mock_readline
        return proc


class MergeScenarioBuilder:
    """Builder for WorktreeManager merge test scenarios."""

    def __init__(self, tmp_path: Path):
        self.tmp_path = tmp_path
        self.manager = WorktreeManager(repo_root=tmp_path)
        self.mock_run_git = AsyncMock()
        self.manager._run_git = self.mock_run_git
        self.merge_path = tmp_path / ".kagan" / "merge-worktree"
        self.manager.ensure_merge_worktree = AsyncMock(return_value=self.merge_path)
        self.manager._reset_merge_worktree = AsyncMock(return_value=self.merge_path)
        self.manager._fast_forward_base = AsyncMock(return_value=(True, "Fast-forwarded main"))
        self.manager._merge_in_progress = AsyncMock(return_value=False)
        self.ticket_id = ""
        self.branch_name = ""
        self.commits: list[str] = []
        self.conflict_marker = ""

    def with_worktree(self, ticket_id: str) -> MergeScenarioBuilder:
        """Create worktree directory for ticket."""
        path = self.tmp_path / ".kagan" / "worktrees" / ticket_id
        path.mkdir(parents=True)
        self.ticket_id = ticket_id
        return self

    def with_branch(self, branch_name: str) -> MergeScenarioBuilder:
        """Set branch name response."""
        self.branch_name = branch_name
        self.manager.get_branch_name = AsyncMock(return_value=branch_name)
        return self

    def with_commits(self, commits: list[str]) -> MergeScenarioBuilder:
        """Set commit log response."""
        self.commits = commits
        self.manager.get_commit_log = AsyncMock(return_value=commits)
        return self

    def with_conflict(self, marker: str = "UU") -> MergeScenarioBuilder:
        """Configure conflict scenario."""
        self.conflict_marker = marker
        return self

    def build_success_responses(self) -> list:
        """Build mock responses for successful squash merge."""
        return [
            ("", ""),  # merge --squash
            ("M file.py", ""),  # status (conflict check - no conflict)
            ("", ""),  # commit
        ]

    def build_regular_merge_responses(self) -> list:
        """Build mock responses for successful regular merge."""
        return [
            ("Merge made by the 'ort' strategy.", ""),  # merge (no squash)
        ]

    def build_conflict_responses(self) -> list:
        """Build mock responses for squash conflict scenario."""
        return [
            ("", ""),  # merge --squash
            (f"{self.conflict_marker} file.py", ""),  # status with conflict
        ]

    def build_regular_conflict_responses(self, in_stderr: bool = False) -> list:
        """Build mock responses for regular merge conflict."""
        conflict_msg = "CONFLICT (content): Merge conflict in file.py"
        return [
            ("", conflict_msg) if in_stderr else (conflict_msg, ""),
        ]

    def build_uncommitted_changes_response(self) -> list:
        """Build mock responses for uncommitted changes in main repo."""
        return [
            ("M tests/helpers/pages.py", ""),  # status --porcelain shows uncommitted changes
        ]


def create_mock_worktree_manager() -> MagicMock:
    """Create a mock WorktreeManager with async methods."""
    from kagan.agents.worktree import WorktreeManager

    manager = MagicMock(spec=WorktreeManager)
    manager.get_path = AsyncMock(return_value=Path("/tmp/worktree"))
    manager.create = AsyncMock(return_value=Path("/tmp/worktree"))
    manager.delete = AsyncMock()
    manager.list_all = AsyncMock(return_value=[])
    manager.get_commit_log = AsyncMock(return_value=["feat: initial"])
    manager.get_diff_stats = AsyncMock(return_value="1 file changed")
    manager.merge_to_main = AsyncMock(return_value=(True, "Merged"))
    manager.prepare_merge_conflicts = AsyncMock(return_value=(True, "Merge conflicts prepared"))
    manager.get_merge_worktree_path = AsyncMock(return_value=Path("/tmp/merge-worktree"))
    return manager


def create_mock_agent(response: str = "Done! <complete/>") -> MagicMock:
    """Create a mock ACP agent with configurable response."""
    agent = MagicMock()
    agent._read_only = False  # Default to normal (non-read-only) mode
    agent.set_auto_approve = MagicMock()
    agent.start = MagicMock()
    agent.wait_ready = AsyncMock()
    agent.send_prompt = AsyncMock()
    agent.get_response_text = MagicMock(return_value=response)
    agent.stop = AsyncMock()
    return agent


def create_mock_session_manager() -> MagicMock:
    """Create a mock SessionManager."""
    manager = MagicMock()
    manager.create_session = AsyncMock(return_value="session-123")
    manager.kill_session = AsyncMock()
    manager.kill_resolution_session = AsyncMock()
    manager.list_sessions = AsyncMock(return_value=[])
    manager.send_keys = AsyncMock()
    return manager


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


def create_mock_scheduler() -> MagicMock:
    """Create a mock Scheduler."""
    manager = MagicMock()
    manager.is_running = MagicMock(return_value=False)
    manager.stop_ticket = AsyncMock()
    manager.start_ticket = AsyncMock()
    return manager


def create_test_config(
    auto_start: bool = True,
    auto_merge: bool = False,
    max_concurrent: int = 2,
    max_iterations: int = 3,
) -> KaganConfig:
    """Create a KaganConfig for testing."""
    from kagan.config import AgentConfig, GeneralConfig, KaganConfig

    return KaganConfig(
        general=GeneralConfig(
            auto_start=auto_start,
            auto_merge=auto_merge,
            max_concurrent_agents=max_concurrent,
            max_iterations=max_iterations,
            iteration_delay_seconds=0.01,
            default_worker_agent="test",
            default_base_branch="main",
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


class SubprocessMockBuilder:
    """Builder for complex subprocess mocking scenarios.

    Example:
        builder = SubprocessMockBuilder()
        mock_subprocess = (
            builder
            .on_call(1, stdout=b"output1")
            .on_call(2, stdout=b"output2")
            .error_on_call(3, message="failure")
            .build()
        )
        mocker.patch("asyncio.create_subprocess_exec", side_effect=mock_subprocess)
    """

    def __init__(self):
        self._call_responses: dict[int, tuple[bytes, bytes, int]] = {}
        self._default_response: tuple[bytes, bytes, int] = (b"", b"", 0)

    def on_call(
        self, call_num: int, stdout: bytes, stderr: bytes = b"", returncode: int = 0
    ) -> SubprocessMockBuilder:
        """Configure response for a specific call number (1-indexed)."""
        self._call_responses[call_num] = (stdout, stderr, returncode)
        return self

    def error_on_call(self, call_num: int, message: str = "error") -> SubprocessMockBuilder:
        """Configure an error response for a specific call number."""
        self._call_responses[call_num] = (b"", message.encode(), 1)
        return self

    def with_default(
        self, stdout: bytes = b"", stderr: bytes = b"", returncode: int = 0
    ) -> SubprocessMockBuilder:
        """Set the default response for unconfigured calls."""
        self._default_response = (stdout, stderr, returncode)
        return self

    def build(self):
        """Build the mock subprocess factory function."""
        call_count = [0]
        responses = self._call_responses
        default = self._default_response

        async def mock_subprocess(*args, **kwargs):
            call_count[0] += 1
            stdout, stderr, code = responses.get(call_count[0], default)
            proc = MagicMock()
            proc.communicate = AsyncMock(return_value=(stdout, stderr))
            proc.returncode = code
            proc.pid = 12345 + call_count[0]
            proc.stdout = MagicMock()
            proc.stdout.readline = AsyncMock(return_value=b"")
            proc.stderr = MagicMock()
            proc.stderr.readline = AsyncMock(return_value=b"")
            proc.wait = AsyncMock(return_value=code)
            proc.terminate = MagicMock()
            proc.kill = MagicMock()
            return proc

        return mock_subprocess
