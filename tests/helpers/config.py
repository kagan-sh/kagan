from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path


def _build_test_config_toml(
    *,
    auto_review: bool = False,
    skip_pair_instructions: bool | None = None,
    default_pair_terminal_backend: str | None = None,
    header_comment: str | None = None,
) -> str:
    """Build a canonical TOML test config used by TUI smoke/snapshot tests."""
    lines: list[str] = []
    if header_comment:
        lines.append(f"# {header_comment}")

    lines.extend(
        [
            "[general]",
            f"auto_review = {str(auto_review).lower()}",
            'default_worker_agent = "claude"',
        ]
    )
    if default_pair_terminal_backend is not None:
        lines.append(f'default_pair_terminal_backend = "{default_pair_terminal_backend}"')

    if skip_pair_instructions is not None:
        lines.extend(
            [
                "",
                "[ui]",
                f"skip_pair_instructions = {str(skip_pair_instructions).lower()}",
            ]
        )

    lines.extend(
        [
            "",
            "[agents.claude]",
            'identity = "claude.ai"',
            'name = "Claude"',
            'short_name = "claude"',
            'run_command."*" = "echo mock-claude"',
            'interactive_command."*" = "echo mock-claude-interactive"',
            "active = true",
        ]
    )

    return "\n".join(lines) + "\n"


def write_test_config(
    config_path: Path,
    *,
    auto_review: bool = False,
    skip_pair_instructions: bool | None = None,
    default_pair_terminal_backend: str | None = None,
    header_comment: str | None = None,
) -> Path:
    """Write a canonical TOML test config and return its path."""
    config_path.write_text(
        _build_test_config_toml(
            auto_review=auto_review,
            skip_pair_instructions=skip_pair_instructions,
            default_pair_terminal_backend=default_pair_terminal_backend,
            header_comment=header_comment,
        ),
        encoding="utf-8",
    )
    return config_path
