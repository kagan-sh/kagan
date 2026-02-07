"""CLI entry point for Kagan."""

from __future__ import annotations

# Python version check - must be before any imports that use 3.12+ syntax.
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

_original_unraisablehook = sys.unraisablehook


def _suppress_event_loop_closed(unraisable: sys.UnraisableHookArgs) -> None:
    """Suppress 'Event loop is closed' errors from asyncio cleanup."""
    if isinstance(unraisable.exc_value, RuntimeError) and "Event loop is closed" in str(
        unraisable.exc_value
    ):
        return
    _original_unraisablehook(unraisable)


# Workaround for Py3.12 asyncio cleanup errors (fixed in 3.13.1+).
sys.unraisablehook = _suppress_event_loop_closed


import asyncio  # noqa: E402
import os  # noqa: E402
import platform  # noqa: E402
import shutil  # noqa: E402
from pathlib import Path  # noqa: E402
from typing import TYPE_CHECKING  # noqa: E402

import click  # noqa: E402

from kagan import __version__  # noqa: E402
from kagan.cli.tools import tools  # noqa: E402
from kagan.cli.update import check_for_updates, prompt_and_update, update  # noqa: E402
from kagan.constants import DEFAULT_DB_PATH  # noqa: E402
from kagan.paths import (  # noqa: E402
    get_cache_dir,
    get_config_dir,
    get_config_path,
    get_data_dir,
    get_worktree_base_dir,
)

if TYPE_CHECKING:
    from collections.abc import Sequence

    from kagan.preflight import DetectedIssue


def _check_for_updates_gate() -> None:
    """Check for updates and prompt user before starting TUI."""
    result = check_for_updates()

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


async def _cleanup_done_workspaces(db_path: str, older_than_days: int) -> int:
    from datetime import datetime, timedelta

    from sqlalchemy.ext.asyncio import async_sessionmaker
    from sqlmodel import col, select

    from kagan.adapters.db.engine import create_db_engine
    from kagan.adapters.db.schema import Task, Workspace, WorkspaceRepo
    from kagan.adapters.git.worktrees import GitWorktreeAdapter
    from kagan.core.models.enums import TaskStatus, WorkspaceStatus

    cutoff = datetime.now() - timedelta(days=older_than_days)
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

        now = datetime.now()
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
    preset = issue.preset  # type: ignore[attr-defined]
    from kagan.preflight import IssueSeverity

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
    from kagan.builtin_agents import list_builtin_agents

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
    from kagan.agents.installer import install_agent

    success, message = asyncio.run(install_agent(selected.config.short_name))
    if success:
        click.secho(f"  {message}", fg="green")
        click.secho("  Please restart kagan.", fg="cyan")
    else:
        click.secho(f"  Installation failed: {message}", fg="red")
    sys.exit(0 if success else 1)


def _display_agent_status() -> dict[str, bool]:
    """Display per-agent availability status and return status dict."""
    from kagan.builtin_agents import get_agent_status, list_builtin_agents

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


def _handle_preflight_issues(result: object, severity_enum: type) -> None:
    """Display preflight issues. Exit on blocking; prompt on warnings-only."""
    issues = result.issues  # type: ignore[attr-defined]
    has_blocking = result.has_blocking_issues  # type: ignore[attr-defined]

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
    else:
        click.echo("  You can continue, but some features may not work optimally.")
        if not click.confirm("  Continue anyway?", default=True):
            sys.exit(0)


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

    if not skip_update_check and not os.environ.get("KAGAN_SKIP_UPDATE_CHECK"):
        _check_for_updates_gate()

    if not skip_preflight:
        _auto_cleanup_done_workspaces(db_path)

        # Show agent status
        agent_status = _display_agent_status()

        if not any(agent_status.values()):
            from kagan.preflight import create_no_agents_issues

            _handle_no_agents(create_no_agents_issues())

        # At least one agent is available - use the first available one for pre-flight
        from kagan.builtin_agents import get_first_available_agent
        from kagan.config import KaganConfig
        from kagan.preflight import IssueSeverity, detect_issues

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

    from kagan.app import KaganApp

    app = KaganApp(db_path=db_path)
    app.run()


