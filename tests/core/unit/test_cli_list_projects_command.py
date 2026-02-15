"""Behavior-focused tests for `kagan list` CLI output."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from click.testing import CliRunner
from sqlalchemy.ext.asyncio import async_sessionmaker

from kagan.cli.commands.list_projects import list_cmd
from kagan.core.adapters.db.engine import create_db_engine, create_db_tables
from kagan.core.adapters.db.schema import Project, ProjectRepo, Repo, Task
from kagan.core.models.enums import TaskStatus, TaskType

if TYPE_CHECKING:
    from pathlib import Path


async def _seed_project_for_list_output(db_path: Path) -> dict[str, object]:
    engine = await create_db_engine(db_path)
    await create_db_tables(engine)
    session_factory = async_sessionmaker(engine, expire_on_commit=False)

    primary_repo_path = "/tmp/demo-primary"
    secondary_repo_path = "/tmp/demo-secondary"

    async with session_factory() as session:
        project = Project(name="Demo Project", description="demo")
        session.add(project)
        await session.flush()

        primary_repo = Repo(name="primary", path=primary_repo_path, default_branch="main")
        secondary_repo = Repo(name="secondary", path=secondary_repo_path, default_branch="main")
        session.add_all([primary_repo, secondary_repo])
        await session.flush()

        session.add_all(
            [
                ProjectRepo(
                    project_id=project.id,
                    repo_id=primary_repo.id,
                    is_primary=True,
                    display_order=0,
                ),
                ProjectRepo(
                    project_id=project.id,
                    repo_id=secondary_repo.id,
                    is_primary=False,
                    display_order=1,
                ),
            ]
        )
        session.add_all(
            [
                Task.create(
                    title="Backlog",
                    status=TaskStatus.BACKLOG,
                    task_type=TaskType.PAIR,
                    project_id=project.id,
                ),
                Task.create(
                    title="In progress",
                    status=TaskStatus.IN_PROGRESS,
                    task_type=TaskType.PAIR,
                    project_id=project.id,
                ),
                Task.create(
                    title="Review",
                    status=TaskStatus.REVIEW,
                    task_type=TaskType.PAIR,
                    project_id=project.id,
                ),
                Task.create(
                    title="Done",
                    status=TaskStatus.DONE,
                    task_type=TaskType.PAIR,
                    project_id=project.id,
                ),
            ]
        )
        await session.commit()

    await engine.dispose()
    return {
        "primary_repo_path": primary_repo_path,
        "secondary_repo_path": secondary_repo_path,
    }


def test_list_projects_renders_status_counts_and_primary_repo_marker(
    tmp_path: Path,
    monkeypatch,
) -> None:
    db_path = tmp_path / "kagan.db"
    seeded = asyncio.run(_seed_project_for_list_output(db_path))
    primary_repo_path = str(seeded["primary_repo_path"])
    secondary_repo_path = str(seeded["secondary_repo_path"])
    monkeypatch.setattr("kagan.cli.commands.list_projects.DEFAULT_DB_PATH", str(db_path))

    runner = CliRunner()
    result = runner.invoke(list_cmd)

    assert result.exit_code == 0
    assert "Projects:" in result.output
    assert "Demo Project" in result.output
    assert "1 backlog, 1 in progress, 1 review, 1 done" in result.output
    assert f"{primary_repo_path} (primary)" in result.output
    assert secondary_repo_path in result.output
