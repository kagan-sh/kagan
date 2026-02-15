"""Project and settings API mixin.

Contains project/repo management, settings, audit, and instrumentation methods.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from kagan.core.config import KaganConfig
from kagan.core.expose import expose
from kagan.core.instrumentation import snapshot as instrumentation_snapshot
from kagan.core.settings import exposed_settings_snapshot, normalize_settings_updates

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.core.adapters.db.schema import AuditEvent, Project, Repo
    from kagan.core.bootstrap import AppContext


class ProjectApiMixin:
    """Mixin providing project, settings, and audit API methods.

    Expects ``self._ctx`` to be an :class:`AppContext` instance,
    initialised by :class:`KaganAPI.__init__`.
    """

    _ctx: AppContext

    # ── Projects ───────────────────────────────────────────────────────

    @expose(
        "projects",
        "open",
        profile="maintainer",
        mutating=True,
        description="Open/switch to a project.",
    )
    async def open_project(self, project_id: str) -> Project:
        """Open/switch to a project."""
        return await self._ctx.project_service.open_project(project_id)

    @expose(
        "projects",
        "create",
        profile="maintainer",
        mutating=True,
        description="Create a new project with optional repositories.",
    )
    async def create_project(
        self,
        name: str,
        *,
        description: str = "",
        repo_paths: list[str | Path] | None = None,
    ) -> str:
        """Create a project and optionally attach repositories.

        Returns:
            The project ID.

        Raises:
            ValueError: If name is empty or repo_paths is not a list.
        """
        name = name.strip()
        if not name:
            msg = "Project name cannot be empty"
            raise ValueError(msg)

        if repo_paths is not None:
            if not isinstance(repo_paths, list):
                msg = "repo_paths must be a list of repository paths"
                raise ValueError(msg)
            repo_paths = [str(p).strip() for p in repo_paths if str(p).strip()]

        return await self._ctx.project_service.create_project(
            name=name, repo_paths=repo_paths, description=description
        )

    @expose(
        "projects",
        "add_repo",
        profile="maintainer",
        mutating=True,
        description="Add a repository to a project.",
    )
    async def add_repo(
        self,
        project_id: str,
        repo_path: str | Path,
        *,
        is_primary: bool = False,
    ) -> str:
        """Add a repository to a project.

        Returns:
            The repo ID.

        Raises:
            ValueError: If project_id or repo_path is empty.
        """
        project_id = str(project_id).strip()
        if not project_id:
            raise ValueError("project_id cannot be empty")
        repo_path_str = str(repo_path).strip()
        if not repo_path_str:
            raise ValueError("repo_path cannot be empty")
        return await self._ctx.project_service.add_repo_to_project(
            project_id=project_id, repo_path=repo_path_str, is_primary=is_primary
        )

    @expose("projects", "get", description="Get a project by ID.")
    async def get_project(self, project_id: str) -> Project | None:
        """Get a project by ID."""
        return await self._ctx.project_service.get_project(project_id)

    @expose("projects", "list", description="List recent projects.")
    async def list_projects(self, *, limit: int = 10) -> list[Project]:
        """List recent projects."""
        return await self._ctx.project_service.list_recent_projects(limit=limit)

    @expose("projects", "repos", description="Get all repos for a project.")
    async def get_project_repos(self, project_id: str) -> list[Repo]:
        """Get all repos for a project."""
        return await self._ctx.project_service.get_project_repos(project_id)

    async def get_project_repo_details(self, project_id: str) -> list[dict]:
        """Get repos with junction metadata for a project."""
        return await self._ctx.project_service.get_project_repo_details(project_id)

    @expose(
        "projects",
        "find_by_repo_path",
        description="Find a project containing the given repository path.",
    )
    async def find_project_by_repo_path(self, repo_path: str | Path) -> Project | None:
        """Find a project containing the given repository path."""
        return await self._ctx.project_service.find_project_by_repo_path(repo_path)

    async def update_repo_default_branch(
        self,
        repo_id: str,
        branch: str,
        *,
        mark_configured: bool = False,
    ) -> Repo | None:
        """Update Repo.default_branch, optionally marking branch as configured."""
        return await self._ctx.project_service.update_repo_default_branch(
            repo_id, branch, mark_configured=mark_configured
        )

    # ── Settings & Audit ───────────────────────────────────────────────

    @expose("settings", "get", profile="maintainer", description="Get admin-exposed settings.")
    async def get_settings(self) -> dict[str, object]:
        """Get MCP-exposed settings snapshot."""
        return exposed_settings_snapshot(self._ctx.config)

    @expose(
        "settings",
        "update",
        profile="maintainer",
        mutating=True,
        description="Update allowlisted settings fields.",
    )
    async def update_settings(
        self, fields: dict[str, object]
    ) -> tuple[bool, str, dict[str, object]]:
        """Update allowlisted settings fields.

        Returns:
            Tuple of (success, message, updated_fields).
        """
        if not fields:
            return False, "fields must be a non-empty object", {}

        try:
            updates = normalize_settings_updates(fields)
        except ValueError as exc:
            return False, str(exc), {}

        config_data = self._ctx.config.model_dump(mode="python")
        for key, value in updates.items():
            section, field = key.split(".", 1)
            section_data = config_data.get(section)
            if not isinstance(section_data, dict):
                return False, f"Invalid settings section: {section}", {}
            section_data[field] = value

        try:
            next_config = KaganConfig.model_validate(config_data)
        except Exception as exc:
            return False, f"Invalid settings update: {exc}", {}

        await next_config.save(self._ctx.config_path)
        self._ctx.config = next_config
        return True, "Settings updated", updates

    @expose("audit", "list", description="List recent audit events.")
    async def list_audit_events(
        self,
        *,
        capability: str | None = None,
        limit: int = 50,
        cursor: str | None = None,
    ) -> list[AuditEvent]:
        """List audit events with optional filtering."""
        return await self._ctx.audit_repository.list_events(
            capability=capability, limit=limit, cursor=cursor
        )

    @expose(
        "diagnostics",
        "instrumentation",
        profile="maintainer",
        description="Return in-memory instrumentation aggregates.",
    )
    async def get_instrumentation(self) -> dict[str, Any]:
        """Return in-memory instrumentation aggregates."""
        return instrumentation_snapshot()
