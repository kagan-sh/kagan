"""Behavior-focused tests for `kagan reset` safety guarantees."""

from __future__ import annotations

import asyncio
import os
from pathlib import Path
import shutil
import signal
from typing import TYPE_CHECKING
from unittest.mock import call

from click.testing import CliRunner
from sqlalchemy.ext.asyncio import async_sessionmaker
from sqlmodel import select

import kagan.cli.commands.reset as reset_module
from kagan.cli.commands.reset import reset
from kagan.core.adapters.db.engine import create_db_engine, create_db_tables
from kagan.core.adapters.db.schema import Project, Task, Workspace
from kagan.core.models.enums import TaskStatus, TaskType, WorkspaceStatus


async def _seed_two_projects(db_path: Path, worktree_root: Path) -> dict[str, object]:
    engine = await create_db_engine(db_path)
    await create_db_tables(engine)
    session_factory = async_sessionmaker(engine, expire_on_commit=False)

    alpha_workspace = worktree_root / "alpha-workspace"
    beta_workspace = worktree_root / "beta-workspace"
    alpha_workspace.mkdir(parents=True, exist_ok=True)
    beta_workspace.mkdir(parents=True, exist_ok=True)
    (alpha_workspace / "artifact.txt").write_text("alpha", encoding="utf-8")
    (beta_workspace / "artifact.txt").write_text("beta", encoding="utf-8")

    async with session_factory() as session:
        alpha = Project(name="Alpha", description="alpha")
        beta = Project(name="Beta", description="beta")
        session.add_all([alpha, beta])
        await session.flush()

        alpha_task = Task.create(
            title="Alpha task",
            description="",
            status=TaskStatus.BACKLOG,
            task_type=TaskType.PAIR,
            project_id=alpha.id,
        )
        beta_task = Task.create(
            title="Beta task",
            description="",
            status=TaskStatus.BACKLOG,
            task_type=TaskType.PAIR,
            project_id=beta.id,
        )
        session.add_all([alpha_task, beta_task])
        await session.flush()

        session.add_all(
            [
                Workspace(
                    project_id=alpha.id,
                    task_id=alpha_task.id,
                    branch_name="feature/alpha",
                    path=str(alpha_workspace),
                    status=WorkspaceStatus.ACTIVE,
                ),
                Workspace(
                    project_id=beta.id,
                    task_id=beta_task.id,
                    branch_name="feature/beta",
                    path=str(beta_workspace),
                    status=WorkspaceStatus.ACTIVE,
                ),
            ]
        )
        await session.commit()

        payload = {
            "alpha_id": alpha.id,
            "alpha_name": alpha.name,
            "beta_id": beta.id,
            "beta_name": beta.name,
            "alpha_workspace": alpha_workspace,
            "beta_workspace": beta_workspace,
        }

    await engine.dispose()
    return payload


async def _read_state(db_path: Path) -> dict[str, object]:
    engine = await create_db_engine(db_path)
    session_factory = async_sessionmaker(engine, expire_on_commit=False)
    async with session_factory() as session:
        project_rows = (await session.execute(select(Project.id, Project.name))).all()
        task_rows = (await session.execute(select(Task.project_id))).all()
        workspace_rows = (await session.execute(select(Workspace.project_id, Workspace.path))).all()

    await engine.dispose()
    return {
        "projects": {project_id: name for project_id, name in project_rows},
        "task_project_ids": sorted(row[0] for row in task_rows),
        "workspace_projects": sorted(row[0] for row in workspace_rows),
        "workspace_paths": sorted(row[1] for row in workspace_rows),
    }


def _patch_runtime_dirs(monkeypatch, *, root: Path) -> dict[str, Path]:
    config_dir = root / "config"
    data_dir = root / "data"
    cache_dir = root / "cache"
    worktree_dir = root / "worktrees"
    for path in (config_dir, data_dir, cache_dir, worktree_dir):
        path.mkdir(parents=True, exist_ok=True)

    monkeypatch.setattr("kagan.cli.commands.reset.get_config_dir", lambda: config_dir)
    monkeypatch.setattr("kagan.cli.commands.reset.get_data_dir", lambda: data_dir)
    monkeypatch.setattr("kagan.cli.commands.reset.get_cache_dir", lambda: cache_dir)
    monkeypatch.setattr("kagan.cli.commands.reset.get_worktree_base_dir", lambda: worktree_dir)
    return {
        "config": config_dir,
        "data": data_dir,
        "cache": cache_dir,
        "worktrees": worktree_dir,
    }


def test_reset_project_selection_deletes_only_selected_project(
    tmp_path: Path,
    monkeypatch,
) -> None:
    runtime_dirs = _patch_runtime_dirs(monkeypatch, root=tmp_path)
    db_path = runtime_dirs["data"] / "kagan.db"
    seeded = asyncio.run(_seed_two_projects(db_path, runtime_dirs["worktrees"]))
    monkeypatch.setattr("kagan.cli.commands.reset.DEFAULT_DB_PATH", str(db_path))

    runner = CliRunner()
    # Menu index: 1=reset all, 2=Alpha (sorted by name), 3=Beta.
    result = runner.invoke(reset, input="2\nyes\n")

    assert result.exit_code == 0
    assert "Project 'Alpha' has been removed." in result.output

    state = asyncio.run(_read_state(db_path))
    projects = state["projects"]
    assert seeded["alpha_id"] not in projects
    assert projects.get(seeded["beta_id"]) == seeded["beta_name"]
    assert seeded["alpha_id"] not in state["task_project_ids"]
    assert seeded["alpha_id"] not in state["workspace_projects"]
    assert seeded["beta_id"] in state["task_project_ids"]
    assert seeded["beta_id"] in state["workspace_projects"]
    assert not seeded["alpha_workspace"].exists()
    assert seeded["beta_workspace"].exists()


