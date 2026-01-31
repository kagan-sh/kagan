"""CLI entry point for Kagan."""

from __future__ import annotations

# Suppress asyncio subprocess cleanup errors on exit.
# When GC runs after the event loop closes, subprocess transports may try to
# close their pipes and fail. This was fixed in Python 3.13.1+ and 3.14+ (gh-114177),
# but we need this workaround for Python 3.12.
import sys

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
from pathlib import Path  # noqa: E402

import click  # noqa: E402

from kagan import __version__  # noqa: E402
from kagan.cli.update import check_for_updates, prompt_and_update, update  # noqa: E402
from kagan.constants import DEFAULT_CONFIG_PATH, DEFAULT_DB_PATH, DEFAULT_LOCK_PATH  # noqa: E402


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


# Register update subcommand
cli.add_command(update)


@cli.command()
@click.option("--db", default=DEFAULT_DB_PATH, help="Path to SQLite database")
@click.option("--config", default=DEFAULT_CONFIG_PATH, help="Path to config file")
@click.option("--skip-preflight", is_flag=True, help="Skip pre-flight checks (development only)")
@click.option(
    "--skip-update-check",
    is_flag=True,
    envvar="KAGAN_SKIP_UPDATE_CHECK",
    help="Skip update check on startup",
)
def tui(db: str, config: str, skip_preflight: bool, skip_update_check: bool) -> None:
    """Run the Kanban TUI (default command)."""
    config_path = Path(config)
    db_path = db

    # Derive db path from config path if only config is specified
    if db == DEFAULT_DB_PATH and config != DEFAULT_CONFIG_PATH:
        db_path = str(config_path.parent / "state.db")

    # Check for updates before starting TUI (unless skipped)
    if not skip_update_check and not os.environ.get("KAGAN_SKIP_UPDATE_CHECK"):
        _check_for_updates_gate()

    # Run pre-flight checks unless skipped
    if not skip_preflight:
        from kagan.data.builtin_agents import (
            any_agent_available,
            get_first_available_agent,
        )
        from kagan.ui.screens.troubleshooting import (
            ISSUE_PRESETS,
            DetectedIssue,
            IssueType,
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

            # Run pre-flight checks (except lock - we'll check that separately)
            result = asyncio.run(
                detect_issues(
                    check_lock=False,
                    agent_config=agent_config,
                    agent_name=agent_name,
                    agent_install_command=agent_install,
                )
            )

            if result.has_blocking_issues:
                app = TroubleshootingApp(result.issues)
                app.run()
                sys.exit(1)

    # Import here to avoid slow startup for --help/--version
    from kagan.lock import InstanceLock, InstanceLockError

    lock = InstanceLock(DEFAULT_LOCK_PATH)
    try:
        lock.acquire()
    except InstanceLockError:
        if skip_preflight:
            # Simple message if preflight was skipped
            from kagan.ui.screens.troubleshooting import (
                ISSUE_PRESETS,
                DetectedIssue,
                IssueType,
                TroubleshootingApp,
            )

            issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.INSTANCE_LOCKED])]
            app = TroubleshootingApp(issues)
            app.run()
            sys.exit(1)
        else:
            # Re-run detect_issues with lock failure
            from kagan.ui.screens.troubleshooting import (
                ISSUE_PRESETS,
                DetectedIssue,
                IssueType,
                TroubleshootingApp,
            )

            issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.INSTANCE_LOCKED])]
            app = TroubleshootingApp(issues)
            app.run()
            sys.exit(1)

    try:
        from kagan.app import KaganApp

        app = KaganApp(db_path=db_path, config_path=config)
        app._instance_lock = lock
        app.run()
    finally:
        lock.release()


@cli.command()
def mcp() -> None:
    """Run the MCP server (STDIO transport).

    This command is typically invoked by AI agents (Claude Code, OpenCode, etc.)
    to communicate with Kagan via the Model Context Protocol.

    The MCP server finds the nearest .kagan/ directory by traversing up
    from the current working directory.
    """
    from kagan.mcp.server import main as mcp_main

    mcp_main()


if __name__ == "__main__":
    cli()
