"""Configuration loader for Kagan."""

from __future__ import annotations

import tomllib
from pathlib import Path

from pydantic import BaseModel, Field


class GeneralConfig(BaseModel):
    """General configuration settings."""

    max_concurrent_agents: int = Field(default=2)
    default_base_branch: str = Field(default="main")
    auto_start: bool = Field(default=False)
    max_iterations: int = Field(default=10)
    iteration_delay_seconds: float = Field(default=2.0)


class HatConfig(BaseModel):
    """Configuration for a hat (agent role)."""

    agent_command: str = Field(default="claude")
    args: list[str] = Field(default_factory=list)
    system_prompt: str = Field(default="")


class KaganConfig(BaseModel):
    """Root configuration model."""

    general: GeneralConfig = Field(default_factory=GeneralConfig)
    hats: dict[str, HatConfig] = Field(default_factory=dict)

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


def load_config() -> KaganConfig:
    """Load configuration from default location."""
    return KaganConfig.load()
