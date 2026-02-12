"""TUI command and startup preflight helpers."""

from __future__ import annotations

import asyncio
import os
import platform
import shutil
import sys
from datetime import timedelta
from pathlib import Path
from typing import TYPE_CHECKING

import click

from kagan.cli.update import check_for_updates, prompt_and_update
from kagan.core.constants import DEFAULT_DB_PATH
from kagan.core.paths import get_config_path
from kagan.core.time import utc_now

if TYPE_CHECKING:
    from collections.abc import Sequence

    from kagan.core.preflight import DetectedIssue, PreflightResult


def _ensure_core_ready_for_cli(db_path: str) -> None:
    """Auto-start core daemon for TUI clients when config allows it."""
    from kagan.core.config import KaganConfig
    from kagan.core.launcher import ensure_core_running_sync

    config = KaganConfig.load(get_config_path())
    if not config.general.core_autostart:
        return

    try:
        ensure_core_running_sync(
            config=config,
            config_path=get_config_path(),
            db_path=Path(db_path),
        )
    except Exception as exc:
        click.secho(f"Failed to start core daemon: {exc}", fg="red")
        sys.exit(1)


def _check_for_updates_gate() -> None:
    """Check for updates and prompt user before starting TUI."""
    result = check_for_updates()

    if result.is_dev or result.is_local or result.error:
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


async def _cleanup_done_workspaces(db_path: str, older_than_days: int) -> int:
    from sqlalchemy.ext.asyncio import async_sessionmaker
    from sqlmodel import col, select

    from kagan.core.adapters.db.engine import create_db_engine
    from kagan.core.adapters.db.schema import Task, Workspace, WorkspaceRepo
    from kagan.core.adapters.git.worktrees import GitWorktreeAdapter
    from kagan.core.models.enums import TaskStatus, WorkspaceStatus

    cutoff = utc_now() - timedelta(days=older_than_days)
    engine = await create_db_engine(db_path)
    session_factory = async_sessionmaker(engine, expire_on_commit=False)
    git = GitWorktreeAdapter()

    async with session_factory() as session:
        result = await session.execute(
            select(Task, Workspace, WorkspaceRepo)
            .join(Workspace, col(Workspace.task_id) == col(Task.id))
            .join(WorkspaceRepo, col(WorkspaceRepo.workspace_id) == col(Workspace.id))
            .where(Task.status == TaskStatus.DONE)
            .where(Task.updated_at < cutoff)
        )
        rows = result.all()
        if not rows:
            await engine.dispose()
            return 0

        now = utc_now()
        workspaces: dict[str, Workspace] = {}
        for _task, workspace, workspace_repo in rows:
            workspaces[workspace.id] = workspace
            if workspace_repo.worktree_path:
                await git.delete_worktree(workspace_repo.worktree_path)
                workspace_repo.worktree_path = None
                workspace_repo.updated_at = now
                session.add(workspace_repo)

        for workspace in workspaces.values():
            if workspace.path and Path(workspace.path).exists():
                shutil.rmtree(workspace.path, ignore_errors=True)
            if workspace.status != WorkspaceStatus.ARCHIVED:
                workspace.status = WorkspaceStatus.ARCHIVED
                workspace.updated_at = now
                session.add(workspace)

        await session.commit()

    await engine.dispose()
    return len(workspaces)


def _auto_cleanup_done_workspaces(db_path: str, *, older_than_days: int = 7) -> None:
    if db_path == ":memory:":
        return
    db_file = Path(db_path)
    if not db_file.exists():
        return
    try:
        asyncio.run(_cleanup_done_workspaces(db_path, older_than_days))
    except Exception as exc:
        click.secho(f"Preflight cleanup failed: {exc}", fg="yellow")


def _display_issue(issue: DetectedIssue) -> None:
    preset = issue.preset
    from kagan.core.preflight import IssueSeverity

    severity_label = "BLOCKING" if preset.severity == IssueSeverity.BLOCKING else "WARNING"
    severity_color = "red" if preset.severity == IssueSeverity.BLOCKING else "yellow"

    click.echo()
    click.echo(
        f"  {click.style(preset.icon, fg=severity_color)} "
        f"{click.style(preset.title, fg=severity_color, bold=True)} "
        f"[{click.style(severity_label, fg=severity_color)}]"
    )
    for line in preset.message.split("\n"):
        click.echo(f"    {line}")
    click.echo(f"    {click.style('Hint:', fg='cyan')} {preset.hint}")
    if preset.url:
        click.echo(f"    {click.style('More info:', fg='cyan')} {preset.url}")


