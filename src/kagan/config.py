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


class RefinementConfig(BaseModel):
    """Configuration for prompt refinement."""

    enabled: bool = Field(default=True, description="Enable prompt refinement feature")
    hotkey: str = Field(default="ctrl+e", description="Hotkey to trigger refinement")
    skip_length_under: int = Field(default=20, description="Skip refinement for short inputs")
    skip_prefixes: list[str] = Field(
        default_factory=lambda: ["/", "!", "?"],
        description="Prefixes that skip refinement (commands, quick questions)",
    )


class GeneralConfig(BaseModel):
    """General configuration settings."""

    max_concurrent_agents: int = Field(default=3)
    default_base_branch: str = Field(default="main")
    auto_start: bool = Field(default=False)
    auto_approve: bool = Field(default=False)
    auto_merge: bool = Field(default=False)
    max_iterations: int = Field(default=10)
    iteration_delay_seconds: float = Field(default=2.0)
    default_worker_agent: str = Field(default="claude")


class UIConfig(BaseModel):
    """UI-related user preferences."""

    skip_tmux_gateway: bool = Field(
        default=False,
        description="Skip tmux gateway info modal when opening PAIR sessions",
    )


class AgentConfig(BaseModel):
    """Configuration for an ACP agent."""

    identity: str = Field(..., description="Unique identifier (e.g., 'claude.com')")
    name: str = Field(..., description="Display name (e.g., 'Claude Code')")
    short_name: str = Field(..., description="CLI alias (e.g., 'claude')")
    protocol: Literal["acp"] = Field(default="acp", description="Protocol type")
    run_command: dict[str, str] = Field(
        default_factory=dict,
        description="OS-specific ACP commands for AUTO mode (e.g., 'npx claude-code-acp')",
    )
    interactive_command: dict[str, str] = Field(
        default_factory=dict,
        description="OS-specific CLI commands for PAIR mode (e.g., 'claude')",
    )
    active: bool = Field(default=True, description="Whether this agent is active")


class KaganConfig(BaseModel):
    """Root configuration model."""

    general: GeneralConfig = Field(default_factory=GeneralConfig)
    agents: dict[str, AgentConfig] = Field(default_factory=dict)
    refinement: RefinementConfig = Field(default_factory=RefinementConfig)
    ui: UIConfig = Field(default_factory=UIConfig)

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

    def get_agent(self, name: str) -> AgentConfig | None:
        """Get agent configuration by name."""
        return self.agents.get(name)

    def get_worker_agent(self) -> AgentConfig | None:
        """Get the configured worker agent."""
        return self.get_agent(self.general.default_worker_agent)


def get_fallback_agent_config() -> AgentConfig:
    """Get fallback agent config when none configured."""
    return AgentConfig(
        identity="claude.com",
        name="Claude Code",
        short_name="claude",
        run_command={"*": "npx claude-code-acp"},
        interactive_command={"*": "claude"},
    )
