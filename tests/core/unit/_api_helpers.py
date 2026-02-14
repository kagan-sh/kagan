"""Shared helpers for KaganAPI test suite."""

from __future__ import annotations

from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock

from kagan.core.adapters.db.repositories import (
    AuditRepository,
    ExecutionRepository,
    RepoRepository,
    TaskRepository,
)
from kagan.core.adapters.db.repositories.auxiliary import ScratchRepository
from kagan.core.api import KaganAPI
from kagan.core.bootstrap import AppContext, InMemoryEventBus
from kagan.core.config import KaganConfig
from kagan.core.plugins.github import register_github_plugin
from kagan.core.plugins.sdk import PluginRegistry
from kagan.core.services.projects import ProjectServiceImpl
from kagan.core.services.tasks import TaskServiceImpl

if TYPE_CHECKING:
    from pathlib import Path


def mock_automation_service() -> MagicMock:
    """Build a mock AutomationService with essential methods."""
    auto = MagicMock()
    auto.is_running = MagicMock(return_value=False)
    auto.is_reviewing = MagicMock(return_value=False)
    auto.stop_task = AsyncMock(return_value=True)
    auto.spawn_for_task = AsyncMock(return_value=True)
    auto.merge_lock = MagicMock()
    auto.stop = AsyncMock()
    return auto


def mock_workspace_service() -> MagicMock:
    """Build a mock WorkspaceService with essential methods."""
    ws = MagicMock()
    ws.get_path = AsyncMock(return_value=None)
    ws.delete = AsyncMock()
    ws.list_workspaces = AsyncMock(return_value=[])
    ws.get_workspace_repos = AsyncMock(return_value=[])
    ws.rebase_onto_base = AsyncMock(return_value=(True, "OK", []))
    ws.abort_rebase = AsyncMock(return_value=(True, "OK"))
    return ws


def mock_session_service() -> MagicMock:
    """Build a mock SessionService with essential methods."""
    ss = MagicMock()
    ss.session_exists = AsyncMock(return_value=False)
    ss.kill_session = AsyncMock()
    ss.create_session = AsyncMock(return_value="kagan-test-session")
    ss.attach_session = AsyncMock(return_value=True)
    return ss


def mock_merge_service() -> MagicMock:
    """Build a mock MergeService with essential methods."""
    ms = MagicMock()
    ms.delete_task = AsyncMock(return_value=(True, "Deleted successfully"))
    ms.merge_task = AsyncMock(return_value=(True, "Merged all repos"))
    ms.apply_rejection_feedback = AsyncMock()
    return ms


def mock_job_service() -> MagicMock:
    """Build a mock JobService with essential methods."""
    from datetime import UTC, datetime

    from kagan.core.services.jobs import JobRecord, JobStatus

    now = datetime.now(UTC)
    record = JobRecord(
        job_id="job-1",
        task_id="task-1",
        action="start_agent",
        status=JobStatus.QUEUED,
        created_at=now,
        updated_at=now,
        params={},
    )
    js = MagicMock()
    js.submit = AsyncMock(return_value=record)
    js.get = AsyncMock(return_value=record)
    js.cancel = AsyncMock(return_value=record)
    js.wait = AsyncMock(return_value=record)
    js.events = AsyncMock(return_value=[])
    js.shutdown = AsyncMock()
    return js


async def build_api(
    tmp_path: Path,
) -> tuple[TaskRepository, KaganAPI, AppContext]:
    """Build a KaganAPI with real task/project services and mocked externals."""
    db_path = tmp_path / "test.db"
    config_path = tmp_path / "config.toml"
    config_path.write_text(
        "[general]\nauto_review = false\n"
        'default_worker_agent = "claude"\n\n'
        "[agents.claude]\n"
        'identity = "claude.ai"\nname = "Claude"\nshort_name = "claude"\n'
        'run_command."*" = "echo"\ninteractive_command."*" = "echo"\nactive = true\n'
    )
    config = KaganConfig.load(config_path)

    event_bus = InMemoryEventBus()
    task_repo = TaskRepository(db_path)
    await task_repo.initialize()
    project_id = await task_repo.ensure_test_project("API Test Project")

    session_factory = task_repo.session_factory
    scratch_repo = ScratchRepository(session_factory)
    repo_repository = RepoRepository(session_factory)
    execution_repo = ExecutionRepository(session_factory)
    audit_repo = AuditRepository(session_factory)

    task_service = TaskServiceImpl(task_repo, event_bus, scratch_repo=scratch_repo)
    project_service = ProjectServiceImpl(session_factory, event_bus, repo_repository)

    ctx = AppContext(
        config=config,
        config_path=config_path,
        db_path=db_path,
        event_bus=event_bus,
    )
    ctx._task_repo = task_repo
    ctx.task_service = task_service
    ctx.project_service = project_service
    ctx.execution_service = execution_repo
    ctx.audit_repository = audit_repo
    ctx.workspace_service = mock_workspace_service()
    ctx.session_service = mock_session_service()
    ctx.automation_service = mock_automation_service()
    ctx.merge_service = mock_merge_service()
    ctx.job_service = mock_job_service()
    ctx.active_project_id = project_id
    ctx.plugin_registry = PluginRegistry()
    register_github_plugin(ctx.plugin_registry)

    api = KaganAPI(ctx)
    return task_repo, api, ctx
