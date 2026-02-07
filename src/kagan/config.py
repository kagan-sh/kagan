"""Configuration loader for Kagan."""

from __future__ import annotations

import asyncio
import platform
import tomllib
from typing import TYPE_CHECKING, Literal

import tomlkit
from pydantic import BaseModel, Field

from kagan.atomic import atomic_write
from kagan.paths import ensure_directories, get_config_path

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

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
    hotkey: str = Field(default="f2", description="Hotkey to trigger refinement")
    skip_length_under: int = Field(default=20, description="Skip refinement for short inputs")
    skip_prefixes: list[str] = Field(
        default_factory=lambda: ["/", "!", "?"],
        description="Prefixes that skip refinement (commands, quick questions)",
    )


class GeneralConfig(BaseModel):
    """General configuration settings."""

    max_concurrent_agents: int = Field(default=1)
    default_base_branch: str = Field(default="main")
    auto_start: bool = Field(default=False)
    auto_approve: bool = Field(default=False)
    auto_merge: bool = Field(default=False)
    auto_retry_on_merge_conflict: bool = Field(
        default=True,
        description="When auto_merge is enabled and fails due to conflict, "
        "rebase and retry automatically",
    )
    require_review_approval: bool = Field(
        default=False, description="Require approved review before merge actions"
    )
    serialize_merges: bool = Field(
        default=False, description="Serialize manual merges to reduce conflicts"
    )
    max_iterations: int = Field(default=10)
    iteration_delay_seconds: float = Field(default=2.0)
    default_worker_agent: str = Field(default="claude")
    default_model_claude: str | None = Field(
        default=None, description="Default Claude model alias or full name (None = agent default)"
    )
    default_model_opencode: str | None = Field(
        default=None, description="Default OpenCode model (None = agent default)"
    )


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
    model_env_var: str = Field(default="", description="Environment variable for model selection")


class KaganConfig(BaseModel):
    """Root configuration model."""

    general: GeneralConfig = Field(default_factory=GeneralConfig)
    agents: dict[str, AgentConfig] = Field(default_factory=dict)
    refinement: RefinementConfig = Field(default_factory=RefinementConfig)
    ui: UIConfig = Field(default_factory=UIConfig)

    @classmethod
    def load(cls, config_path: Path | None = None) -> KaganConfig:
        """Load configuration from TOML file or use defaults."""
        ensure_directories()
        if config_path is None:
            config_path = get_config_path()

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

    async def save(self, path: Path) -> None:
        """Serialize current config to TOML file.

        Args:
            path: Path to write config file (created if missing)
        """
        doc = tomlkit.document()

        # General section
        general_table = tomlkit.table()
        for key, value in self.general.model_dump().items():
            if value is not None:
                general_table[key] = value
        doc["general"] = general_table

        # Agents section (if any)
        if self.agents:
            agents_table = tomlkit.table()
            for agent_name, agent_cfg in self.agents.items():
                agent_table = tomlkit.table()
                for key, value in agent_cfg.model_dump().items():
                    if value is not None and value != {}:
                        agent_table[key] = value
                agents_table[agent_name] = agent_table
            doc["agents"] = agents_table

        # Refinement section
        refinement_table = tomlkit.table()
        for key, value in self.refinement.model_dump().items():
            if value is not None:
                refinement_table[key] = value
        doc["refinement"] = refinement_table

        # UI section
        ui_table = tomlkit.table()
        for key, value in self.ui.model_dump().items():
            if value is not None:
                ui_table[key] = value
        doc["ui"] = ui_table

        content = tomlkit.dumps(doc)
        await asyncio.to_thread(atomic_write, path, content)

    async def update_ui_preferences(
        self,
        path: Path,
        *,
        skip_tmux_gateway: bool | None = None,
    ) -> None:
        """Update UI preferences in existing TOML file (preserves comments).

        Args:
            path: Path to config file (created if missing)
            skip_tmux_gateway: Value for skip_tmux_gateway (None = no change)
        """
        import aiofiles

        # Load existing TOML or create minimal structure
        if path.exists():
            async with aiofiles.open(path, encoding="utf-8") as f:
                content = await f.read()
            doc = tomlkit.parse(content)
        else:
            doc = tomlkit.document()
            doc["general"] = tomlkit.table()
            doc["general"]["auto_start"] = False  # type: ignore[index]

        # Ensure [ui] section exists
        if "ui" not in doc:
            doc["ui"] = tomlkit.table()

        # Update preferences
        if skip_tmux_gateway is not None:
            # Type checker workaround: cast to dict-like for assignment
            doc["ui"]["skip_tmux_gateway"] = skip_tmux_gateway  # type: ignore[index]

        # Write back
        content = tomlkit.dumps(doc)
        await asyncio.to_thread(atomic_write, path, content)


def get_fallback_agent_config() -> AgentConfig:
    """Get fallback agent config when none configured."""
    return AgentConfig(
        identity="claude.com",
        name="Claude Code",
        short_name="claude",
        run_command={"*": "npx claude-code-acp"},
        interactive_command={"*": "claude"},
        model_env_var="ANTHROPIC_MODEL",
    )