def test_reset_cancelled_does_not_delete_any_data(
    tmp_path: Path,
    monkeypatch,
) -> None:
    runtime_dirs = _patch_runtime_dirs(monkeypatch, root=tmp_path)
    db_path = runtime_dirs["data"] / "kagan.db"
    seeded = asyncio.run(_seed_two_projects(db_path, runtime_dirs["worktrees"]))
    monkeypatch.setattr("kagan.cli.commands.reset.DEFAULT_DB_PATH", str(db_path))

    before = asyncio.run(_read_state(db_path))
    runner = CliRunner()
    result = runner.invoke(reset, input="2\nno\n")
    after = asyncio.run(_read_state(db_path))

    assert result.exit_code == 0
    assert "Reset cancelled." in result.output
    assert before == after
    assert seeded["alpha_workspace"].exists()
    assert seeded["beta_workspace"].exists()


def test_reset_force_full_wipe_removes_all_known_dirs_and_reports_success(
    tmp_path: Path,
    monkeypatch,
) -> None:
    runtime_dirs = _patch_runtime_dirs(monkeypatch, root=tmp_path)
    monkeypatch.setattr("kagan.cli.commands.reset._stop_core_before_reset", lambda: None)

    for path in runtime_dirs.values():
        (path / "artifact.txt").write_text("payload", encoding="utf-8")

    runner = CliRunner()
    result = runner.invoke(reset, ["--force"])

    assert result.exit_code == 0
    assert "Reset complete. All Kagan data has been removed." in result.output
    for path in runtime_dirs.values():
        assert not path.exists()


def test_reset_all_cancelled_after_prompt_is_no_op(
    tmp_path: Path,
    monkeypatch,
) -> None:
    runtime_dirs = _patch_runtime_dirs(monkeypatch, root=tmp_path)
    monkeypatch.setattr("kagan.cli.commands.reset._stop_core_before_reset", lambda: None)
    monkeypatch.setattr("kagan.cli.commands.reset.DEFAULT_DB_PATH", str(tmp_path / "missing.db"))

    for path in runtime_dirs.values():
        (path / "artifact.txt").write_text("payload", encoding="utf-8")

    runner = CliRunner()
    result = runner.invoke(reset, input="no\n")

    assert result.exit_code == 0
    assert "Reset cancelled." in result.output
    for path in runtime_dirs.values():
        assert path.exists()
        assert (path / "artifact.txt").exists()


def test_reset_all_reports_partial_delete_errors_without_false_success(
    tmp_path: Path,
    monkeypatch,
) -> None:
    runtime_dirs = _patch_runtime_dirs(monkeypatch, root=tmp_path)
    monkeypatch.setattr("kagan.cli.commands.reset._stop_core_before_reset", lambda: None)

    for path in runtime_dirs.values():
        (path / "artifact.txt").write_text("payload", encoding="utf-8")

    failing_path = runtime_dirs["cache"]
    original_rmtree = shutil.rmtree

    def flaky_rmtree(path, *args, **kwargs):
        if Path(path) == failing_path:
            raise OSError("simulated delete failure")
        return original_rmtree(path, *args, **kwargs)

    monkeypatch.setattr("kagan.cli.commands.reset.shutil.rmtree", flaky_rmtree)

    runner = CliRunner()
    result = runner.invoke(reset, ["--force"])

    assert result.exit_code == 0
    assert "Reset completed with 1 error(s)." in result.output
    assert "Reset complete. All Kagan data has been removed." not in result.output
    assert failing_path.exists()
    assert not runtime_dirs["config"].exists()
    assert not runtime_dirs["data"].exists()
    assert not runtime_dirs["worktrees"].exists()


def test_reset_stops_core_and_escalates_to_sigkill_after_timeout(
    monkeypatch, tmp_path: Path
) -> None:
    runtime_dir = tmp_path / "runtime"
    runtime_dir.mkdir(parents=True, exist_ok=True)

    monkeypatch.setattr(reset_module, "get_core_runtime_dir", lambda: runtime_dir)
    monkeypatch.setattr(reset_module, "_read_pid", lambda _path: 1111)
    monkeypatch.setattr(reset_module, "_read_lease_owner_pid", lambda _path: 2222)
    monkeypatch.setattr(reset_module, "_pid_exists", lambda _pid: True)

    kills: list[call] = []

    def fake_kill(pid: int, sig: int) -> None:
        kills.append(call(pid, sig))

    times = iter([100.0, 100.0, 100.5, 101.5, 102.5, 103.1])
    monkeypatch.setattr(reset_module.time, "time", lambda: next(times))
    monkeypatch.setattr(reset_module.time, "sleep", lambda _seconds: None)
    monkeypatch.setattr(reset_module.os, "kill", fake_kill)

    reset_module._stop_core_before_reset()

    sigterm_calls = {
        (item.args[0], item.args[1]) for item in kills if item.args[1] == signal.SIGTERM
    }
    sigkill_calls = {
        (item.args[0], item.args[1]) for item in kills if item.args[1] == signal.SIGKILL
    }
    assert sigterm_calls == {(1111, signal.SIGTERM), (2222, signal.SIGTERM)}
    assert sigkill_calls == {(1111, signal.SIGKILL), (2222, signal.SIGKILL)}
