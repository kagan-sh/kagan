"""CLI entry point for Kagan."""

from __future__ import annotations

import os
import sys
from pathlib import Path

import click

from kagan import __version__
from kagan.cli.update import check_for_updates, prompt_and_update, update
from kagan.constants import DEFAULT_CONFIG_PATH, DEFAULT_DB_PATH, DEFAULT_LOCK_PATH


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
        from kagan.config import KaganConfig
        from kagan.data.builtin_agents import get_builtin_agent
        from kagan.ui.screens.troubleshooting import (
            ISSUE_PRESETS,
            DetectedIssue,
            IssueType,
            TroubleshootingApp,
            detect_issues,
        )

        # Determine agent to check (read config early if exists)
        agent_name = "Claude Code"
        agent_install = "curl -fsSL https://claude.ai/install.sh | bash"
        agent_config = None

        if config_path.exists():
            cfg = KaganConfig.load(config_path)
            agent_key = cfg.general.default_worker_agent
            agent_config = cfg.get_agent(agent_key)
            if builtin := get_builtin_agent(agent_key):
                agent_name = builtin.config.name
                agent_install = builtin.install_command
                agent_config = builtin.config
        else:
            # Use default Claude agent config for first boot
            builtin = get_builtin_agent("claude")
            if builtin:
                agent_config = builtin.config

        # Run pre-flight checks (except lock - we'll check that separately)
        result = detect_issues(
            check_lock=False,
            agent_config=agent_config,
            agent_name=agent_name,
            agent_install_command=agent_install,
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
