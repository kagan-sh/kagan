"""Terminal management for agent communication."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual import log

from kagan import jsonrpc
from kagan.acp.terminal import TerminalRunner
from kagan.ansi import clean_terminal_output

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.acp import protocol


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
        env: list[protocol.EnvVariable] | None = None,
        output_byte_limit: int | None = None,
    ) -> tuple[str, str]:
        """Create a new terminal. Returns (terminal_id, display_command)."""
        self._count += 1
        terminal_id = f"terminal-{self._count}"
        cmd_display = command + (" " + " ".join(args) if args else "")
        log.info(f"[RPC] terminal/create: id={terminal_id}, cmd={cmd_display}")
        log.debug(f"[RPC] terminal/create: cwd={cwd}, env={env}")

        env_dict = {v["name"]: v["value"] for v in (env or [])}
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
                raise jsonrpc.JSONRPCError("Failed to start terminal")
            log.info(f"[RPC] terminal/create: terminal {terminal_id} started successfully")
        except jsonrpc.JSONRPCError:
            raise
        except Exception as e:
            log.error(f"[RPC] terminal/create: exception starting terminal: {e}")
            self._terminals.pop(terminal_id, None)
            raise jsonrpc.JSONRPCError(f"Failed to create terminal: {e}") from e

        return terminal_id, cmd_display

    def get_output(self, terminal_id: str) -> protocol.TerminalOutputResponse:
        terminal = self._terminals.get(terminal_id)
        if terminal is None:
            raise jsonrpc.JSONRPCError(f"No terminal with id {terminal_id!r}")

        state = terminal.state
        result: protocol.TerminalOutputResponse = {
            "output": state.output,
            "truncated": state.truncated,
        }
        if state.return_code is not None:
            result["exitStatus"] = {"exitCode": state.return_code}
        return result

    def kill(self, terminal_id: str) -> None:
        if terminal := self._terminals.get(terminal_id):
            terminal.kill()

    def release(self, terminal_id: str) -> None:
        if terminal := self._terminals.get(terminal_id):
            terminal.kill()
            terminal.release()

    async def wait_for_exit(self, terminal_id: str) -> tuple[int, str | None]:
        terminal = self._terminals.get(terminal_id)
        if terminal is None:
            raise jsonrpc.JSONRPCError(f"No terminal with id {terminal_id!r}")
        return await terminal.wait_for_exit()

    def get_final_output(self, terminal_id: str, limit: int = 500) -> str:
        """Get the last N chars of output for a terminal, cleaned for display."""
        terminal = self._terminals.get(terminal_id)
        if terminal is None:
            return ""
        raw_output = terminal.state.output[-limit:]
        return clean_terminal_output(raw_output)

    def cleanup_all(self) -> None:
        """Kill and release all terminals."""
        for terminal in list(self._terminals.values()):
            terminal.kill()
            terminal.release()
        self._terminals.clear()
