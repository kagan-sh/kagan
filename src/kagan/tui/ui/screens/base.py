"""Base screen class for Kagan screens."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, cast

from textual.screen import Screen

from kagan.core.git_utils import get_current_branch

if TYPE_CHECKING:
    from kagan.core.adapters.db.schema import Project, Repo
    from kagan.core.bootstrap import AppContext
    from kagan.core.ipc.client import IPCClient
    from kagan.tui.app import KaganApp
    from kagan.tui.ui.widgets.header import KaganHeader


class KaganScreen(Screen):
    @property
    def kagan_app(self) -> KaganApp:
        """Get the typed KaganApp instance."""
        return cast(
            "KaganApp",
            self.app,
        )  # cast-justified: smoke/snapshot harnesses run Screen under generic App subclasses.

    @property
    def core_client(self) -> IPCClient | None:
        """Get the core IPC client, if the app is attached to a running core."""
        app = self.kagan_app
        return getattr(app, "_core_client", None)

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

        tasks = await self.ctx.api.list_tasks(project_id=self.ctx.active_project_id)
        header.update_count(len(tasks))
        header.update_agent_from_config(self.kagan_app.config)
        header.update_core_status(self.kagan_app._core_status)

    async def _get_active_project(self) -> Project | None:
        api = self.ctx.api
        active_project_id = self.ctx.active_project_id
        if active_project_id is not None:
            return await api.get_project(active_project_id)
        project_root = self.kagan_app.project_root
        return await api.find_project_by_repo_path(str(project_root))

    async def _get_active_repo_name(self, project: Project) -> str | None:
        repos = await self.ctx.api.get_project_repos(project.id)
        if not repos:
            return None

        repo = self._match_repo(repos, self.ctx.active_repo_id, self.kagan_app.project_root)
        if repo is None:
            repo = repos[0]
        return repo.display_name or repo.name

    async def auto_sync_branch(self, header: KaganHeader) -> None:
        """If auto_sync enabled and git branch changed, update Repo.default_branch."""
        config = self.kagan_app.config
        if not config.general.auto_sync_base_branch:
            return

        repo_id = self.ctx.active_repo_id
        if repo_id is None:
            return

        current_branch = await get_current_branch(self.kagan_app.project_root)
        if not current_branch or current_branch == header.git_branch:
            return

        header.update_branch(current_branch)
        await self.ctx.api.update_repo_default_branch(repo_id, current_branch)

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
