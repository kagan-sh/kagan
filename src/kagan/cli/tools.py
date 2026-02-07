"""CLI tools for stateless one-shot operations."""

from __future__ import annotations

import asyncio
from pathlib import Path

import click
from rich._spinners import SPINNERS
from rich.console import Console

from kagan.ui.utils.animation import WAVE_FRAMES, WAVE_INTERVAL_MS

SPINNERS["portal"] = {"interval": WAVE_INTERVAL_MS, "frames": WAVE_FRAMES}

TOOL_CHOICES = ("claude", "opencode")


def _get_default_tool() -> str:
    """Auto-detect the first available AI tool."""
    from kagan.builtin_agents import get_all_agent_availability

    for availability in get_all_agent_availability():
        if availability.is_available:
            return availability.agent.config.short_name
    return "claude"


@click.group()
def tools() -> None:
    """Stateless developer utilities."""
    pass


@tools.command()
@click.argument("prompt", required=False, default=None)
@click.option(
    "-t",
    "--tool",
    type=click.Choice(TOOL_CHOICES, case_sensitive=False),
    default=None,
    help="AI tool for enhancement (auto-detects if omitted)",
)
@click.option(
    "-f",
    "--file",
    "file_path",
    type=click.Path(exists=True, readable=True, path_type=Path),
    default=None,
    help="Read prompt from a file (supports multiline content)",
)
def enhance(prompt: str | None, tool: str | None, file_path: Path | None) -> None:
    """Enhance a prompt for AI coding assistants.

    \b
    Examples:
        kagan tools enhance "fix the login bug"
        kagan tools enhance "add dark mode" -t opencode
        kagan tools enhance "refactor auth" | pbcopy
        kagan tools enhance --file prompt.txt
        kagan tools enhance -f requirements.md -t claude
    """
    from kagan.agents.refiner import PromptRefiner
    from kagan.builtin_agents import get_builtin_agent

    if file_path is not None:
        prompt = file_path.read_text().strip()
    elif prompt is None:
        raise click.UsageError("Either provide a PROMPT argument or use --file option")

    console = Console(stderr=True)

    if tool is None:
        tool = _get_default_tool()
        console.print(f"[dim]Using {tool}[/]", highlight=False)

    agent = get_builtin_agent(tool)
    if not agent or not agent.config:
        raise click.ClickException(f"Unknown tool: {tool}")

    agent_config = agent.config

    async def _enhance() -> str:
        refiner = PromptRefiner(Path.cwd(), agent_config)
        try:
            return await refiner.refine(prompt)
        finally:
            await refiner.stop()

    with console.status("[cyan]Enhancing...", spinner="portal"):
        try:
            result = asyncio.run(_enhance())
        except Exception as e:
            console.print(f"[yellow]Enhancement failed: {e}[/]", highlight=False)
            result = prompt

    click.echo(result)
