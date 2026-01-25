"""Configuration loader for Kagan."""

import tomllib
from pathlib import Path

from pydantic import BaseModel, Field


class GeneralConfig(BaseModel):
    """General configuration settings."""

    max_concurrent_agents: int = Field(default=3)
    default_base_branch: str = Field(default="main")


class AgentConfig(BaseModel):
    """Configuration for an agent type."""

    model: str = Field(default="claude-3-5-sonnet")
    temperature: float = Field(default=0.7)
    cli_tool: str = Field(default="claude")
    run_on: str = Field(default="all")


class AgentsConfig(BaseModel):
    """Configuration for all agents."""

    planner: AgentConfig = Field(default_factory=AgentConfig)
    worker: AgentConfig = Field(default_factory=AgentConfig)
    overseer: AgentConfig = Field(default_factory=AgentConfig)


class KaganConfig(BaseModel):
    """Root configuration model."""

    general: GeneralConfig = Field(default_factory=GeneralConfig)
    agents: AgentsConfig = Field(default_factory=AgentsConfig)

    @classmethod
    def load(cls, config_path: Path | None = None) -> "KaganConfig":
        """Load configuration from TOML file or use defaults."""
        if config_path is None:
            config_path = Path(".kagan/config.toml")

        if config_path.exists():
            with open(config_path, "rb") as f:
                data = tomllib.load(f)
            return cls.model_validate(data)

        return cls()

    @classmethod
    def get_default_config_content(cls) -> str:
        """Get default configuration file content."""
        return """[general]
max_concurrent_agents = 3
default_base_branch = "main"

[agents.planner]
model = "claude-3-5-sonnet"
temperature = 0.7

[agents.worker]
model = "claude-3-5-sonnet"
cli_tool = "claude"  # or "aider", "opencode"

[agents.overseer]
model = "gpt-4o"
run_on = "high_priority"  # all | high_priority | manual
"""

    def save(self, config_path: Path | None = None) -> None:
        """Save configuration to TOML file."""
        if config_path is None:
            config_path = Path(".kagan/config.toml")

        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text(self.get_default_config_content())


def load_config() -> KaganConfig:
    """Load configuration from default location."""
    return KaganConfig.load()


def ensure_config_exists() -> KaganConfig:
    """Ensure config file exists, creating default if needed."""
    config_path = Path(".kagan/config.toml")
    if not config_path.exists():
        config = KaganConfig()
        config.save(config_path)
        return config
    return KaganConfig.load(config_path)
