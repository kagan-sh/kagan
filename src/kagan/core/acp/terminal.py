"""Terminal runner for ACP agent commands."""

from __future__ import annotations

import asyncio
import os
from collections import deque
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from kagan.core.adapters.process import spawn_shell
from kagan.core.command_utils import format_command_for_shell

if TYPE_CHECKING:
    from collections.abc import Mapping


@dataclass
class TerminalState:
    """Current state of a terminal."""

    output: str
    truncated: bool
    return_code: int | None = None
    signal: str | None = None


class TerminalRunner:
    """Runs terminal commands for ACP agents."""

    def __init__(
        self,
        terminal_id: str,
        command: str,
        args: list[str] | None = None,
        cwd: str | None = None,
        env: Mapping[str, str] | None = None,
        output_byte_limit: int | None = None,
        project_root: Path | None = None,
    ) -> None:
        """Initialize the terminal runner.

        Args:
            terminal_id: Unique identifier for this terminal.
            command: Command to execute.
            args: Command arguments.
            cwd: Working directory (relative to project_root or absolute).
            env: Additional environment variables.
            output_byte_limit: Maximum output bytes to retain.
            project_root: Project root path for relative cwd.
        """
        self.terminal_id = terminal_id
        self.command = command
        self.args = args or []
        self.cwd = cwd
        self.env = dict(env) if env else {}
        self.output_byte_limit = output_byte_limit
        self.project_root = project_root or Path.cwd()

        self._process: asyncio.subprocess.Process | None = None
        self._output: deque[bytes] = deque()
        self._output_bytes_count = 0
        self._return_code: int | None = None
        self._released = False
        self._exit_event = asyncio.Event()
        self._task: asyncio.Task[None] | None = None

    @property
    def return_code(self) -> int | None:
        """The command return code, or None if not yet set."""
        return self._return_code

    @property
    def released(self) -> bool:
        """Has the terminal been released?"""
        return self._released

    @property
    def state(self) -> TerminalState:
        """Get the current terminal state."""
        output, truncated = self._get_output()
        return TerminalState(
            output=output,
            truncated=truncated,
            return_code=self.return_code,
        )

    async def start(self) -> bool:
        """Start the terminal process.

        Returns:
            True if process started successfully, False otherwise.
        """
        try:
            self._task = asyncio.create_task(self._run())

            await asyncio.sleep(0.1)
            return self._process is not None
        except OSError:
            return False

    async def _run(self) -> None:
        """Run the command and capture output."""
        if self.args:
            full_command = format_command_for_shell(self.command, self.args)
        else:
            full_command = self.command

        if self.cwd:
            work_dir = self.cwd if os.path.isabs(self.cwd) else str(self.project_root / self.cwd)
        else:
            work_dir = str(self.project_root)

        environment = os.environ.copy()
        environment.update(self.env)

        try:
            self._process = await spawn_shell(
                full_command,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.STDOUT,
                env=environment,
                cwd=work_dir,
            )
        except OSError:
            self._return_code = 1
            self._exit_event.set()
            return

        assert self._process.stdout is not None

        try:
            while True:
                data = await self._process.stdout.read(8192)
                if not data:
                    break
                self._record_output(data)
        except (OSError, asyncio.CancelledError):
            pass

        self._return_code = await self._process.wait()
        self._exit_event.set()

    def _record_output(self, data: bytes) -> None:
        """Record output bytes, respecting the limit."""
        self._output.append(data)
        self._output_bytes_count += len(data)

        if self.output_byte_limit is None:
            return

        while self._output_bytes_count > self.output_byte_limit and self._output:
            oldest = self._output[0]
            if self._output_bytes_count - len(oldest) < self.output_byte_limit:
                break
            self._output.popleft()
            self._output_bytes_count -= len(oldest)

    def _get_output(self) -> tuple[str, bool]:
        """Get the output as a string.

        Returns:
            Tuple of (output_text, was_truncated).
        """
        output_bytes = b"".join(self._output)
        truncated = False

        if self.output_byte_limit is not None and len(output_bytes) > self.output_byte_limit:
            truncated = True
            output_bytes = output_bytes[-self.output_byte_limit :]

            # Trim to a UTF-8 boundary after slicing raw bytes.
            for offset, byte_val in enumerate(output_bytes):
                if (byte_val & 0b11000000) != 0b10000000:
                    if offset:
                        output_bytes = output_bytes[offset:]
                    break

        return output_bytes.decode("utf-8", "replace"), truncated

    def kill(self) -> bool:
        """Kill the terminal process and cancel the read task.

        Returns:
            True if killed, False if no process.
        """
        if self._return_code is not None:
            return False
        if self._process is None:
            return False
        try:
            self._process.kill()

            if self._task is not None and not self._task.done():
                self._task.cancel()
            return True
        except (OSError, ProcessLookupError):
            return False

    def release(self) -> None:
        """Release the terminal (no longer usable via ACP)."""
        self._released = True

        if self._task is not None and not self._task.done():
            self._task.cancel()

        self._output.clear()
        self._output_bytes_count = 0

    async def wait_for_exit(self) -> tuple[int, str | None]:
        """Wait for the process to exit.

        Returns:
            Tuple of (return_code, signal).
        """
        await self._exit_event.wait()
        return (self._return_code or 0, None)
