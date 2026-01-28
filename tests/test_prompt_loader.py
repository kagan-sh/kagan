"""Tests for the PromptLoader module."""

from __future__ import annotations

from pathlib import Path  # noqa: TC003

from kagan.agents.prompt_loader import PromptLoader, dump_default_prompts
from kagan.config import KaganConfig, PromptsConfig


class TestPromptsConfig:
    """Test PromptsConfig model."""

    def test_defaults(self) -> None:
        """Test default values."""
        config = PromptsConfig()
        assert config.planner_system_prompt == ""

    def test_custom_values(self) -> None:
        """Test custom values."""
        config = PromptsConfig(planner_system_prompt="planner prompt")
        assert config.planner_system_prompt == "planner prompt"


class TestPromptLoaderBuiltinDefaults:
    """Test PromptLoader with built-in defaults."""

    def test_get_planner_prompt_default(self, tmp_path: Path) -> None:
        """Test loading default planner prompt (preamble only)."""
        config = KaganConfig()
        loader = PromptLoader(config, prompts_dir=tmp_path / "prompts")
        prompt = loader.get_planner_prompt()
        # Planner prompt now returns preamble only, not the XML format
        assert "project planning assistant" in prompt
        assert "Guidelines" in prompt


class TestPromptLoaderTomlOverride:
    """Test PromptLoader with TOML inline config overrides."""

    def test_planner_prompt_from_toml(self, tmp_path: Path) -> None:
        """Test planner prompt from TOML config."""
        config = KaganConfig(prompts=PromptsConfig(planner_system_prompt="Custom planner prompt"))
        loader = PromptLoader(config, prompts_dir=tmp_path / "prompts")
        prompt = loader.get_planner_prompt()
        assert prompt == "Custom planner prompt"


class TestPromptLoaderFileOverride:
    """Test PromptLoader with file overrides."""

    def test_planner_prompt_from_file(self, tmp_path: Path) -> None:
        """Test planner prompt from file override."""
        prompts_dir = tmp_path / "prompts"
        prompts_dir.mkdir()
        (prompts_dir / "planner.md").write_text("File planner prompt")

        config = KaganConfig()
        loader = PromptLoader(config, prompts_dir=prompts_dir)
        prompt = loader.get_planner_prompt()
        assert prompt == "File planner prompt"

    def test_file_takes_priority_over_toml(self, tmp_path: Path) -> None:
        """Test file overrides take priority over TOML config."""
        prompts_dir = tmp_path / "prompts"
        prompts_dir.mkdir()
        (prompts_dir / "planner.md").write_text("File prompt")

        config = KaganConfig(prompts=PromptsConfig(planner_system_prompt="TOML prompt"))
        loader = PromptLoader(config, prompts_dir=prompts_dir)
        prompt = loader.get_planner_prompt()
        # File should take priority
        assert prompt == "File prompt"


class TestKaganConfigWithPrompts:
    """Test KaganConfig with prompts section."""

    def test_config_includes_prompts(self) -> None:
        """Test KaganConfig includes prompts field."""
        config = KaganConfig()
        assert hasattr(config, "prompts")
        assert isinstance(config.prompts, PromptsConfig)

    def test_config_prompts_defaults(self) -> None:
        """Test prompts have defaults."""
        config = KaganConfig()
        assert config.prompts.planner_system_prompt == ""


class TestDumpDefaultPrompts:
    """Test dump_default_prompts function."""

    def test_creates_directory_structure(self, tmp_path: Path) -> None:
        """Test creates prompts directory."""
        prompts_dir = tmp_path / "prompts"
        dump_default_prompts(prompts_dir)

        assert prompts_dir.exists()

    def test_creates_planner_template(self, tmp_path: Path) -> None:
        """Test creates planner.md with preamble (not output format)."""
        prompts_dir = tmp_path / "prompts"
        dump_default_prompts(prompts_dir)

        planner_file = prompts_dir / "planner.md"
        assert planner_file.exists()
        content = planner_file.read_text()
        # Planner.md contains preamble only, not the XML format
        assert "project planning assistant" in content
        assert "Guidelines" in content
        # Should NOT contain HTML comments (they confuse the AI)
        assert "<!--" not in content

    def test_idempotent(self, tmp_path: Path) -> None:
        """Test can be called multiple times without error."""
        prompts_dir = tmp_path / "prompts"
        dump_default_prompts(prompts_dir)
        dump_default_prompts(prompts_dir)  # Second call should not raise

        assert (prompts_dir / "planner.md").exists()
