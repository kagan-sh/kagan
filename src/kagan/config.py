"""Configuration loader for Kagan."""

from __future__ import annotations

import platform
import tomllib
from pathlib import Path
from typing import TYPE_CHECKING, Literal

from pydantic import BaseModel, Field

if TYPE_CHECKING:
    from collections.abc import Mapping

# OS detection for platform-specific commands
type OS = Literal["linux", "macos", "windows", "*"]

_OS_MAP = {"Linux": "linux", "Darwin": "macos", "Windows": "windows"}
CURRENT_OS: str = _OS_MAP.get(platform.system(), "linux")


def get_os_value[T](matrix: Mapping[str, T]) -> T | None:
    """Get OS-specific value with wildcard fallback.

    Args:
        matrix: Dict mapping OS names to values (e.g., {"macos": "cmd1", "*": "cmd2"})

    Returns:
        The value for the current OS, or the wildcard "*" value, or None.
    """
    return matrix.get(CURRENT_OS) or matrix.get("*")


class GeneralConfig(BaseModel):
    """General configuration settings."""

    max_concurrent_agents: int = Field(default=3)
    default_base_branch: str = Field(default="main")
    auto_start: bool = Field(default=False)
    max_iterations: int = Field(default=10)
    iteration_delay_seconds: float = Field(default=2.0)


class HatConfig(BaseModel):
    """Configuration for a hat (agent role)."""

    agent_command: str = Field(default="claude")
    args: list[str] = Field(default_factory=list)
    system_prompt: str = Field(default="")


class AgentConfig(BaseModel):
    """Configuration for an ACP agent."""

    identity: str = Field(..., description="Unique identifier (e.g., 'claude.com')")
    name: str = Field(..., description="Display name (e.g., 'Claude Code')")
    short_name: str = Field(..., description="CLI alias (e.g., 'claude')")
    protocol: Literal["acp"] = Field(default="acp", description="Protocol type")
    run_command: dict[str, str] = Field(
        default_factory=dict,
        description="OS-specific run commands (keys: 'linux', 'macos', 'windows', '*')",
    )
    active: bool = Field(default=True, description="Whether this agent is active")


class KaganConfig(BaseModel):
    """Root configuration model."""

    general: GeneralConfig = Field(default_factory=GeneralConfig)
    hats: dict[str, HatConfig] = Field(default_factory=dict)
    agents: dict[str, AgentConfig] = Field(default_factory=dict)

    @classmethod
    def load(cls, config_path: Path | None = None) -> KaganConfig:
        """Load configuration from TOML file or use defaults."""
        if config_path is None:
            config_path = Path(".kagan/config.toml")

        if config_path.exists():
            with open(config_path, "rb") as f:
                data = tomllib.load(f)
            return cls.model_validate(data)

        return cls()

    def get_hat(self, name: str) -> HatConfig | None:
        """Get hat configuration by name."""
        return self.hats.get(name)

    def get_default_hat(self) -> tuple[str, HatConfig] | None:
        """Get the first hat as default."""
        if self.hats:
            name = next(iter(self.hats))
            return name, self.hats[name]
        return None

    def get_agent(self, name: str) -> AgentConfig | None:
        """Get agent configuration by name."""
        return self.agents.get(name)

    def get_default_agent(self) -> tuple[str, AgentConfig] | None:
        """Get the first active agent as default."""
        for name, agent in self.agents.items():
            if agent.active:
                return name, agent
        return None


def load_config() -> KaganConfig:
    """Load configuration from default location."""
    return KaganConfig.load()
