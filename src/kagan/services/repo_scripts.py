"""Service for executing per-repo scripts."""

from __future__ import annotations

import asyncio
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path
from typing import TYPE_CHECKING, Protocol

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession

    from kagan.adapters.db.repositories import ClosingAwareSessionFactory
    from kagan.adapters.db.schema import Repo
    from kagan.core.events import EventBus
    from kagan.services.workspaces import WorkspaceService


class ScriptType(StrEnum):
    """Types of repo scripts."""

    SETUP = "setup"
    CLEANUP = "cleanup"
    DEV_SERVER = "dev_server"


@dataclass
class ScriptResult:
    """Result of running a script."""

    repo_id: str
    repo_name: str
    script_type: ScriptType
    success: bool
    exit_code: int
    stdout: str
    stderr: str


class RepoScriptService(Protocol):
    """Service for running repo scripts."""

    async def run_script(
        self,
        workspace_id: str,
        repo_id: str,
        script_type: ScriptType,
    ) -> ScriptResult:
        """Run a script for a specific repo."""
        ...

    async def run_all_setup(self, workspace_id: str) -> list[ScriptResult]:
        """Run setup scripts for all repos."""
        ...

    async def run_all_cleanup(self, workspace_id: str) -> list[ScriptResult]:
        """Run cleanup scripts for all repos."""
        ...

    async def start_dev_servers(self, workspace_id: str) -> list[asyncio.subprocess.Process]:
        """Start dev servers for all repos that have them."""
        ...

    async def stop_dev_servers(self, workspace_id: str) -> None:
        """Stop dev servers for a workspace."""
        ...


class RepoScriptServiceImpl:
    """Implementation of RepoScriptService."""

    def __init__(
        self,
        session_factory: ClosingAwareSessionFactory,
        workspace_service: WorkspaceService,
        event_bus: EventBus,
    ) -> None:
        self._session_factory = session_factory
        self._workspace_service = workspace_service
        self._events = event_bus
        self._dev_server_processes: dict[str, list[asyncio.subprocess.Process]] = {}

    def _get_session(self) -> AsyncSession:
        """Get a new async session."""
        return self._session_factory()

    async def run_script(
        self,
        workspace_id: str,
        repo_id: str,
        script_type: ScriptType,
    ) -> ScriptResult:
        """Run a script for a specific repo."""
        from sqlmodel import select

        from kagan.adapters.db.schema import Repo, WorkspaceRepo
        from kagan.core.events import ScriptCompleted

        async with self._get_session() as session:
            result = await session.execute(
                select(WorkspaceRepo, Repo)
                .join(Repo)
                .where(WorkspaceRepo.workspace_id == workspace_id)
                .where(WorkspaceRepo.repo_id == repo_id)
            )
            row = result.first()

        if not row:
            raise ValueError(f"Repo {repo_id} not found in workspace {workspace_id}")

        workspace_repo, repo = row
        script_content = self._get_script(repo, script_type)
        if not script_content:
            result = ScriptResult(
                repo_id=repo_id,
                repo_name=repo.name,
                script_type=script_type,
                success=True,
                exit_code=0,
                stdout="",
                stderr="No script configured",
            )
            await self._events.publish(
                ScriptCompleted(
                    workspace_id=workspace_id,
                    repo_id=repo_id,
                    script_type=script_type.value,
                    success=True,
                    exit_code=0,
                )
            )
            return result

        result = await self._execute_script(
            repo_id=repo_id,
            repo_name=repo.name,
            script_type=script_type,
            script_content=script_content,
            working_dir=Path(workspace_repo.worktree_path),
        )
        await self._events.publish(
            ScriptCompleted(
                workspace_id=workspace_id,
                repo_id=repo_id,
                script_type=script_type.value,
                success=result.success,
                exit_code=result.exit_code,
            )
        )
        return result

    async def run_all_setup(self, workspace_id: str) -> list[ScriptResult]:
        """Run setup scripts for all repos in parallel."""
        repos = await self._workspace_service.get_workspace_repos(workspace_id)
        tasks = [self.run_script(workspace_id, repo["repo_id"], ScriptType.SETUP) for repo in repos]
        return await asyncio.gather(*tasks)

    async def run_all_cleanup(self, workspace_id: str) -> list[ScriptResult]:
        """Run cleanup scripts for all repos."""
        repos = await self._workspace_service.get_workspace_repos(workspace_id)
        results: list[ScriptResult] = []
        for repo in repos:
            results.append(await self.run_script(workspace_id, repo["repo_id"], ScriptType.CLEANUP))
        return results

    async def start_dev_servers(self, workspace_id: str) -> list[asyncio.subprocess.Process]:
        """Start dev servers for all repos that have them."""
        from sqlmodel import select

        from kagan.adapters.db.schema import Repo, WorkspaceRepo

        async with self._get_session() as session:
            result = await session.execute(
                select(WorkspaceRepo, Repo)
                .join(Repo)
                .where(WorkspaceRepo.workspace_id == workspace_id)
            )
            results = result.all()

        processes: list[asyncio.subprocess.Process] = []
        for workspace_repo, repo in results:
            script = repo.scripts.get("dev_server")
            if not script:
                continue
            proc = await asyncio.create_subprocess_shell(
                script,
                cwd=workspace_repo.worktree_path,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            processes.append(proc)

        self._dev_server_processes[workspace_id] = processes
        return processes

    async def stop_dev_servers(self, workspace_id: str) -> None:
        """Stop all dev servers for a workspace."""
        processes = self._dev_server_processes.get(workspace_id, [])
        for proc in processes:
            if proc.returncode is None:
                proc.terminate()
                try:
                    await asyncio.wait_for(proc.wait(), timeout=5.0)
                except TimeoutError:
                    proc.kill()
        self._dev_server_processes.pop(workspace_id, None)

    def _get_script(self, repo: Repo, script_type: ScriptType) -> str | None:
        """Get script content for a repo."""
        key_map = {
            ScriptType.SETUP: "setup",
            ScriptType.CLEANUP: "cleanup",
            ScriptType.DEV_SERVER: "dev_server",
        }
        return repo.scripts.get(key_map[script_type])

    async def _execute_script(
        self,
        repo_id: str,
        repo_name: str,
        script_type: ScriptType,
        script_content: str,
        working_dir: Path,
    ) -> ScriptResult:
        """Execute a script and capture output."""
        proc = await asyncio.create_subprocess_shell(
            script_content,
            cwd=working_dir,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await proc.communicate()

        return ScriptResult(
            repo_id=repo_id,
            repo_name=repo_name,
            script_type=script_type,
            success=proc.returncode == 0,
            exit_code=proc.returncode or 0,
            stdout=stdout.decode(),
            stderr=stderr.decode(),
        )
