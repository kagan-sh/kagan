"""Shared subprocess adapter for core process execution."""

from __future__ import annotations

import asyncio
import contextlib
import os
import subprocess
from dataclasses import dataclass
from typing import TYPE_CHECKING

from kagan.core.instrumentation import increment_counter, timed_operation

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence
    from pathlib import Path


@dataclass(frozen=True)
class ProcessRetryPolicy:
    """Retry behavior for subprocess execution."""

    max_attempts: int = 1
    delay_seconds: float = 0.0
    retry_on_timeout: bool = True
    retry_on_nonzero: bool = False
    retry_on_oserror: bool = True

    def normalized(self) -> ProcessRetryPolicy:
        attempts = max(1, self.max_attempts)
        delay = max(0.0, self.delay_seconds)
        return ProcessRetryPolicy(
            max_attempts=attempts,
            delay_seconds=delay,
            retry_on_timeout=self.retry_on_timeout,
            retry_on_nonzero=self.retry_on_nonzero,
            retry_on_oserror=self.retry_on_oserror,
        )


@dataclass(frozen=True)
class ProcessResult:
    """Captured result of a subprocess execution."""

    returncode: int
    stdout: bytes
    stderr: bytes

    def stdout_text(self) -> str:
        """Decode stdout as UTF-8 with replacement."""
        return self.stdout.decode("utf-8", errors="replace")

    def stderr_text(self) -> str:
        """Decode stderr as UTF-8 with replacement."""
        return self.stderr.decode("utf-8", errors="replace")


@dataclass(frozen=True)
class ProcessExecutionError(RuntimeError):
    """Structured process failure with machine-readable code and command context."""

    code: str
    command: tuple[str, ...]
    returncode: int | None = None
    timed_out: bool = False
    attempts: int = 1
    stdout: str | None = None
    stderr: str | None = None
    detail: str | None = None

    def __str__(self) -> str:
        command_text = " ".join(self.command)
        parts = [f"[{self.code}] {command_text}"]
        if self.returncode is not None:
            parts.append(f"(rc={self.returncode})")
        if self.timed_out:
            parts.append("(timed out)")
        if self.attempts > 1:
            parts.append(f"after {self.attempts} attempts")

        message = " ".join(parts)
        detail = self.detail or self.stderr or self.stdout
        if detail:
            return f"{message}: {detail}"
        return message


def _normalize_cwd(cwd: str | Path | None) -> str | None:
    if cwd is None:
        return None
    return str(cwd)


def _normalize_env(env: Mapping[str, str] | None) -> dict[str, str] | None:
    if env is None:
        return None
    return dict(env)


async def spawn_exec(
    executable: str,
    *args: str,
    cwd: str | Path | None = None,
    env: Mapping[str, str] | None = None,
    stdin: int | None = None,
    stdout: int | None = None,
    stderr: int | None = None,
) -> asyncio.subprocess.Process:
    """Spawn a subprocess using ``create_subprocess_exec``."""
    return await asyncio.create_subprocess_exec(
        executable,
        *args,
        cwd=_normalize_cwd(cwd),
        env=_normalize_env(env),
        stdin=stdin,
        stdout=stdout,
        stderr=stderr,
    )


async def spawn_shell(
    command: str,
    *,
    cwd: str | Path | None = None,
    env: Mapping[str, str] | None = None,
    stdin: int | None = None,
    stdout: int | None = None,
    stderr: int | None = None,
) -> asyncio.subprocess.Process:
    """Spawn a subprocess using ``create_subprocess_shell``."""
    return await asyncio.create_subprocess_shell(
        command,
        cwd=_normalize_cwd(cwd),
        env=_normalize_env(env),
        stdin=stdin,
        stdout=stdout,
        stderr=stderr,
    )


async def _communicate(
    process: asyncio.subprocess.Process,
    *,
    timeout: float | None = None,
) -> tuple[bytes, bytes]:
    try:
        if timeout is None:
            stdout, stderr = await process.communicate()
        else:
            stdout, stderr = await asyncio.wait_for(process.communicate(), timeout=timeout)
    except TimeoutError:
        with contextlib.suppress(ProcessLookupError):
            process.kill()
        with contextlib.suppress(ProcessLookupError):
            await process.communicate()
        raise

    return stdout or b"", stderr or b""


def _command_fields(
    *,
    mode: str,
    executable: str | None = None,
) -> dict[str, object]:
    fields: dict[str, object] = {"mode": mode}
    if executable is not None:
        fields["command"] = executable
    return fields


