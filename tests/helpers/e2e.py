"""E2E test utilities."""

from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path


def create_kagan_config_content(
    auto_start: bool = False,
    auto_merge: bool = False,
    base_branch: str = "main",
) -> str:
    """Generate .kagan/config.toml content for E2E tests."""
    return f"""# Kagan Test Configuration
[general]
auto_start = {str(auto_start).lower()}
auto_merge = {str(auto_merge).lower()}
default_base_branch = "{base_branch}"
default_worker_agent = "claude"

[agents.claude]
identity = "claude.ai"
name = "Claude"
short_name = "claude"
run_command."*" = "echo mock-claude"
interactive_command."*" = "echo mock-claude-interactive"
active = true
"""


def setup_kagan_dir(project: Path, config_content: str | None = None) -> SimpleNamespace:
    """Setup .kagan directory structure. Returns namespace with paths."""
    kagan_dir = project / ".kagan"
    kagan_dir.mkdir(parents=True, exist_ok=True)

    content = config_content or create_kagan_config_content()
    (kagan_dir / "config.toml").write_text(content)

    return SimpleNamespace(
        root=project,
        db=str(kagan_dir / "state.db"),
        config=str(kagan_dir / "config.toml"),
        kagan_dir=kagan_dir,
    )
