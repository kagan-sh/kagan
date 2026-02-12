from __future__ import annotations

from unittest.mock import AsyncMock, Mock

import pytest

from kagan.core.adapters.process import (
    ProcessExecutionError,
    ProcessRetryPolicy,
    run_exec_capture,
    run_exec_checked,
    run_shell_checked,
)


class _ProcessStub:
    def __init__(self, *, returncode: int) -> None:
        self.returncode = returncode


class _TimeoutCleanupProcess:
    def __init__(self) -> None:
        self.returncode = None
        self.kill = Mock()
        self.communicate = AsyncMock(side_effect=[TimeoutError(), (b"", b"")])


async def test_when_exec_capture_times_out_and_retry_enabled_then_second_attempt_result_is_returned(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        "kagan.core.adapters.process.spawn_exec",
        AsyncMock(return_value=_ProcessStub(returncode=0)),
    )
    monkeypatch.setattr(
        "kagan.core.adapters.process._communicate",
        AsyncMock(side_effect=[TimeoutError(), (b"ok", b"")]),
    )

    result = await run_exec_capture(
        "git",
        "status",
        retry_policy=ProcessRetryPolicy(max_attempts=2, delay_seconds=0.0),
    )

    assert result.returncode == 0
    assert result.stdout == b"ok"


async def test_when_exec_checked_receives_nonzero_exit_then_structured_error_is_raised(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        "kagan.core.adapters.process.spawn_exec",
        AsyncMock(return_value=_ProcessStub(returncode=2)),
    )
    monkeypatch.setattr(
        "kagan.core.adapters.process._communicate",
        AsyncMock(return_value=(b"stdout", b"fatal: bad ref")),
    )

    with pytest.raises(ProcessExecutionError) as exc_info:
        await run_exec_checked("git", "rev-parse", "HEAD")

    exc = exc_info.value
    assert exc.code == "PROCESS_NONZERO_EXIT"
    assert exc.returncode == 2
    assert exc.command == ("git", "rev-parse", "HEAD")
    assert "fatal: bad ref" in str(exc)


async def test_when_shell_checked_times_out_then_timeout_error_contains_attempt_count(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        "kagan.core.adapters.process.spawn_shell",
        AsyncMock(return_value=_ProcessStub(returncode=1)),
    )
    monkeypatch.setattr(
        "kagan.core.adapters.process._communicate",
        AsyncMock(side_effect=TimeoutError),
    )

    with pytest.raises(ProcessExecutionError) as exc_info:
        await run_shell_checked(
            "npm i -g foo",
            retry_policy=ProcessRetryPolicy(max_attempts=2, delay_seconds=0.0),
        )

    exc = exc_info.value
    assert exc.code == "PROCESS_TIMEOUT"
    assert exc.timed_out is True
    assert exc.attempts == 2


async def test_when_exec_checked_spawn_raises_oserror_then_structured_error_is_raised(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        "kagan.core.adapters.process.spawn_exec",
        AsyncMock(side_effect=OSError("cannot execute")),
    )

    with pytest.raises(ProcessExecutionError) as exc_info:
        await run_exec_checked("git", "status", retry_policy=ProcessRetryPolicy(max_attempts=1))

    exc = exc_info.value
    assert exc.code == "PROCESS_OS_ERROR"
    assert exc.command == ("git", "status")
    assert "cannot execute" in str(exc)


async def test_when_exec_capture_times_out_without_retry_then_process_is_killed_and_drained(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    process = _TimeoutCleanupProcess()
    monkeypatch.setattr(
        "kagan.core.adapters.process.spawn_exec",
        AsyncMock(return_value=process),
    )

    with pytest.raises(TimeoutError):
        await run_exec_capture(
            "git",
            "status",
            timeout=0.01,
            retry_policy=ProcessRetryPolicy(max_attempts=1, delay_seconds=0.0),
        )

    process.kill.assert_called_once_with()
    assert process.communicate.await_count == 2


async def test_when_exec_capture_nonzero_and_retry_enabled_then_result_is_returned(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Non-zero exits should retry only when retry_on_nonzero is enabled."""
    spawn_exec = AsyncMock(
        side_effect=[
            _ProcessStub(returncode=128),
            _ProcessStub(returncode=0),
        ]
    )
    communicate = AsyncMock(side_effect=[(b"", b"fatal"), (b"ok", b"")])
    monkeypatch.setattr("kagan.core.adapters.process.spawn_exec", spawn_exec)
    monkeypatch.setattr("kagan.core.adapters.process._communicate", communicate)

    result = await run_exec_capture(
        "git",
        "status",
        retry_policy=ProcessRetryPolicy(
            max_attempts=2,
            delay_seconds=0.0,
            retry_on_nonzero=True,
        ),
    )

    assert result.returncode == 0
    assert result.stdout == b"ok"
    assert spawn_exec.await_count == 2
    assert communicate.await_count == 2


async def test_when_exec_capture_times_out_and_timeout_retry_disabled_then_only_one_attempt_is_made(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Timeout retries must not occur when retry_on_timeout is disabled."""
    spawn_exec = AsyncMock(return_value=_TimeoutCleanupProcess())
    monkeypatch.setattr("kagan.core.adapters.process.spawn_exec", spawn_exec)

    with pytest.raises(TimeoutError):
        await run_exec_capture(
            "git",
            "status",
            timeout=0.01,
            retry_policy=ProcessRetryPolicy(
                max_attempts=3,
                delay_seconds=0.0,
                retry_on_timeout=False,
            ),
        )

    assert spawn_exec.await_count == 1