def _retry_counter_fields(
    *,
    mode: str,
    reason: str,
    executable: str | None = None,
) -> dict[str, object]:
    fields = _command_fields(mode=mode, executable=executable)
    fields["reason"] = reason
    return fields


async def _maybe_retry_wait(delay_seconds: float) -> None:
    if delay_seconds > 0:
        await asyncio.sleep(delay_seconds)


async def run_exec_capture(
    executable: str,
    *args: str,
    cwd: str | Path | None = None,
    env: Mapping[str, str] | None = None,
    timeout: float | None = None,
    retry_policy: ProcessRetryPolicy | None = None,
) -> ProcessResult:
    """Run an exec subprocess and capture stdout/stderr."""
    policy = (retry_policy or ProcessRetryPolicy()).normalized()
    fields = _command_fields(mode="exec", executable=executable)
    increment_counter("core.process.exec.calls", fields=fields)
    with timed_operation("core.process.exec.duration_ms", fields=fields):
        attempt = 1
        while True:
            try:
                process = await spawn_exec(
                    executable,
                    *args,
                    cwd=cwd,
                    env=env,
                    stdout=asyncio.subprocess.PIPE,
                    stderr=asyncio.subprocess.PIPE,
                )
            except OSError:
                if policy.retry_on_oserror and attempt < policy.max_attempts:
                    increment_counter(
                        "core.process.exec.retries",
                        fields=_retry_counter_fields(
                            mode="exec",
                            reason="oserror",
                            executable=executable,
                        ),
                    )
                    attempt += 1
                    await _maybe_retry_wait(policy.delay_seconds)
                    continue
                raise

            try:
                stdout, stderr = await _communicate(process, timeout=timeout)
            except TimeoutError:
                if policy.retry_on_timeout and attempt < policy.max_attempts:
                    increment_counter(
                        "core.process.exec.retries",
                        fields=_retry_counter_fields(
                            mode="exec",
                            reason="timeout",
                            executable=executable,
                        ),
                    )
                    attempt += 1
                    await _maybe_retry_wait(policy.delay_seconds)
                    continue
                increment_counter("core.process.exec.timeouts", fields=fields)
                raise

            result = ProcessResult(
                returncode=process.returncode if process.returncode is not None else 1,
                stdout=stdout,
                stderr=stderr,
            )
            if result.returncode != 0 and policy.retry_on_nonzero and attempt < policy.max_attempts:
                increment_counter(
                    "core.process.exec.retries",
                    fields=_retry_counter_fields(
                        mode="exec",
                        reason="nonzero",
                        executable=executable,
                    ),
                )
                attempt += 1
                await _maybe_retry_wait(policy.delay_seconds)
                continue
            break

    if result.returncode != 0:
        error_fields = dict(fields)
        error_fields["returncode"] = result.returncode
        increment_counter("core.process.exec.nonzero_returncode", fields=error_fields)
    return result


async def run_shell_capture(
    command: str,
    *,
    cwd: str | Path | None = None,
    env: Mapping[str, str] | None = None,
    timeout: float | None = None,
    retry_policy: ProcessRetryPolicy | None = None,
) -> ProcessResult:
    """Run a shell subprocess and capture stdout/stderr."""
    policy = (retry_policy or ProcessRetryPolicy()).normalized()
    fields = _command_fields(mode="shell")
    increment_counter("core.process.exec.calls", fields=fields)
    with timed_operation("core.process.exec.duration_ms", fields=fields):
        attempt = 1
        while True:
            try:
                process = await spawn_shell(
                    command,
                    cwd=cwd,
                    env=env,
                    stdout=asyncio.subprocess.PIPE,
                    stderr=asyncio.subprocess.PIPE,
                )
            except OSError:
                if policy.retry_on_oserror and attempt < policy.max_attempts:
                    increment_counter(
                        "core.process.exec.retries",
                        fields=_retry_counter_fields(mode="shell", reason="oserror"),
                    )
                    attempt += 1
                    await _maybe_retry_wait(policy.delay_seconds)
                    continue
                raise

            try:
                stdout, stderr = await _communicate(process, timeout=timeout)
            except TimeoutError:
                if policy.retry_on_timeout and attempt < policy.max_attempts:
                    increment_counter(
                        "core.process.exec.retries",
                        fields=_retry_counter_fields(mode="shell", reason="timeout"),
                    )
                    attempt += 1
                    await _maybe_retry_wait(policy.delay_seconds)
                    continue
                increment_counter("core.process.exec.timeouts", fields=fields)
                raise

            result = ProcessResult(
                returncode=process.returncode if process.returncode is not None else 1,
                stdout=stdout,
                stderr=stderr,
            )
            if result.returncode != 0 and policy.retry_on_nonzero and attempt < policy.max_attempts:
                increment_counter(
                    "core.process.exec.retries",
                    fields=_retry_counter_fields(mode="shell", reason="nonzero"),
                )
                attempt += 1
                await _maybe_retry_wait(policy.delay_seconds)
                continue
            break

    if result.returncode != 0:
        error_fields = dict(fields)
        error_fields["returncode"] = result.returncode
        increment_counter("core.process.exec.nonzero_returncode", fields=error_fields)
    return result


