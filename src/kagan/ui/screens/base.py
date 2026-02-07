"""Base screen class for Kagan screens."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, cast

from textual.screen import Screen

if TYPE_CHECKING:
    from kagan.adapters.db.schema import Project, Repo
    from kagan.app import KaganApp
    from kagan.bootstrap import AppContext
    from kagan.ui.widgets.header import KaganHeader


class KaganScreen(Screen):
    @property
    def kagan_app(self) -> KaganApp:
        """Get the typed KaganApp instance."""
        return cast("KaganApp", self.app)

    @property
    def ctx(self) -> AppContext:
        """Get the application context for service access.

        Raises:
            RuntimeError: If AppContext is not initialized on the app.
        """
        app = self.kagan_app
        if not hasattr(app, "_ctx") or app._ctx is None:
            msg = "AppContext not initialized. Ensure bootstrap has completed."
            raise RuntimeError(msg)
        return app._ctx

    async def sync_header_context(self, header: KaganHeader) -> None:
        """Sync header with active project and repo."""
        project = await self._get_active_project()
        if project is None:
            header.update_project(Path(self.kagan_app.project_root).name)
            header.update_repo("")
        else:
            header.update_project(project.name)
            repo_name = await self._get_active_repo_name(project)
            header.update_repo(repo_name or "")

        tasks = await self.ctx.task_service.list_tasks(project_id=self.ctx.active_project_id)
        header.update_count(len(tasks))
        header.update_agent_from_config(self.kagan_app.config)

    async def _get_active_project(self) -> Project | None:
        project_service = self.ctx.project_service
        active_project_id = self.ctx.active_project_id
        if active_project_id is not None:
            return await project_service.get_project(active_project_id)
        project_root = self.kagan_app.project_root
        return await project_service.find_project_by_repo_path(str(project_root))

    async def _get_active_repo_name(self, project: Project) -> str | None:
        repos = await self.ctx.project_service.get_project_repos(project.id)
        if not repos:
            return None

        repo = self._match_repo(repos, self.ctx.active_repo_id, self.kagan_app.project_root)
        if repo is None:
            repo = repos[0]
        return repo.display_name or repo.name

    @staticmethod
    def _match_repo(
        repos: list[Repo],
        active_repo_id: str | None,
        project_root: Path,
    ) -> Repo | None:
        if active_repo_id:
            for repo in repos:
                if repo.id == active_repo_id:
                    return repo

        resolved_root = project_root.resolve()
        for repo in repos:
            if Path(repo.path).resolve() == resolved_root:
                return repo
        return None
