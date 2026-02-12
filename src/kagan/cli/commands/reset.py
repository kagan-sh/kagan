"""Destructive reset command for Kagan data."""

from __future__ import annotations

import asyncio
import contextlib
import json
import os
import shutil
import signal
import time
from pathlib import Path

import click

from kagan.core.constants import DEFAULT_DB_PATH
from kagan.core.paths import (
    get_cache_dir,
    get_config_dir,
    get_core_runtime_dir,
    get_data_dir,
    get_worktree_base_dir,
)
from kagan.core.process_liveness import pid_exists


async def _get_projects(db_path: str) -> list[tuple[str, str]]:
    """Return list of (id, name) for all projects in the database."""
    from sqlalchemy.ext.asyncio import async_sessionmaker
    from sqlmodel import select

    from kagan.core.adapters.db.engine import create_db_engine
    from kagan.core.adapters.db.schema import Project

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

    from kagan.core.adapters.db.engine import create_db_engine
    from kagan.core.adapters.db.schema import (
        CodingAgentTurn,
        ExecutionProcess,
        ExecutionProcessLog,
        ExecutionProcessRepoState,
        Merge,
        Project,
        ProjectRepo,
        Session,
        Task,
        TaskLink,
        Workspace,
        WorkspaceRepo,
    )

    engine = await create_db_engine(db_path)
    session_factory = async_sessionmaker(engine, expire_on_commit=False)
    try:
        async with session_factory() as session:
            proj = await session.get(Project, project_id)
            if not proj:
                msg = f"Project {project_id} not found"
                raise ValueError(msg)
            project_name = proj.name

            ws_rows = await session.execute(
                select(Workspace.id).where(col(Workspace.project_id) == project_id)
            )
            ws_ids = [r[0] for r in ws_rows.all()]

            sess_rows = await session.execute(
                select(Session.id).where(col(Session.workspace_id).in_(ws_ids))
            )
            sess_ids = [r[0] for r in sess_rows.all()]

            exec_rows = await session.execute(
                select(ExecutionProcess.id).where(col(ExecutionProcess.session_id).in_(sess_ids))
            )
            exec_ids = [r[0] for r in exec_rows.all()]

            for model in (ExecutionProcessRepoState, ExecutionProcessLog, CodingAgentTurn):
                await session.execute(
                    delete(model).where(col(model.execution_process_id).in_(exec_ids))
                )

            await session.execute(
                delete(ExecutionProcess).where(col(ExecutionProcess.id).in_(exec_ids))
            )
            await session.execute(delete(Session).where(col(Session.id).in_(sess_ids)))

            wr_rows = await session.execute(
                select(WorkspaceRepo).where(col(WorkspaceRepo.workspace_id).in_(ws_ids))
            )
            for (wr,) in wr_rows.all():
                if wr.worktree_path:
                    shutil.rmtree(wr.worktree_path, ignore_errors=True)

            w_rows = await session.execute(select(Workspace).where(col(Workspace.id).in_(ws_ids)))
            for (workspace,) in w_rows.all():
                if workspace.path and Path(workspace.path).exists():
                    shutil.rmtree(workspace.path, ignore_errors=True)

            await session.execute(delete(Merge).where(col(Merge.workspace_id).in_(ws_ids)))
            await session.execute(
                delete(WorkspaceRepo).where(col(WorkspaceRepo.workspace_id).in_(ws_ids))
            )
            await session.execute(delete(Workspace).where(col(Workspace.id).in_(ws_ids)))

            task_rows = await session.execute(
                select(Task.id).where(col(Task.project_id) == project_id)
            )
            task_ids = [r[0] for r in task_rows.all()]

            await session.execute(
                delete(TaskLink).where(
                    col(TaskLink.task_id).in_(task_ids) | col(TaskLink.ref_task_id).in_(task_ids)
                )
            )
            await session.execute(
                update(Task)
                .where(col(Task.project_id) == project_id)
                .where(col(Task.parent_id).isnot(None))
                .values(parent_id=None)
            )
            await session.execute(delete(Task).where(col(Task.project_id) == project_id))

            await session.execute(
                delete(ProjectRepo).where(col(ProjectRepo.project_id) == project_id)
            )
            await session.execute(delete(Project).where(col(Project.id) == project_id))

            await session.commit()
            return project_name
    finally:
        await engine.dispose()


