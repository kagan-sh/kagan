"""Project list command."""

from __future__ import annotations

import asyncio
from pathlib import Path

import click

from kagan.core.constants import DEFAULT_DB_PATH


async def _list_projects_data(
    db_path: str,
) -> list[dict[str, object]]:
    """Fetch project data with repos and task counts for display."""
    from sqlalchemy.ext.asyncio import async_sessionmaker
    from sqlmodel import col, func, select

    from kagan.core.adapters.db.engine import create_db_engine
    from kagan.core.adapters.db.schema import Project, ProjectRepo, Repo, Task
    from kagan.core.models.enums import TaskStatus

    engine = await create_db_engine(db_path)
    session_factory = async_sessionmaker(engine, expire_on_commit=False)
    try:
        async with session_factory() as session:
            proj_result = await session.execute(select(Project).order_by(Project.name))
            projects = proj_result.scalars().all()
            if not projects:
                return []

            output: list[dict[str, object]] = []
            for project in projects:
                counts: dict[str, int] = {}
                for status in (
                    TaskStatus.BACKLOG,
                    TaskStatus.IN_PROGRESS,
                    TaskStatus.REVIEW,
                    TaskStatus.DONE,
                ):
                    cnt_result = await session.execute(
                        select(func.count(col(Task.id))).where(
                            col(Task.project_id) == project.id,
                            col(Task.status) == status,
                        )
                    )
                    counts[status.value] = cnt_result.scalar_one()

                repo_rows = await session.execute(
                    select(Repo.path, ProjectRepo.is_primary)
                    .join(
                        ProjectRepo,
                        col(ProjectRepo.repo_id) == col(Repo.id),
                    )
                    .where(col(ProjectRepo.project_id) == project.id)
                    .order_by(col(ProjectRepo.display_order))
                )
                repos = [{"path": row[0], "is_primary": row[1]} for row in repo_rows.all()]

                output.append(
                    {
                        "id": project.id,
                        "name": project.name,
                        "created_at": project.created_at,
                        "last_opened_at": project.last_opened_at,
                        "counts": counts,
                        "repos": repos,
                    }
                )
            return output
    finally:
        await engine.dispose()


@click.command(name="list")
def list_cmd() -> None:
    """List all projects and their associated repos."""
    db_file = Path(DEFAULT_DB_PATH)
    if not db_file.exists():
        click.secho("No projects found.", fg="yellow")
        return

    try:
        projects = asyncio.run(_list_projects_data(DEFAULT_DB_PATH))
    except Exception as error:
        click.secho(f"Failed to read database: {error}", fg="red")
        return

    if not projects:
        click.secho("No projects found.", fg="yellow")
        return

    from kagan.core.models.enums import TaskStatus

    click.echo()
    click.secho("Projects:", bold=True)

    for project in projects:
        click.echo(
            f"  \U0001f4c1 {click.style(str(project['name']), bold=True)} (id: {project['id']})"
        )

        created = str(project["created_at"])[:10] if project["created_at"] else "?"
        opened = str(project["last_opened_at"])[:10] if project["last_opened_at"] else "never"
        click.echo(f"  \u2502  Created: {created} | Last opened: {opened}")

        counts = project["counts"]
        assert isinstance(counts, dict)
        click.echo(
            f"  \u2502  Tasks: "
            f"{counts.get(TaskStatus.BACKLOG.value, 0)} backlog, "
            f"{counts.get(TaskStatus.IN_PROGRESS.value, 0)} in progress, "
            f"{counts.get(TaskStatus.REVIEW.value, 0)} review, "
            f"{counts.get(TaskStatus.DONE.value, 0)} done"
        )

        repos = project["repos"]
        assert isinstance(repos, list)
        for i, repo in enumerate(repos):
            is_last = i == len(repos) - 1
            prefix = "\u2514\u2500\u2500" if is_last else "\u251c\u2500\u2500"
            primary = " (primary)" if repo["is_primary"] else ""
            click.echo(f"  {prefix} \U0001f4c2 {repo['path']}{primary}")

        click.echo()