def _build_nonzero_error(
    *,
    command: tuple[str, ...],
    result: ProcessResult,
    attempts: int,
) -> ProcessExecutionError:
    stderr_text = result.stderr_text().strip()
    stdout_text = result.stdout_text().strip()
    detail = stderr_text or stdout_text or "process exited with a non-zero status"
    return ProcessExecutionError(
        code="PROCESS_NONZERO_EXIT",
        command=command,
        returncode=result.returncode,
        attempts=attempts,
        stdout=stdout_text or None,
        stderr=stderr_text or None,
        detail=detail,
    )


def _build_timeout_error(
    *,
    command: tuple[str, ...],
    attempts: int,
) -> ProcessExecutionError:
    return ProcessExecutionError(
        code="PROCESS_TIMEOUT",
        command=command,
        timed_out=True,
        attempts=attempts,
        detail="process execution exceeded timeout",
    )


def _build_oserror(
    *,
    command: tuple[str, ...],
    attempts: int,
    exc: OSError,
) -> ProcessExecutionError:
    return ProcessExecutionError(
        code="PROCESS_OS_ERROR",
        command=command,
        attempts=attempts,
        detail=str(exc),
    )


async def run_exec_checked(
    executable: str,
    *args: str,
    cwd: str | Path | None = None,
    env: Mapping[str, str] | None = None,
    timeout: float | None = None,
    retry_policy: ProcessRetryPolicy | None = None,
) -> ProcessResult:
    """Run exec subprocess and raise a structured error when execution fails."""
    policy = (retry_policy or ProcessRetryPolicy()).normalized()
    command = (executable, *args)
    try:
        result = await run_exec_capture(
            executable,
            *args,
            cwd=cwd,
            env=env,
            timeout=timeout,
            retry_policy=policy,
        )
    except TimeoutError as exc:
        raise _build_timeout_error(command=command, attempts=policy.max_attempts) from exc
    except OSError as exc:
        raise _build_oserror(command=command, attempts=policy.max_attempts, exc=exc) from exc

    if result.returncode != 0:
        raise _build_nonzero_error(command=command, result=result, attempts=policy.max_attempts)

    return result


async def run_shell_checked(
    command: str,
    *,
    cwd: str | Path | None = None,
    env: Mapping[str, str] | None = None,
    timeout: float | None = None,
    retry_policy: ProcessRetryPolicy | None = None,
) -> ProcessResult:
    """Run shell subprocess and raise a structured error when execution fails."""
    policy = (retry_policy or ProcessRetryPolicy()).normalized()
    command_tuple = (command,)
    try:
        result = await run_shell_capture(
            command,
            cwd=cwd,
            env=env,
            timeout=timeout,
            retry_policy=policy,
        )
    except TimeoutError as exc:
        raise _build_timeout_error(command=command_tuple, attempts=policy.max_attempts) from exc
    except OSError as exc:
        raise _build_oserror(command=command_tuple, attempts=policy.max_attempts, exc=exc) from exc

    if result.returncode != 0:
        raise _build_nonzero_error(
            command=command_tuple,
            result=result,
            attempts=policy.max_attempts,
        )

    return result


def spawn_detached(
    command: Sequence[str],
    *,
    cwd: str | Path | None = None,
    env: Mapping[str, str] | None = None,
    windows_creationflags: int = 0,
) -> subprocess.Popen[bytes]:
    """Spawn a detached subprocess for background daemon-style processes."""
    if os.name == "nt":
        return subprocess.Popen(
            list(command),
            cwd=_normalize_cwd(cwd),
            env=_normalize_env(env),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            close_fds=True,
            creationflags=windows_creationflags,
        )
    return subprocess.Popen(
        list(command),
        cwd=_normalize_cwd(cwd),
        env=_normalize_env(env),
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        close_fds=True,
        start_new_session=True,
    )


__all__ = [
    "ProcessExecutionError",
    "ProcessResult",
    "ProcessRetryPolicy",
    "run_exec_capture",
    "run_exec_checked",
    "run_shell_capture",
    "run_shell_checked",
    "spawn_detached",
    "spawn_exec",
    "spawn_shell",
]