def _format_size(size_bytes: int | float) -> str:
    """Format byte size to human-readable string."""
    size = float(size_bytes)
    for unit in ["B", "KB", "MB", "GB"]:
        if size < 1024:
            return f"{size:.1f} {unit}"
        size /= 1024
    return f"{size:.1f} TB"


def _read_pid(path: Path) -> int | None:
    try:
        raw = path.read_text(encoding="utf-8").strip()
    except OSError:
        return None
    if not raw:
        return None
    try:
        return int(raw)
    except ValueError:
        return None


def _pid_exists(pid: int) -> bool:
    return pid_exists(pid)


def _read_lease_owner_pid(path: Path) -> int | None:
    try:
        lease_data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError, TypeError, ValueError):
        return None
    if not isinstance(lease_data, dict):
        return None

    raw_owner_pid = lease_data.get("owner_pid")
    if isinstance(raw_owner_pid, int):
        return raw_owner_pid
    if isinstance(raw_owner_pid, str):
        with contextlib.suppress(ValueError):
            return int(raw_owner_pid)
    return None


def _stop_core_before_reset() -> None:
    """Stop running core process before deleting runtime/data directories."""
    lock_path = get_core_runtime_dir() / "core.instance.lock"
    lease_path = get_core_runtime_dir() / "core.lease.json"
    lease_pid = _read_lease_owner_pid(lease_path)
    pids = {
        pid for path in (lock_path,) if (pid := _read_pid(path)) is not None and _pid_exists(pid)
    }
    if lease_pid is not None and _pid_exists(lease_pid):
        pids.add(lease_pid)
    if not pids:
        return

    click.echo("Stopping running core before reset...")
    for pid in pids:
        with contextlib.suppress(ProcessLookupError, PermissionError):
            os.kill(pid, signal.SIGTERM)

    deadline = time.time() + 3.0
    while time.time() < deadline:
        if not any(_pid_exists(pid) for pid in pids):
            return
        time.sleep(0.1)

    for pid in pids:
        with contextlib.suppress(ProcessLookupError, PermissionError):
            os.kill(pid, signal.SIGKILL)


def _reset_all(existing_dirs: list[tuple[str, Path]], force: bool) -> None:
    """Nuclear reset: delete all Kagan directories."""
    _stop_core_before_reset()
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
            for file_path in key_files:
                click.echo(f"      - {file_path.name}")
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

    errors: list[tuple[str, Path, OSError]] = []
    for name, path in existing_dirs:
        try:
            shutil.rmtree(path)
            click.echo(f"  {click.style('✓', fg='green')} Removed {name}: {path}")
        except OSError as error:
            errors.append((name, path, error))
            click.echo(f"  {click.style('✗', fg='red')} Failed to remove {name}: {error}")

    click.echo()
    if errors:
        click.secho(f"Reset completed with {len(errors)} error(s).", fg="yellow")
    else:
        click.secho(
            "Reset complete. All Kagan data has been removed.",
            fg="green",
            bold=True,
        )


def _prompt_reset_choice(projects: list[tuple[str, str]]) -> int:
    click.echo()
    click.secho("What would you like to reset?", bold=True)
    click.echo()
    click.echo(f"  {click.style('1', fg='cyan', bold=True)}) Reset all (nuclear wipe)")
    for i, (project_id, project_name) in enumerate(projects, start=2):
        click.echo(
            f"  {click.style(str(i), fg='cyan', bold=True)}) "
            f"Project: {project_name} (id: {project_id})"
        )
    click.echo()
    return click.prompt(
        "Select option",
        type=click.IntRange(1, len(projects) + 1),
        default=1,
    )


@click.command()
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

    if force:
        _reset_all(existing_dirs, force=True)
        return

    db_file = Path(DEFAULT_DB_PATH)
    projects: list[tuple[str, str]] = []
    if db_file.exists():
        with contextlib.suppress(Exception):
            projects = asyncio.run(_get_projects(DEFAULT_DB_PATH))

    if not projects:
        _reset_all(existing_dirs, force=False)
        return

    choice = _prompt_reset_choice(projects)
    if choice == 1:
        _reset_all(existing_dirs, force=False)
        return

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
    except Exception as error:
        click.secho(f"Failed to delete project: {error}", fg="red")
