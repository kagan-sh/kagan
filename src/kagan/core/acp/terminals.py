"""Terminal management for agent communication."""

from __future__ import annotations

import re
from typing import TYPE_CHECKING

from acp import RequestError

from kagan.core.acp.terminal import TerminalRunner
from kagan.core.debug_log import log

_ANSI_ESCAPE = re.compile(r"\x1B(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*\x07|[@-Z\\^_-])")


def strip_ansi(text: str) -> str:
    """Remove all ANSI escape sequences from text."""
    if not text:
        return ""
    return _ANSI_ESCAPE.sub("", text)


if TYPE_CHECKING:
    from pathlib import Path

    from acp.schema import EnvVariable, TerminalOutputResponse


class TerminalManager:
    """Manages terminal instances for an agent."""

    def __init__(self, project_root: Path) -> None:
        self.project_root = project_root
        self._count: int = 0
        self._terminals: dict[str, TerminalRunner] = {}

    def get(self, terminal_id: str) -> TerminalRunner | None:
        return self._terminals.get(terminal_id)

    async def create(
        self,
        command: str,
        args: list[str] | None = None,
        cwd: str | None = None,
        env: list[EnvVariable] | None = None,
        output_byte_limit: int | None = None,
    ) -> tuple[str, str]:
        """Create a new terminal. Returns (terminal_id, display_command)."""
        self._count += 1
        terminal_id = f"terminal-{self._count}"
        cmd_display = command + (" " + " ".join(args) if args else "")
        log.info(f"[RPC] terminal/create: id={terminal_id}, cmd={cmd_display}")
        log.debug(f"[RPC] terminal/create: cwd={cwd}, env={env}")

        env_dict = {v.name: v.value for v in (env or [])}
        terminal = TerminalRunner(
            terminal_id=terminal_id,
            command=command,
            args=args,
            cwd=cwd,
            env=env_dict,
            output_byte_limit=output_byte_limit,
            project_root=self.project_root,
        )
        self._terminals[terminal_id] = terminal

        try:
            success = await terminal.start()
            if not success:
                log.error(f"[RPC] terminal/create: failed to start terminal {terminal_id}")
                del self._terminals[terminal_id]
                raise RequestError.internal_error({"details": "Failed to start terminal"})
            log.info(f"[RPC] terminal/create: terminal {terminal_id} started successfully")
        except Exception as e:
            log.error(f"[RPC] terminal/create: exception starting terminal: {e}")
            self._terminals.pop(terminal_id, None)
            raise RequestError.internal_error({"details": f"Failed to create terminal: {e}"}) from e

        return terminal_id, cmd_display

    def get_output(self, terminal_id: str) -> TerminalOutputResponse:
        terminal = self._terminals.get(terminal_id)
        if terminal is None:
            raise RequestError.invalid_params({"details": f"No terminal with id {terminal_id!r}"})

        state = terminal.state
        exit_status = None
        if state.return_code is not None:
            from acp.schema import TerminalExitStatus

            exit_status = TerminalExitStatus(exit_code=state.return_code)
        from acp.schema import TerminalOutputResponse

        return TerminalOutputResponse(
            output=state.output,
            truncated=state.truncated,
            exit_status=exit_status,
        )

    def kill(self, terminal_id: str) -> None:
        if terminal := self._terminals.get(terminal_id):
            terminal.kill()

    def release(self, terminal_id: str) -> None:
        if terminal := self._terminals.get(terminal_id):
            terminal.kill()
            terminal.release()
            del self._terminals[terminal_id]

    async def wait_for_exit(self, terminal_id: str) -> tuple[int, str | None]:
        terminal = self._terminals.get(terminal_id)
        if terminal is None:
            raise RequestError.invalid_params({"details": f"No terminal with id {terminal_id!r}"})
        return await terminal.wait_for_exit()

    def get_final_output(self, terminal_id: str, limit: int = 500) -> str:
        """Get the last N chars of output for a terminal, cleaned for display."""
        terminal = self._terminals.get(terminal_id)
        if terminal is None:
            return ""
        raw_output = terminal.state.output[-limit:]
        return strip_ansi(raw_output)

    def cleanup_all(self) -> None:
        """Kill and release all terminals."""
        for terminal in list(self._terminals.values()):
            terminal.kill()
            terminal.release()
        self._terminals.clear()
