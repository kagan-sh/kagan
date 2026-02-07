"""Agent health checking service."""

from __future__ import annotations

import shutil
from typing import TYPE_CHECKING, Protocol

from kagan.core.models.enums import AgentStatus

if TYPE_CHECKING:
    from kagan.config import KaganConfig


class AgentHealthService(Protocol):
    """Protocol for agent health checking."""

    def check_status(self) -> AgentStatus: ...
    def get_status_message(self) -> str | None: ...
    def is_available(self) -> bool: ...
    def refresh(self) -> None: ...


class AgentHealthServiceImpl:
    """Check if configured agent CLI is available."""

    def __init__(self, config: KaganConfig) -> None:
        self._config = config
        self._status: AgentStatus = AgentStatus.AVAILABLE
        self._message: str | None = None
        self._check_agent()

    def _check_agent(self) -> None:
        """Check if agent CLI exists."""
        agent_name = self._config.general.default_worker_agent
        # Check common agent CLIs
        cli_names: dict[str, list[str]] = {
            "claude": ["claude"],
            "opencode": ["opencode"],
        }
        commands = cli_names.get(agent_name, [agent_name])

        for cmd in commands:
            if shutil.which(cmd):
                self._status = AgentStatus.AVAILABLE
                self._message = None
                return

        self._status = AgentStatus.UNAVAILABLE
        self._message = f"Agent CLI '{agent_name}' not found in PATH"

    def check_status(self) -> AgentStatus:
        """Return current agent status."""
        return self._status

    def get_status_message(self) -> str | None:
        """Return status message (if unavailable)."""
        return self._message

    def is_available(self) -> bool:
        """Check if agent is available."""
        return self._status == AgentStatus.AVAILABLE

    def refresh(self) -> None:
        """Re-check agent availability."""
        self._check_agent()
