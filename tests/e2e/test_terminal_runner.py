"""Tests for TerminalRunner class."""

from __future__ import annotations

from collections import deque
from typing import TYPE_CHECKING

import pytest

from kagan.acp.terminal import TerminalRunner, TerminalState

if TYPE_CHECKING:
    from pathlib import Path

    from pytest_mock import MockerFixture

pytestmark = pytest.mark.e2e


class TestTerminalState:
    def test_state_defaults(self):
        state = TerminalState(output="hello", truncated=False)
        assert state.output == "hello"
        assert state.truncated is False
        assert state.return_code is None
        assert state.signal is None

    def test_state_with_return_code(self):
        state = TerminalState(output="done", truncated=True, return_code=0, signal="SIGTERM")
        assert state.return_code == 0
        assert state.signal == "SIGTERM"
        assert state.truncated is True


class TestTerminalRunnerInit:
    def test_defaults(self, tmp_path: Path):
        runner = TerminalRunner("t1", "echo", project_root=tmp_path)
        assert runner.terminal_id == "t1"
        assert runner.command == "echo"
        assert runner.args == []
        assert runner.cwd is None
        assert runner.env == {}
        assert runner.output_byte_limit is None
        assert runner.project_root == tmp_path

    def test_with_args_and_env(self, tmp_path: Path):
        runner = TerminalRunner(
            "t2",
            "python",
            args=["-c", "print(1)"],
            cwd="src",
            env={"FOO": "bar"},
            output_byte_limit=1024,
            project_root=tmp_path,
        )
        assert runner.args == ["-c", "print(1)"]
        assert runner.cwd == "src"
        assert runner.env == {"FOO": "bar"}
        assert runner.output_byte_limit == 1024


class TestOutputBuffering:
    def test_record_output_no_limit(self, tmp_path: Path):
        runner = TerminalRunner("t1", "echo", project_root=tmp_path)
        runner._record_output(b"hello")
        runner._record_output(b"world")
        assert runner._output_bytes_count == 10
        assert list(runner._output) == [b"hello", b"world"]

    def test_record_output_with_limit_no_trim(self, tmp_path: Path):
        runner = TerminalRunner("t1", "echo", output_byte_limit=100, project_root=tmp_path)
        runner._record_output(b"small")
        assert runner._output_bytes_count == 5
        assert len(runner._output) == 1

    def test_record_output_trims_oldest(self, tmp_path: Path):
        runner = TerminalRunner("t1", "echo", output_byte_limit=10, project_root=tmp_path)
        runner._record_output(b"aaaaa")  # 5 bytes
        runner._record_output(b"bbbbb")  # 5 bytes, total=10
        runner._record_output(b"ccccc")  # 5 bytes, total=15, should trim first
        assert runner._output_bytes_count == 10
        assert list(runner._output) == [b"bbbbb", b"ccccc"]

    def test_get_output_simple(self, tmp_path: Path):
        runner = TerminalRunner("t1", "echo", project_root=tmp_path)
        runner._output = deque([b"hello ", b"world"])
        runner._output_bytes_count = 11
        output, truncated = runner._get_output()
        assert output == "hello world"
        assert truncated is False

    def test_get_output_truncated(self, tmp_path: Path):
        runner = TerminalRunner("t1", "echo", output_byte_limit=5, project_root=tmp_path)
        runner._output = deque([b"0123456789"])
        runner._output_bytes_count = 10
        output, truncated = runner._get_output()
        assert truncated is True
        assert len(output) == 5
        assert output == "56789"

    def test_get_output_utf8_boundary(self, tmp_path: Path):
        runner = TerminalRunner("t1", "echo", output_byte_limit=5, project_root=tmp_path)
        # UTF-8 multibyte char (é = 0xC3 0xA9) at start after truncation
        runner._output = deque([b"abc\xc3\xa9xyz"])  # "abcéxyz" = 9 bytes
        runner._output_bytes_count = 9
        output, truncated = runner._get_output()
        assert truncated is True
        # Should skip the incomplete UTF-8 sequence
        assert "\ufffd" not in output or output.startswith("é") or output.startswith("x")

    def test_get_output_empty(self, tmp_path: Path):
        runner = TerminalRunner("t1", "echo", project_root=tmp_path)
        output, truncated = runner._get_output()
        assert output == ""
        assert truncated is False


class TestStateProperty:
    def test_state_returns_terminal_state(self, tmp_path: Path):
        runner = TerminalRunner("t1", "echo", project_root=tmp_path)
        runner._output = deque([b"test output"])
        runner._output_bytes_count = 11
        runner._return_code = 0
        state = runner.state
        assert isinstance(state, TerminalState)
        assert state.output == "test output"
        assert state.truncated is False
        assert state.return_code == 0


class TestKillAndRelease:
    def test_kill_no_process(self, tmp_path: Path):
        runner = TerminalRunner("t1", "echo", project_root=tmp_path)
        assert runner.kill() is False

    def test_kill_already_exited(self, tmp_path: Path, mocker: MockerFixture):
        runner = TerminalRunner("t1", "echo", project_root=tmp_path)
        runner._return_code = 0
        runner._process = mocker.MagicMock()
        assert runner.kill() is False

    def test_kill_running_process(self, tmp_path: Path, mocker: MockerFixture):
        runner = TerminalRunner("t1", "echo", project_root=tmp_path)
        mock_process = mocker.MagicMock()
        mock_task = mocker.MagicMock()
        mock_task.done.return_value = False
        runner._process = mock_process
        runner._task = mock_task
        assert runner.kill() is True
        mock_process.kill.assert_called_once()
        mock_task.cancel.assert_called_once()

    def test_kill_handles_oserror(self, tmp_path: Path, mocker: MockerFixture):
        runner = TerminalRunner("t1", "echo", project_root=tmp_path)
        mock_process = mocker.MagicMock()
        mock_process.kill.side_effect = OSError("No such process")
        runner._process = mock_process
        assert runner.kill() is False

    def test_release_sets_flag(self, tmp_path: Path):
        runner = TerminalRunner("t1", "echo", project_root=tmp_path)
        assert runner.released is False
        runner.release()
        assert runner.released

    def test_release_clears_output(self, tmp_path: Path):
        runner = TerminalRunner("t1", "echo", project_root=tmp_path)
        runner._output = deque([b"data"])
        runner._output_bytes_count = 4
        runner.release()
        assert len(runner._output) == 0
        assert runner._output_bytes_count == 0

    def test_release_cancels_task(self, tmp_path: Path, mocker: MockerFixture):
        runner = TerminalRunner("t1", "echo", project_root=tmp_path)
        mock_task = mocker.MagicMock()
        mock_task.done.return_value = False
        runner._task = mock_task
        runner.release()
        mock_task.cancel.assert_called_once()


class TestReturnCodeProperty:
    def test_return_code_none_initially(self, tmp_path: Path):
        runner = TerminalRunner("t1", "echo", project_root=tmp_path)
        assert runner.return_code is None

    def test_return_code_after_set(self, tmp_path: Path):
        runner = TerminalRunner("t1", "echo", project_root=tmp_path)
        runner._return_code = 42
        assert runner.return_code == 42