async def _get_projects(db_path: str) -> list[tuple[str, str]]:
    """Return list of (id, name) for all projects in the database."""
    from sqlalchemy.ext.asyncio import async_sessionmaker
    from sqlmodel import select

    from kagan.adapters.db.engine import create_db_engine
    from kagan.adapters.db.schema import Project

    engine = await create_db_engine(db_path)
    session_factory = async_sessionmaker(engine, expire_on_commit=False)
    try:
        async with session_factory() as session:
            result = await session.execute(select(Project.id, Project.name).order_by(Project.name))
            return [(row[0], row[1]) for row in result.all()]
    finally:
        await engine.dispose()


async def _delete_project_data(db_path: str, project_id: str) -> str:
    """Delete all data for a project. Returns project name."""
    from sqlalchemy import delete, update
    from sqlalchemy.ext.asyncio import async_sessionmaker
    from sqlmodel import col, select

    from kagan.adapters.db.engine import create_db_engine
    from kagan.adapters.db.schema import (
        CodingAgentTurn,
        ExecutionProcess,
        ExecutionProcessLog,
        ExecutionProcessRepoState,
        Image,
        Merge,
        Project,
        ProjectRepo,
        Session,
        Task,
        TaskLink,
        TaskTag,
        Workspace,
        WorkspaceRepo,
    )

    engine = await create_db_engine(db_path)
    session_factory = async_sessionmaker(engine, expire_on_commit=False)
    try:
        async with session_factory() as session:
            # Fetch project name
            proj = await session.get(Project, project_id)
            if not proj:
                msg = f"Project {project_id} not found"
                raise ValueError(msg)
            project_name = proj.name

            # Collect workspace IDs
            ws_rows = await session.execute(
                select(Workspace.id).where(col(Workspace.project_id) == project_id)
            )
            ws_ids = [r[0] for r in ws_rows.all()]

            # Collect session IDs
            sess_ids: list[str] = []
            if ws_ids:
                sess_rows = await session.execute(
                    select(Session.id).where(col(Session.workspace_id).in_(ws_ids))
                )
                sess_ids = [r[0] for r in sess_rows.all()]

            # Collect execution IDs
            exec_ids: list[str] = []
            if sess_ids:
                exec_rows = await session.execute(
                    select(ExecutionProcess.id).where(
                        col(ExecutionProcess.session_id).in_(sess_ids)
                    )
                )
                exec_ids = [r[0] for r in exec_rows.all()]

            # Delete deepest children first: exec children
            if exec_ids:
                for model in (
                    ExecutionProcessRepoState,
                    ExecutionProcessLog,
                    CodingAgentTurn,
                ):
                    await session.execute(
                        delete(model).where(col(model.execution_process_id).in_(exec_ids))
                    )

                # Delete execution processes
                await session.execute(
                    delete(ExecutionProcess).where(col(ExecutionProcess.id).in_(exec_ids))
                )

            # Delete sessions
            if sess_ids:
                await session.execute(delete(Session).where(col(Session.id).in_(sess_ids)))

            # Clean up worktree/workspace paths, then delete
            if ws_ids:
                wr_rows = await session.execute(
                    select(WorkspaceRepo).where(col(WorkspaceRepo.workspace_id).in_(ws_ids))
                )
                for (wr,) in wr_rows.all():
                    if wr.worktree_path:
                        shutil.rmtree(wr.worktree_path, ignore_errors=True)

                w_rows = await session.execute(
                    select(Workspace).where(col(Workspace.id).in_(ws_ids))
                )
                for (w,) in w_rows.all():
                    if w.path and Path(w.path).exists():
                        shutil.rmtree(w.path, ignore_errors=True)

                await session.execute(delete(Merge).where(col(Merge.workspace_id).in_(ws_ids)))
                await session.execute(
                    delete(WorkspaceRepo).where(col(WorkspaceRepo.workspace_id).in_(ws_ids))
                )
                await session.execute(delete(Workspace).where(col(Workspace.id).in_(ws_ids)))

            # Collect task IDs
            task_rows = await session.execute(
                select(Task.id).where(col(Task.project_id) == project_id)
            )
            task_ids = [r[0] for r in task_rows.all()]

            if task_ids:
                # Delete task children
                await session.execute(
                    delete(TaskLink).where(
                        col(TaskLink.task_id).in_(task_ids)
                        | col(TaskLink.ref_task_id).in_(task_ids)
                    )
                )
                await session.execute(delete(TaskTag).where(col(TaskTag.task_id).in_(task_ids)))
                await session.execute(delete(Image).where(col(Image.task_id).in_(task_ids)))
                # Nullify self-referential parent_id
                await session.execute(
                    update(Task)
                    .where(col(Task.project_id) == project_id)
                    .where(col(Task.parent_id).isnot(None))
                    .values(parent_id=None)
                )
                # Delete tasks
                await session.execute(delete(Task).where(col(Task.project_id) == project_id))

            # Delete project repos and project
            await session.execute(
                delete(ProjectRepo).where(col(ProjectRepo.project_id) == project_id)
            )
            await session.execute(delete(Project).where(col(Project.id) == project_id))

            await session.commit()
            return project_name
    finally:
        await engine.dispose()


