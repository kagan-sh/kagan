"""Prompt loader with layered override support."""

from __future__ import annotations

from functools import cache
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from kagan.config import KaganConfig

# Default prompts directory (user overrides)
DEFAULT_PROMPTS_DIR = Path(".kagan/prompts")

# Built-in prompts directory (package defaults)
BUILTIN_PROMPTS_DIR = Path(__file__).parent.parent / "prompts"


def dump_default_prompts(prompts_dir: Path | None = None) -> None:
    """Dump default prompt templates to the user's prompts directory.

    Creates the prompts directory structure and writes the built-in templates
    so users can customize them.

    Args:
        prompts_dir: Target directory (defaults to .kagan/prompts).
    """
    target_dir = prompts_dir or DEFAULT_PROMPTS_DIR

    # Create directory
    target_dir.mkdir(parents=True, exist_ok=True)

    # Write planner prompt (no comments - they get sent to the AI)
    planner_content = _get_default_planner_prompt()
    (target_dir / "planner.md").write_text(planner_content)


class PromptLoader:
    """Load prompts with layered override support.

    Priority: User files > TOML inline > Built-in defaults

    User files are loaded from:
        .kagan/prompts/planner.md
    """

    def __init__(self, config: KaganConfig, prompts_dir: Path | None = None) -> None:
        """Initialize the prompt loader.

        Args:
            config: The Kagan configuration.
            prompts_dir: Optional custom prompts directory (defaults to .kagan/prompts).
        """
        self._config = config
        self._prompts_dir = prompts_dir or DEFAULT_PROMPTS_DIR

    def get_planner_prompt(self) -> str:
        """Load planner system prompt: .kagan/prompts/planner.md > toml > hardcoded."""
        # Priority 1: User file override
        user_file = self._prompts_dir / "planner.md"
        if user_file.exists():
            return user_file.read_text()

        # Priority 2: TOML inline config
        if self._config.prompts.planner_system_prompt:
            return self._config.prompts.planner_system_prompt

        # Priority 3: Built-in default (hardcoded in planner.py)
        return _get_default_planner_prompt()


@cache
def _load_builtin_template(filename: str) -> str:
    """Load a built-in template file.

    Args:
        filename: The template filename (e.g., "iteration.md").

    Returns:
        The template content, or fallback inline template if not found.
    """
    template_path = BUILTIN_PROMPTS_DIR / filename
    if template_path.exists():
        return template_path.read_text()

    return ""


def _get_default_planner_prompt() -> str:
    """Get the default planner preamble (customizable part only).

    Note: The output format section is always appended by build_planner_prompt()
    to ensure ticket parsing works correctly.
    """
    return """\
You are a project planning assistant. Your job is to take user requests
and create well-structured development tickets.

When the user describes what they want to build or accomplish,
analyze their request and create ONE detailed ticket.

## Guidelines
1. Title should start with a verb (Create, Implement, Fix, Add, Update, etc.)
2. Description should be thorough enough for a developer to understand the task
3. Include acceptance criteria as bullet points
4. If the request is vague, make reasonable assumptions and note them

After outputting the ticket, briefly explain what you created and any assumptions you made.
"""
