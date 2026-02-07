"""CLI entry point for Kagan."""

from __future__ import annotations

# Python version check - must be before any imports that use 3.12+ syntax.
# This check uses only Python 3.8+ compatible syntax.
# Note: from __future__ is allowed before this check as it's valid in Python 3.7+.
import sys

if sys.version_info < (3, 12):  # noqa: UP036
    print("Error: Kagan requires Python 3.12 or higher.")
    print(
        "You are running Python {}.{}".format(  # noqa: UP032
            sys.version_info.major, sys.version_info.minor
        )
    )
    print("Please upgrade Python: https://www.python.org/downloads/")
    sys.exit(1)

# Suppress asyncio subprocess cleanup errors on exit.
# When GC runs after the event loop closes, subprocess transports may try to
# close their pipes and fail. This was fixed in Python 3.13.1+ and 3.14+ (gh-114177),
# but we need this workaround for Python 3.12.

_original_unraisablehook = sys.unraisablehook


def _suppress_event_loop_closed(unraisable: sys.UnraisableHookArgs) -> None:
    """Suppress 'Event loop is closed' errors from asyncio cleanup."""
    if isinstance(unraisable.exc_value, RuntimeError) and "Event loop is closed" in str(
        unraisable.exc_value
    ):
        return
    _original_unraisablehook(unraisable)


sys.unraisablehook = _suppress_event_loop_closed

# Standard imports after hook is installed
import asyncio  # noqa: E402
import os  # noqa: E402
import shutil  # noqa: E402
from pathlib import Path  # noqa: E402, TC003

import click  # noqa: E402

from kagan import __version__  # noqa: E402
from kagan.cli.tools import tools  # noqa: E402
from kagan.cli.update import check_for_updates, prompt_and_update, update  # noqa: E402
from kagan.constants import DEFAULT_DB_PATH  # noqa: E402
from kagan.paths import (  # noqa: E402
    get_cache_dir,
    get_config_dir,
    get_data_dir,
    get_worktree_base_dir,
)


def _check_for_updates_gate() -> None:
    """Check for updates and prompt user before starting TUI.

    If an update is available, prompts the user to update. If they choose
    to update, performs the update and exits so they can restart with the
    new version.
    """
    result = check_for_updates()

    # Skip silently for dev versions or fetch errors
    if result.is_dev or result.error:
        return

    if result.update_available:
        click.echo()
        click.secho("A newer version of kagan is available!", fg="yellow", bold=True)
        click.echo(f"  Current: {click.style(result.current_version, fg='red')}")
        click.echo(f"  Latest:  {click.style(result.latest_version, fg='green', bold=True)}")
        click.echo()

        if click.confirm("Would you like to update before starting?", default=True):
            updated = prompt_and_update(result, force=True)
            if updated:
                click.echo()
                click.secho("Please restart kagan to use the new version.", fg="cyan")
                sys.exit(0)
        else:
            click.echo("Continuing with current version...")
            click.echo()


@click.group(invoke_without_command=True)
@click.option("--version", is_flag=True, help="Show version and exit")
@click.pass_context
def cli(ctx: click.Context, version: bool) -> None:
    """AI-powered Kanban TUI for autonomous development workflows."""
    if version:
        click.echo(f"kagan {__version__}")
        ctx.exit(0)

    # Run TUI by default if no subcommand
    if ctx.invoked_subcommand is None:
        ctx.invoke(tui)


# Register subcommands
cli.add_command(update)
cli.add_command(tools)


@cli.command()
@click.option("--db", default=DEFAULT_DB_PATH, help="Path to SQLite database")
@click.option("--skip-preflight", is_flag=True, help="Skip pre-flight checks (development only)")
@click.option(
    "--skip-update-check",
    is_flag=True,
    envvar="KAGAN_SKIP_UPDATE_CHECK",
    help="Skip update check on startup",
)
def tui(db: str, skip_preflight: bool, skip_update_check: bool) -> None:
    """Run the Kanban TUI (default command)."""
    db_path = db

    # Check for updates before starting TUI (unless skipped)
    if not skip_update_check and not os.environ.get("KAGAN_SKIP_UPDATE_CHECK"):
        _check_for_updates_gate()

    # Run pre-flight checks unless skipped
    if not skip_preflight:
        from kagan.builtin_agents import (
            any_agent_available,
            get_first_available_agent,
        )
        from kagan.ui.screens.troubleshooting import (
            TroubleshootingApp,
            create_no_agents_issues,
            detect_issues,
        )

        # First check: Are ANY agents available?
        if not any_agent_available():
            # No agents available - show installation options for all supported agents
            issues = create_no_agents_issues()
            app = TroubleshootingApp(issues)
            app.run()
            sys.exit(1)

        # At least one agent is available - use the first available one for pre-flight
        # The user can select a different agent in the welcome screen later
        best_agent = get_first_available_agent()
        if best_agent:
            agent_name = best_agent.config.name
            agent_install = best_agent.install_command
            agent_config = best_agent.config

            # Run pre-flight checks
            result = asyncio.run(
                detect_issues(
                    agent_config=agent_config,
                    agent_name=agent_name,
                    agent_install_command=agent_install,
                )
            )

            # Show troubleshooting screen if there are any issues
            if result.issues:
                app = TroubleshootingApp(result.issues)
                exit_code = app.run()

                # If blocking issues, always exit
                if result.has_blocking_issues:
                    sys.exit(1)

                # For warnings, check if user chose to quit or continue
                if exit_code != TroubleshootingApp.EXIT_CONTINUE:
                    sys.exit(exit_code if exit_code is not None else 1)

    # Launch the app (no instance lock - multiple instances allowed)
    from kagan.app import KaganApp

    app = KaganApp(db_path=db_path)
    app.run()