def _reset_all(existing_dirs: list[tuple[str, Path]], force: bool) -> None:
    """Nuclear reset: delete all Kagan directories."""
    click.echo()
    click.secho(
        "WARNING: This will permanently delete the following:",
        fg="red",
        bold=True,
    )
    click.echo()

    total_size = 0
    for name, path in existing_dirs:
        dir_size = sum(f.stat().st_size for f in path.rglob("*") if f.is_file())
        total_size += dir_size
        size_str = _format_size(dir_size)

        click.echo(f"  {click.style('•', fg='red')} {name}: {click.style(str(path), fg='cyan')}")
        click.echo(f"    Size: {size_str}")

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

    if not force:
        confirmed = click.prompt(
            click.style("Type 'yes' to confirm deletion", fg="yellow"),
            default="",
            show_default=False,
        )
        if confirmed.lower() != "yes":
            click.secho("Reset cancelled.", fg="green")
            return

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
        click.secho(
            "Reset complete. All Kagan data has been removed.",
            fg="green",
            bold=True,
        )


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

    Without --force, presents a menu to reset all or a specific project.
    Use --force to skip prompts and do a full nuclear reset.
    """

    dirs_to_remove: list[tuple[str, Path]] = [
        ("Config directory", get_config_dir()),
        ("Data directory", get_data_dir()),
        ("Cache directory", get_cache_dir()),
        ("Worktree directory", get_worktree_base_dir()),
    ]

    seen_paths: set[Path] = set()
    existing_dirs: list[tuple[str, Path]] = []
    for name, path in dirs_to_remove:
        if path.exists() and path not in seen_paths:
            existing_dirs.append((name, path))
            seen_paths.add(path)

    if not existing_dirs:
        click.secho("Nothing to reset - no Kagan directories found.", fg="yellow")
        return

    # --force: nuclear reset, no menu
    if force:
        _reset_all(existing_dirs, force=True)
        return

    # Try to load projects from DB for interactive menu
    db_file = Path(DEFAULT_DB_PATH)
    projects: list[tuple[str, str]] = []
    if db_file.exists():
        import contextlib

        with contextlib.suppress(Exception):
            projects = asyncio.run(_get_projects(DEFAULT_DB_PATH))

    if not projects:
        # No DB or no projects — fall through to nuclear reset
        _reset_all(existing_dirs, force=False)
        return

    # Interactive menu
    click.echo()
    click.secho("What would you like to reset?", bold=True)
    click.echo()
    click.echo(f"  {click.style('1', fg='cyan', bold=True)}) Reset all (nuclear wipe)")
    for i, (pid, pname) in enumerate(projects, start=2):
        click.echo(f"  {click.style(str(i), fg='cyan', bold=True)}) Project: {pname} (id: {pid})")
    click.echo()

    choice = click.prompt(
        "Select option",
        type=click.IntRange(1, len(projects) + 1),
        default=1,
    )

    if choice == 1:
        _reset_all(existing_dirs, force=False)
    else:
        project_id, project_name = projects[choice - 2]
        click.echo()
        confirmed = click.prompt(
            click.style(
                f"Type 'yes' to delete project '{project_name}'",
                fg="yellow",
            ),
            default="",
            show_default=False,
        )
        if confirmed.lower() != "yes":
            click.secho("Reset cancelled.", fg="green")
            return

        click.echo()
        click.echo(f"Deleting project '{project_name}'...")
        try:
            asyncio.run(_delete_project_data(DEFAULT_DB_PATH, project_id))
            click.secho(
                f"Project '{project_name}' has been removed.",
                fg="green",
                bold=True,
            )
        except Exception as e:
            click.secho(f"Failed to delete project: {e}", fg="red")


async def _list_projects_data(
    db_path: str,
) -> list[dict[str, object]]:
    """Fetch project data with repos and task counts for display."""
    from sqlalchemy.ext.asyncio import async_sessionmaker
    from sqlmodel import col, func, select

    from kagan.adapters.db.engine import create_db_engine
    from kagan.adapters.db.schema import Project, ProjectRepo, Repo, Task
    from kagan.core.models.enums import TaskStatus

    engine = await create_db_engine(db_path)
    session_factory = async_sessionmaker(engine, expire_on_commit=False)
    try:
        async with session_factory() as session:
            # Fetch projects
            proj_result = await session.execute(select(Project).order_by(Project.name))
            projects = proj_result.scalars().all()
            if not projects:
                return []

            output: list[dict[str, object]] = []
            for proj in projects:
                # Task counts by status
                counts: dict[str, int] = {}
                for status in (
                    TaskStatus.BACKLOG,
                    TaskStatus.IN_PROGRESS,
                    TaskStatus.REVIEW,
                    TaskStatus.DONE,
                ):
                    cnt_result = await session.execute(
                        select(func.count(col(Task.id))).where(
                            col(Task.project_id) == proj.id,
                            col(Task.status) == status,
                        )
                    )
                    counts[status.value] = cnt_result.scalar_one()

                # Repos via ProjectRepo
                repo_rows = await session.execute(
                    select(Repo.path, ProjectRepo.is_primary)
                    .join(
                        ProjectRepo,
                        col(ProjectRepo.repo_id) == col(Repo.id),
                    )
                    .where(col(ProjectRepo.project_id) == proj.id)
                    .order_by(col(ProjectRepo.display_order))
                )
                repos = [{"path": r[0], "is_primary": r[1]} for r in repo_rows.all()]

                output.append(
                    {
                        "id": proj.id,
                        "name": proj.name,
                        "created_at": proj.created_at,
                        "last_opened_at": proj.last_opened_at,
                        "counts": counts,
                        "repos": repos,
                    }
                )
            return output
    finally:
        await engine.dispose()


@cli.command(name="list")
def list_cmd() -> None:
    """List all projects and their associated repos."""
    db_file = Path(DEFAULT_DB_PATH)
    if not db_file.exists():
        click.secho("No projects found.", fg="yellow")
        return

    try:
        projects = asyncio.run(_list_projects_data(DEFAULT_DB_PATH))
    except Exception as e:
        click.secho(f"Failed to read database: {e}", fg="red")
        return

    if not projects:
        click.secho("No projects found.", fg="yellow")
        return

    from kagan.core.models.enums import TaskStatus

    click.echo()
    click.secho("Projects:", bold=True)

    for proj in projects:
        click.echo(f"  \U0001f4c1 {click.style(str(proj['name']), bold=True)} (id: {proj['id']})")

        created = str(proj["created_at"])[:10] if proj["created_at"] else "?"
        opened = str(proj["last_opened_at"])[:10] if proj["last_opened_at"] else "never"
        click.echo(f"  \u2502  Created: {created} | Last opened: {opened}")

        counts = proj["counts"]
        assert isinstance(counts, dict)
        click.echo(
            f"  \u2502  Tasks: "
            f"{counts.get(TaskStatus.BACKLOG.value, 0)} backlog, "
            f"{counts.get(TaskStatus.IN_PROGRESS.value, 0)} in progress, "
            f"{counts.get(TaskStatus.REVIEW.value, 0)} review, "
            f"{counts.get(TaskStatus.DONE.value, 0)} done"
        )

        repos = proj["repos"]
        assert isinstance(repos, list)
        for i, repo in enumerate(repos):
            is_last = i == len(repos) - 1
            prefix = "\u2514\u2500\u2500" if is_last else "\u251c\u2500\u2500"
            primary = " (primary)" if repo["is_primary"] else ""
            click.echo(f"  {prefix} \U0001f4c2 {repo['path']}{primary}")

        click.echo()


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
    (get_parallel_tasks, get_task).
    """
    from kagan.mcp.server import main as mcp_main

    mcp_main(readonly=readonly)


if __name__ == "__main__":
    cli()
