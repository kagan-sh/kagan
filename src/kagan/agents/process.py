"""Agent process management via PTY."""

from __future__ import annotations

import asyncio
import fcntl
import os
import pty
from collections import deque
from enum import Enum
from typing import TYPE_CHECKING

from kagan.agents.shell_read import shell_read

if TYPE_CHECKING:
    from pathlib import Path

OUTPUT_LIMIT = 500_000  # 500KB output limit


class AgentState(str, Enum):
    IDLE = "idle"
    RUNNING = "running"
    FINISHED = "finished"
    FAILED = "failed"


class AgentProcess:
    """Manages a single CLI agent subprocess via PTY."""

    def __init__(self, ticket_id: str) -> None:
        self.ticket_id = ticket_id
        self.state = AgentState.IDLE
        self.return_code: int | None = None
        self._process: asyncio.subprocess.Process | None = None
        self._master_fd: int | None = None
        self._pid: int | None = None
        self._output: deque[bytes] = deque()
        self._output_bytes: int = 0
        self._task: asyncio.Task[None] | None = None
        self._ready_event = asyncio.Event()
        self._exit_event = asyncio.Event()

    async def start(self, command: str, cwd: Path, env: dict[str, str] | None = None) -> None:
        """Start the agent process in a PTY."""
        master, slave = pty.openpty()
        self._master_fd = master

        flags = fcntl.fcntl(master, fcntl.F_GETFL)
        fcntl.fcntl(master, fcntl.F_SETFL, flags | os.O_NONBLOCK)

        process_env = os.environ.copy()
        process_env["TERM"] = "xterm-256color"
        process_env["FORCE_COLOR"] = "1"
        process_env["COLORTERM"] = "truecolor"
        if env:
            process_env.update(env)

        self._process = await asyncio.create_subprocess_shell(
            command,
            stdin=slave,
            stdout=slave,
            stderr=slave,
            env=process_env,
            cwd=str(cwd),
        )
        self._pid = self._process.pid
        os.close(slave)

        self.state = AgentState.RUNNING
        self._task = asyncio.create_task(self._read_loop())
        self._ready_event.set()

    async def _read_loop(self) -> None:
        """Read output from PTY until process exits."""
        assert self._master_fd is not None
        buffer_size = 64 * 1024
        reader = asyncio.StreamReader(buffer_size)
        protocol = asyncio.StreamReaderProtocol(reader)
        loop = asyncio.get_event_loop()

        transport, _ = await loop.connect_read_pipe(
            lambda: protocol, os.fdopen(self._master_fd, "rb", 0)
        )

        try:
            while True:
                data = await shell_read(reader, buffer_size)
                if data:
                    self._record_output(data)
                if not data:
                    break
        finally:
            transport.close()

        if self._process:
            self.return_code = await self._process.wait()
            self.state = AgentState.FINISHED if self.return_code == 0 else AgentState.FAILED

        self._exit_event.set()

    def _record_output(self, data: bytes) -> None:
        """Record output with size limit."""
        self._output.append(data)
        self._output_bytes += len(data)
        while self._output_bytes > OUTPUT_LIMIT and self._output:
            oldest = self._output.popleft()
            self._output_bytes -= len(oldest)

    async def _write(self, data: bytes) -> int:
        """Write to the PTY."""
        if self._master_fd is None:
            return 0
        try:
            return await asyncio.to_thread(os.write, self._master_fd, data)
        except OSError:
            return 0

    async def send_input(self, text: str) -> None:
        """Send text input to the agent."""
        await self._write(text.encode("utf-8"))

    async def interrupt(self) -> None:
        """Send SIGINT (Ctrl+C) to the agent."""
        await self._write(b"\x03")

    async def terminate(self) -> None:
        """Gracefully terminate, then kill if needed."""
        if self._process:
            self._process.terminate()
            try:
                await asyncio.wait_for(self._exit_event.wait(), timeout=5.0)
            except TimeoutError:
                self._process.kill()
                await self._exit_event.wait()
            self.state = AgentState.FINISHED

    def _is_busy(self) -> bool:
        """Check if agent has running child processes (sync)."""
        if self._pid is None:
            return False
        import psutil

        try:
            proc = psutil.Process(self._pid)
            return bool(proc.children(recursive=True))
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            return False

    async def is_busy(self) -> bool:
        """Check if agent has running child processes."""
        return await asyncio.to_thread(self._is_busy)

    def get_output(self, limit: int = 50000) -> tuple[str, bool]:
        """Get captured output (output, was_truncated)."""
        output_bytes = b"".join(self._output)
        truncated = len(output_bytes) > limit
        if truncated:
            output_bytes = output_bytes[-limit:]
            # Ensure we start on UTF-8 boundary
            for offset, byte_val in enumerate(output_bytes):
                if (byte_val & 0b11000000) != 0b10000000:
                    output_bytes = output_bytes[offset:]
                    break
        return output_bytes.decode("utf-8", "replace"), truncated

    async def wait_for_exit(self) -> int | None:
        """Wait for process to exit, return exit code."""
        await self._exit_event.wait()
        return self.return_code