@cli.command()
@click.option(
    "--force",
    "-f",
    is_flag=True,
    help="Skip confirmation prompt (use with caution)",
)
def reset(force: bool) -> None:
    """Remove all Kagan configuration, data, and cache files.

    This is a DESTRUCTIVE operation that will permanently delete:
    - Configuration files (config.toml, profiles.toml)
    - Database (kagan.db with all tasks and history)
    - Cache files
    - Worktree directories

    Use --force to skip the confirmation prompt.
    """
    # Gather all directories that will be affected
    dirs_to_remove: list[tuple[str, Path]] = [
        ("Config directory", get_config_dir()),
        ("Data directory", get_data_dir()),
        ("Cache directory", get_cache_dir()),
        ("Worktree directory", get_worktree_base_dir()),
    ]

    # Check which directories actually exist and deduplicate paths
    # (on macOS, config and data dirs may be the same)
    seen_paths: set[Path] = set()
    existing_dirs: list[tuple[str, Path]] = []
    for name, path in dirs_to_remove:
        if path.exists() and path not in seen_paths:
            existing_dirs.append((name, path))
            seen_paths.add(path)

    if not existing_dirs:
        click.secho("Nothing to reset - no Kagan directories found.", fg="yellow")
        return

    # Display warning and what will be deleted
    click.echo()
    click.secho("WARNING: This will permanently delete the following:", fg="red", bold=True)
    click.echo()

    total_size = 0
    for name, path in existing_dirs:
        # Calculate directory size
        dir_size = sum(f.stat().st_size for f in path.rglob("*") if f.is_file())
        total_size += dir_size
        size_str = _format_size(dir_size)

        click.echo(f"  {click.style('•', fg='red')} {name}: {click.style(str(path), fg='cyan')}")
        click.echo(f"    Size: {size_str}")

        # List key files in the directory
        key_files = list(path.glob("*"))[:5]
        if key_files:
            click.echo("    Contains:")
            for f in key_files:
                click.echo(f"      - {f.name}")
            remaining = len(list(path.glob("*"))) - 5
            if remaining > 0:
                click.echo(f"      ... and {remaining} more items")
        click.echo()

    click.echo(f"Total size: {click.style(_format_size(total_size), fg='yellow', bold=True)}")
    click.echo()
    click.secho("This action cannot be undone!", fg="red", bold=True)
    click.echo()

    # Require explicit confirmation unless --force is used
    if not force:
        confirmed = click.prompt(
            click.style("Type 'yes' to confirm deletion", fg="yellow"),
            default="",
            show_default=False,
        )
        if confirmed.lower() != "yes":
            click.secho("Reset cancelled.", fg="green")
            return

    # Perform the deletion
    click.echo()
    click.echo("Removing Kagan directories...")

    errors = []
    for name, path in existing_dirs:
        try:
            shutil.rmtree(path)
            click.echo(f"  {click.style('✓', fg='green')} Removed {name}: {path}")
        except OSError as e:
            errors.append((name, path, e))
            click.echo(f"  {click.style('✗', fg='red')} Failed to remove {name}: {e}")

    click.echo()
    if errors:
        click.secho(f"Reset completed with {len(errors)} error(s).", fg="yellow")
    else:
        click.secho("Reset complete. All Kagan data has been removed.", fg="green", bold=True)


def _format_size(size_bytes: int | float) -> str:
    """Format byte size to human-readable string."""
    size = float(size_bytes)
    for unit in ["B", "KB", "MB", "GB"]:
        if size < 1024:
            return f"{size:.1f} {unit}"
        size /= 1024
    return f"{size:.1f} TB"


@cli.command()
@click.option(
    "--readonly",
    is_flag=True,
    help="Expose only read-only coordination tools (for ACP agents)",
)
def mcp(readonly: bool) -> None:
    """Run the MCP server (STDIO transport).

    This command is typically invoked by AI agents (Claude Code, OpenCode, etc.)
    to communicate with Kagan via the Model Context Protocol.

    The MCP server uses centralized storage and assumes the current working
    directory is a Kagan-managed project.

    Use --readonly for ACP agents to expose only coordination tools
    (get_parallel_tasks, get_agent_logs).
    """
    from kagan.mcp.server import main as mcp_main

    mcp_main(readonly=readonly)


if __name__ == "__main__":
    cli()