def _handle_no_agents(issues: Sequence[DetectedIssue]) -> None:
    """Show all agent options and prompt for installation."""
    del issues
    from kagan.core.builtin_agents import list_builtin_agents

    click.echo()
    click.secho("  No AI Agents Found", fg="red", bold=True)
    click.echo("  Install one of the following to get started:")
    click.echo()

    agents = list_builtin_agents()
    for i, agent in enumerate(agents, 1):
        click.echo(f"    {i}) {click.style(agent.config.name, bold=True)}")
        click.echo(f"       {agent.description}")
        click.echo(f"       $ {click.style(agent.install_command, fg='cyan')}")
        click.echo()

    if not click.confirm("  Would you like to install an agent now?", default=True):
        sys.exit(1)

    choice = click.prompt(
        "  Select agent to install",
        type=click.IntRange(1, len(agents)),
        default=1,
    )
    selected = agents[choice - 1]

    click.echo()
    click.echo(f"  Installing {selected.config.name}...")
    from kagan.core.agents.installer import install_agent

    success, message = asyncio.run(install_agent(selected.config.short_name))
    if success:
        click.secho(f"  {message}", fg="green")
        click.secho("  Please restart kagan.", fg="cyan")
    else:
        click.secho(f"  Installation failed: {message}", fg="red")
    sys.exit(0 if success else 1)


def _display_agent_status() -> dict[str, bool]:
    """Display per-agent availability status and return status dict."""
    from kagan.core.builtin_agents import get_agent_status, list_builtin_agents

    agents = list_builtin_agents()
    status = get_agent_status()

    click.echo()
    click.secho("  AI Agents:", bold=True)

    for agent in agents:
        name = agent.config.short_name
        available = status.get(name, False)

        if available:
            icon = click.style("✓", fg="green")
            label = click.style(agent.config.name, fg="green")
        else:
            icon = click.style("○", fg="bright_black")
            label = click.style(f"{agent.config.name} (not installed)", fg="bright_black")

        click.echo(f"    {icon} {label}")

    available_count = sum(status.values())
    click.echo()
    if available_count > 0:
        click.secho(f"  {available_count} agent(s) available", fg="green")
    else:
        click.secho("  No agents installed", fg="red")

    return status


def _handle_preflight_issues(result: PreflightResult, severity_enum: type) -> None:
    """Display preflight issues. Exit on blocking; prompt on warnings-only."""
    issues = result.issues
    has_blocking = result.has_blocking_issues

    blocking_count = sum(1 for i in issues if i.preset.severity == severity_enum.BLOCKING)
    warning_count = sum(1 for i in issues if i.preset.severity == severity_enum.WARNING)

    click.echo()
    if has_blocking:
        plural = "s" if blocking_count != 1 else ""
        click.secho("  Startup Issues Detected", fg="red", bold=True)
        click.echo(f"  {blocking_count} blocking issue{plural} found")
    else:
        plural = "s" if warning_count != 1 else ""
        click.secho("  Startup Warnings", fg="yellow", bold=True)
        click.echo(f"  {warning_count} warning{plural} detected")

    for issue in issues:
        _display_issue(issue)

    click.echo()
    if has_blocking:
        click.echo("  Resolve the blocking issues above and restart kagan.")
        sys.exit(1)

    click.echo("  You can continue, but some features may not work optimally.")
    if not click.confirm("  Continue anyway?", default=True):
        sys.exit(0)


@click.command()
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

    if not skip_update_check and not os.environ.get("KAGAN_SKIP_UPDATE_CHECK"):
        _check_for_updates_gate()

    if not skip_preflight:
        _auto_cleanup_done_workspaces(db_path)

        agent_status = _display_agent_status()
        if not any(agent_status.values()):
            from kagan.core.preflight import create_no_agents_issues

            _handle_no_agents(create_no_agents_issues())

        from kagan.core.builtin_agents import get_first_available_agent
        from kagan.core.config import KaganConfig
        from kagan.core.preflight import IssueSeverity, detect_issues

        default_pair_terminal_backend = "vscode" if platform.system() == "Windows" else "tmux"
        try:
            loaded_config = KaganConfig.load(get_config_path())
            default_pair_terminal_backend = loaded_config.general.default_pair_terminal_backend
        except Exception:
            pass

        best_agent = get_first_available_agent()
        if best_agent:
            agent_name = best_agent.config.name
            agent_install = best_agent.install_command
            agent_config = best_agent.config

            result = asyncio.run(
                detect_issues(
                    agent_config=agent_config,
                    agent_name=agent_name,
                    agent_install_command=agent_install,
                    default_pair_terminal_backend=default_pair_terminal_backend,
                )
            )

            if result.issues:
                _handle_preflight_issues(result, IssueSeverity)

    from kagan.tui.app import KaganApp

    _ensure_core_ready_for_cli(db_path)
    app = KaganApp(db_path=db_path)
    app.run()
