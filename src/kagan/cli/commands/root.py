"""Root CLI command registration."""

from __future__ import annotations

from importlib.metadata import version

import click

__version__ = version("kagan")
from kagan.cli.tools import tools
from kagan.cli.update import update

from .core import core
from .list_projects import list_cmd
from .mcp import mcp
from .reset import reset
from .tui import tui


@click.group(invoke_without_command=True)
@click.option("--version", is_flag=True, help="Show version and exit")
@click.pass_context
def cli(ctx: click.Context, version: bool) -> None:
    """AI-powered Kanban TUI for autonomous development workflows."""
    if version:
        click.echo(f"kagan {__version__}")
        ctx.exit(0)

    if ctx.invoked_subcommand is None:
        ctx.invoke(tui)


cli.add_command(update)
cli.add_command(tools)
cli.add_command(tui)
cli.add_command(reset)
cli.add_command(list_cmd)
cli.add_command(mcp)
cli.add_command(core)
