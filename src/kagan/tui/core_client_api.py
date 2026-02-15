"""Core-backed TUI API/context adapters.

TUI uses this module to behave as a thin IPC client over the core host.
"""

from __future__ import annotations

import asyncio
import contextlib
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, NotRequired, TypedDict, cast

from kagan.core.adapters.db.schema import Project, Repo, Task, Workspace
from kagan.core.models.enums import TaskStatus
from kagan.core.services.diffs import FileDiff, RepoDiff
from kagan.core.services.jobs import JobRecord, JobStatus
from kagan.core.services.merges import MergeResult, MergeStrategy
from kagan.core.services.runtime import (
    RuntimeContextState,
    RuntimeSessionEvent,
    StartupSessionDecision,
)
from kagan.core.session_binding import SessionOrigin

if TYPE_CHECKING:
    from kagan.core.config import KaganConfig
    from kagan.core.ipc.client import IPCClient


def _raise_core_error(method: str, message: str | None) -> RuntimeError:
    text = message or f"Core TUI API call failed: {method}"
    return RuntimeError(text)


def _task_id_from_input(value: object) -> str:
    if isinstance(value, Task):
        return str(value.id)
    if isinstance(value, dict):
        raw = value.get("id")
        if isinstance(raw, str) and raw.strip():
            return raw.strip()
    if isinstance(value, str) and value.strip():
        return value.strip()
    raise ValueError("task_id is required")


def _clean_project_repo_args(project_id: str, repo_id: str | None) -> tuple[str, str | None]:
    cleaned_project_id = project_id.strip()
    if not cleaned_project_id:
        raise ValueError("project_id is required")

    cleaned_repo_id: str | None = None
    if repo_id is not None:
        normalized_repo_id = repo_id.strip()
        if not normalized_repo_id:
            raise ValueError("repo_id must be a non-empty string when provided")
        cleaned_repo_id = normalized_repo_id
    return cleaned_project_id, cleaned_repo_id


def _parse_datetime(value: object) -> datetime:
    if isinstance(value, datetime):
        return value
    if isinstance(value, str) and value.strip():
        normalized = value.replace("Z", "+00:00")
        try:
            parsed = datetime.fromisoformat(normalized)
        except ValueError:
            return datetime.now(UTC)
        if parsed.tzinfo is None:
            return parsed.replace(tzinfo=UTC)
        return parsed
    return datetime.now(UTC)


def _task_from_payload(payload: dict[str, Any]) -> Task:
    data = dict(payload)
    data.pop("runtime", None)
    return Task.model_validate(data)


def _project_from_payload(payload: dict[str, Any]) -> Project:
    return Project.model_validate(payload)


def _repo_from_payload(payload: dict[str, Any]) -> Repo:
    return Repo.model_validate(payload)


def _workspace_from_payload(payload: dict[str, Any]) -> Workspace:
    return Workspace.model_validate(payload)


def _job_record_from_payload(payload: dict[str, Any]) -> JobRecord:
    status_value = str(payload.get("status", JobStatus.QUEUED.value))
    return JobRecord(
        job_id=str(payload.get("job_id", "")),
        task_id=str(payload.get("task_id", "")),
        action=str(payload.get("action", "")),
        status=JobStatus(status_value),
        created_at=_parse_datetime(payload.get("created_at")),
        updated_at=_parse_datetime(payload.get("updated_at")),
        params=dict(payload.get("params", {}) or {}),
        result=dict(payload["result"]) if isinstance(payload.get("result"), dict) else None,
        message=str(payload["message"]) if payload.get("message") is not None else None,
        code=str(payload["code"]) if payload.get("code") is not None else None,
    )


def _merge_result_from_payload(payload: dict[str, Any]) -> MergeResult:
    strategy_raw = payload.get("strategy", MergeStrategy.DIRECT.value)
    strategy = (
        strategy_raw
        if isinstance(strategy_raw, MergeStrategy)
        else MergeStrategy(str(strategy_raw))
    )
    conflict_files_raw = payload.get("conflict_files")
    conflict_files = (
        [str(item) for item in conflict_files_raw] if isinstance(conflict_files_raw, list) else None
    )
    return MergeResult(
        repo_id=str(payload.get("repo_id", "")),
        repo_name=str(payload.get("repo_name", "")),
        strategy=strategy,
        success=bool(payload.get("success", False)),
        message=str(payload.get("message", "")),
        pr_url=str(payload["pr_url"]) if payload.get("pr_url") is not None else None,
        commit_sha=str(payload["commit_sha"]) if payload.get("commit_sha") is not None else None,
        conflict_op=(
            str(payload["conflict_op"]) if payload.get("conflict_op") is not None else None
        ),
        conflict_files=conflict_files,
    )


def _repo_diff_from_payload(payload: dict[str, Any]) -> RepoDiff:
    files_payload = payload.get("files")
    files = []
    if isinstance(files_payload, list):
        for item in files_payload:
            if not isinstance(item, dict):
                continue
            files.append(
                FileDiff(
                    path=str(item.get("path", "")),
                    additions=int(item.get("additions", 0) or 0),
                    deletions=int(item.get("deletions", 0) or 0),
                    status=str(item.get("status", "")),
                    diff_content=str(item.get("diff_content", "")),
                )
            )
    return RepoDiff(
        repo_id=str(payload.get("repo_id", "")),
        repo_name=str(payload.get("repo_name", "")),
        target_branch=str(payload.get("target_branch", "")),
        files=files,
        total_additions=int(payload.get("total_additions", 0) or 0),
        total_deletions=int(payload.get("total_deletions", 0) or 0),
    )


@dataclass(frozen=True, slots=True)
class JanitorResultView:
    worktrees_pruned: int
    branches_deleted: list[str]
    repos_processed: list[str]
    total_cleaned: int


class PluginUiRefresh(TypedDict, total=False):
    repo: bool
    tasks: bool
    sessions: bool


class PluginUiInvokeResult(TypedDict):
    ok: bool
    code: str
    message: str
    data: dict[str, Any] | None
    refresh: PluginUiRefresh


class PluginUiCatalog(TypedDict):
    schema_version: str
    actions: list[dict[str, Any]]
    forms: list[dict[str, Any]]
    badges: list[dict[str, Any]]
    diagnostics: NotRequired[list[str]]


class CoreBackedApi:
    """API adapter that forwards calls to core over IPC."""

    def __init__(
        self,
        client: IPCClient,
        *,
        session_id: str,
        session_profile: str = "maintainer",
        session_origin: str = SessionOrigin.TUI.value,
        config_path: Path | None = None,
        db_path: Path | None = None,
    ) -> None:
        self._client = client
        self._session_id = session_id
        self._session_profile = session_profile
        self._session_origin = session_origin
        self._config_path = config_path
        self._db_path = db_path
        self._reconnect_lock = asyncio.Lock()
        self._runtime_by_task: dict[str, dict[str, Any]] = {}
        self._runtime_state = RuntimeContextState()

    async def _request_core(
        self,
        *,
        capability: str,
        method: str,
        params: dict[str, Any],
        request_timeout_seconds: float | None = None,
    ) -> Any:
        await self._ensure_connected()
        try:
            response = await self._client.request(
                session_id=self._session_id,
                session_profile=self._session_profile,
                session_origin=self._session_origin,
                capability=capability,
                method=method,
                params=params,
                request_timeout_seconds=request_timeout_seconds,
            )
        except ConnectionError:
            await self._reconnect_client()
            response = await self._client.request(
                session_id=self._session_id,
                session_profile=self._session_profile,
                session_origin=self._session_origin,
                capability=capability,
                method=method,
                params=params,
                request_timeout_seconds=request_timeout_seconds,
            )
        if not response.ok:
            raise _raise_core_error(method, response.error.message if response.error else None)
        return response.result or {}

    async def _ensure_connected(self) -> None:
        if self._client.is_connected:
            return
        await self._reconnect_client()

    async def _reconnect_client(self) -> None:
        async with self._reconnect_lock:
            if self._client.is_connected:
                return

            with contextlib.suppress(ConnectionError, OSError):
                await self._client.connect()
            if self._client.is_connected:
                return

            if self._config_path is None or self._db_path is None:
                msg = "Client is not connected; call connect() first"
                raise ConnectionError(msg)

            from kagan.core.launcher import ensure_core_running

            endpoint = await ensure_core_running(
                config_path=self._config_path,
                db_path=self._db_path,
            )
            with contextlib.suppress(ConnectionError, OSError):
                await self._client.close()
            self._client._endpoint = endpoint  # type: ignore[attr-defined]  # quality-allow-private
            self._client._transport = self._client._transport_for_endpoint(  # type: ignore[attr-defined]  # quality-allow-private
                endpoint
            )
            await self._client.connect()

    async def _call_core(
        self,
        method: str,
        *,
        kwargs: dict[str, Any] | None = None,
        request_timeout_seconds: float | None = None,
    ) -> Any:
        payload = await self._request_core(
            capability="tui",
            method="api_call",
            params={"method": method, "kwargs": kwargs or {}},
            request_timeout_seconds=request_timeout_seconds,
        )
        if not bool(payload.get("success", False)):
            raise _raise_core_error(method, str(payload.get("message", "")))
        return payload.get("value")

    @staticmethod
    def _as_namespace(value: dict[str, Any]) -> SimpleNamespace:
        return SimpleNamespace(**value)

    def _cache_task_runtime(self, task_payload: dict[str, Any]) -> None:
        task_id = task_payload.get("id")
        if not isinstance(task_id, str):
            task_id = task_payload.get("task_id")
        runtime = task_payload.get("runtime")
        if isinstance(task_id, str) and isinstance(runtime, dict):
            self._runtime_by_task[task_id] = dict(runtime)

    def _cache_task_runtimes(self, payloads: list[dict[str, Any]]) -> None:
        for payload in payloads:
            if isinstance(payload, dict):
                self._cache_task_runtime(payload)

    def _runtime_view(self, task_id: str) -> dict[str, Any] | None:
        value = self._runtime_by_task.get(task_id)
        return dict(value) if value is not None else None

    async def create_task(
        self,
        title: str,
        description: str = "",
        *,
        project_id: str | None = None,
        created_by: str | None = None,
        **fields: Any,
    ) -> Task:
        kwargs: dict[str, Any] = {
            "title": title,
            "description": description,
            **fields,
        }
        if project_id is not None:
            kwargs["project_id"] = project_id
        if created_by is not None:
            kwargs["created_by"] = created_by
        raw = await self._call_core("create_task", kwargs=kwargs)
        assert isinstance(raw, dict)
        self._cache_task_runtime(raw)
        return _task_from_payload(raw)

    async def get_task(self, task_id: str) -> Task | None:
        raw = await self._call_core("get_task", kwargs={"task_id": task_id})
        if not isinstance(raw, dict):
            return None
        self._cache_task_runtime(raw)
        return _task_from_payload(raw)

    async def list_tasks(
        self,
        *,
        project_id: str | None = None,
        status: TaskStatus | str | None = None,
        filter: str | None = None,
    ) -> list[Task]:
        kwargs: dict[str, Any] = {}
        if project_id is not None:
            kwargs["project_id"] = project_id
        if status is not None:
            kwargs["status"] = status.value if isinstance(status, TaskStatus) else str(status)
        if filter is not None:
            kwargs["filter"] = filter
        raw = await self._call_core("list_tasks", kwargs=kwargs)
        payloads = [item for item in raw if isinstance(item, dict)] if isinstance(raw, list) else []
        self._cache_task_runtimes(payloads)
        return [_task_from_payload(item) for item in payloads]

    async def search_tasks(self, query: str) -> list[Task]:
        raw = await self._call_core("search_tasks", kwargs={"query": query})
        payloads = [item for item in raw if isinstance(item, dict)] if isinstance(raw, list) else []
        self._cache_task_runtimes(payloads)
        return [_task_from_payload(item) for item in payloads]

    async def update_task(self, task_id: str, **fields: Any) -> Task | None:
        kwargs: dict[str, Any] = {"task_id": task_id, **fields}
        raw = await self._call_core("update_task", kwargs=kwargs)
        if not isinstance(raw, dict):
            return None
        self._cache_task_runtime(raw)
        return _task_from_payload(raw)

    async def move_task(self, task_id: str, status: TaskStatus | str) -> Task | None:
        status_value = status.value if isinstance(status, TaskStatus) else str(status)
        raw = await self._call_core(
            "move_task",
            kwargs={"task_id": task_id, "status": status_value},
        )
        if not isinstance(raw, dict):
            return None
        self._cache_task_runtime(raw)
        return _task_from_payload(raw)

    async def delete_task(self, task_id: str) -> tuple[bool, str]:
        raw = await self._call_core("delete_task", kwargs={"task_id": task_id})
        if isinstance(raw, list) and len(raw) == 2:
            return bool(raw[0]), str(raw[1])
        return False, f"Task {task_id} delete failed"

    async def get_scratchpad(self, task_id: str) -> str:
        raw = await self._call_core("get_scratchpad", kwargs={"task_id": task_id})
        return str(raw) if raw is not None else ""

    async def open_project(self, project_id: str) -> Project:
        raw = await self._call_core("open_project", kwargs={"project_id": project_id})
        assert isinstance(raw, dict)
        return _project_from_payload(raw)

    async def create_project(
        self,
        name: str,
        *,
        description: str = "",
        repo_paths: list[str] | None = None,
    ) -> str:
        kwargs: dict[str, Any] = {"name": name, "description": description}
        if repo_paths is not None:
            kwargs["repo_paths"] = repo_paths
        raw = await self._call_core("create_project", kwargs=kwargs)
        return str(raw)

    async def add_repo(
        self,
        project_id: str,
        repo_path: str,
        *,
        is_primary: bool = False,
    ) -> str:
        raw = await self._call_core(
            "add_repo",
            kwargs={"project_id": project_id, "repo_path": repo_path, "is_primary": is_primary},
        )
        return str(raw)

    async def get_project(self, project_id: str) -> Project | None:
        raw = await self._call_core("get_project", kwargs={"project_id": project_id})
        if not isinstance(raw, dict):
            return None
        return _project_from_payload(raw)

    async def list_projects(self, *, limit: int = 10) -> list[Project]:
        raw = await self._call_core("list_projects", kwargs={"limit": limit})
        payloads = [item for item in raw if isinstance(item, dict)] if isinstance(raw, list) else []
        return [_project_from_payload(item) for item in payloads]

    async def get_project_repos(self, project_id: str) -> list[Repo]:
        raw = await self._call_core("get_project_repos", kwargs={"project_id": project_id})
        payloads = [item for item in raw if isinstance(item, dict)] if isinstance(raw, list) else []
        return [_repo_from_payload(item) for item in payloads]

    async def get_project_repo_details(self, project_id: str) -> list[dict[str, Any]]:
        raw = await self._call_core("get_project_repo_details", kwargs={"project_id": project_id})
        if not isinstance(raw, list):
            return []
        return [dict(item) for item in raw if isinstance(item, dict)]

    async def find_project_by_repo_path(self, repo_path: str | Path) -> Project | None:
        raw = await self._call_core(
            "find_project_by_repo_path",
            kwargs={"repo_path": str(repo_path)},
        )
        if not isinstance(raw, dict):
            return None
        return _project_from_payload(raw)

    async def update_repo_default_branch(
        self,
        repo_id: str,
        branch: str,
        *,
        mark_configured: bool = False,
    ) -> Repo | None:
        raw = await self._call_core(
            "update_repo_default_branch",
            kwargs={
                "repo_id": repo_id,
                "branch": branch,
                "mark_configured": mark_configured,
            },
        )
        if not isinstance(raw, dict):
            return None
        return _repo_from_payload(raw)

    async def get_settings(self) -> dict[str, object]:
        payload = await self._request_core(
            capability="settings",
            method="get",
            params={},
        )
        raw = payload.get("settings")
        if not isinstance(raw, dict):
            return {}
        return {str(key): value for key, value in raw.items()}

    async def update_settings(
        self,
        fields: dict[str, object],
    ) -> tuple[bool, str, dict[str, object], dict[str, object]]:
        payload = await self._request_core(
            capability="settings",
            method="update",
            params={"fields": dict(fields)},
        )
        success = bool(payload.get("success", False))
        message = str(payload.get("message", ""))
        updated_raw = payload.get("updated")
        settings_raw = payload.get("settings")
        updated = (
            {str(key): value for key, value in updated_raw.items()}
            if isinstance(updated_raw, dict)
            else {}
        )
        settings = (
            {str(key): value for key, value in settings_raw.items()}
            if isinstance(settings_raw, dict)
            else {}
        )
        return success, message, updated, settings

    async def invoke_plugin(
        self,
        capability: str,
        method: str,
        params: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Invoke a plugin operation via core IPC.

        Args:
            capability: Plugin capability namespace.
            method: Operation method name.
            params: Optional parameters dict.

        Returns:
            Plugin operation result dict.
        """
        raw = await self._call_core(
            "invoke_plugin",
            kwargs={"capability": capability, "method": method, "params": params or {}},
        )
        if not isinstance(raw, dict):
            msg = f"Core returned invalid plugin payload: {capability}.{method}"
            raise RuntimeError(msg)
        return dict(raw)

    async def plugin_ui_catalog(
        self,
        *,
        project_id: str,
        repo_id: str | None = None,
    ) -> PluginUiCatalog:
        cleaned_project_id, cleaned_repo_id = _clean_project_repo_args(project_id, repo_id)
        kwargs: dict[str, Any] = {"project_id": cleaned_project_id}
        if cleaned_repo_id is not None:
            kwargs["repo_id"] = cleaned_repo_id

        raw = await self._call_core("plugin_ui_catalog", kwargs=kwargs)
        if not isinstance(raw, dict):
            raise RuntimeError("Core returned invalid plugin UI catalog payload")
        return cast("PluginUiCatalog", dict(raw))

    async def plugin_ui_invoke(
        self,
        *,
        project_id: str,
        plugin_id: str,
        action_id: str,
        repo_id: str | None = None,
        inputs: dict[str, Any] | None = None,
    ) -> PluginUiInvokeResult:
        cleaned_project_id, cleaned_repo_id = _clean_project_repo_args(project_id, repo_id)
        cleaned_plugin_id = plugin_id.strip()
        if not cleaned_plugin_id:
            raise ValueError("plugin_id is required")
        cleaned_action_id = action_id.strip()
        if not cleaned_action_id:
            raise ValueError("action_id is required")

        kwargs: dict[str, Any] = {
            "project_id": cleaned_project_id,
            "plugin_id": cleaned_plugin_id,
            "action_id": cleaned_action_id,
        }
        if cleaned_repo_id is not None:
            kwargs["repo_id"] = cleaned_repo_id
        if inputs is not None:
            kwargs["inputs"] = dict(inputs)

        raw = await self._call_core("plugin_ui_invoke", kwargs=kwargs)
        if not isinstance(raw, dict):
            raise RuntimeError("Core returned invalid plugin UI invoke payload")
        return cast("PluginUiInvokeResult", dict(raw))

    async def submit_job(
        self,
        task_id: str,
        action: str,
        *,
        arguments: dict[str, Any] | None = None,
    ) -> JobRecord:
        kwargs: dict[str, Any] = {"task_id": task_id, "action": action}
        if arguments is not None:
            kwargs["arguments"] = arguments
        raw = await self._call_core("submit_job", kwargs=kwargs)
        assert isinstance(raw, dict)
        return _job_record_from_payload(raw)

    async def wait_job(
        self,
        job_id: str,
        *,
        task_id: str,
        timeout_seconds: float | None = None,
    ) -> JobRecord | None:
        kwargs: dict[str, Any] = {"job_id": job_id, "task_id": task_id}
        if timeout_seconds is not None:
            kwargs["timeout_seconds"] = timeout_seconds
        timeout = timeout_seconds + 5.0 if timeout_seconds is not None else None
        raw = await self._call_core("wait_job", kwargs=kwargs, request_timeout_seconds=timeout)
        if not isinstance(raw, dict):
            return None
        return _job_record_from_payload(raw)

    async def cancel_job(self, job_id: str, *, task_id: str) -> JobRecord | None:
        raw = await self._call_core("cancel_job", kwargs={"job_id": job_id, "task_id": task_id})
        if not isinstance(raw, dict):
            return None
        return _job_record_from_payload(raw)

    async def create_session(
        self,
        task_id: str,
        *,
        worktree_path: Path | None = None,
        reuse_if_exists: bool = True,
    ) -> Any:
        kwargs: dict[str, Any] = {"task_id": task_id, "reuse_if_exists": reuse_if_exists}
        if worktree_path is not None:
            kwargs["worktree_path"] = str(worktree_path)
        return await self._call_core("create_session", kwargs=kwargs)

    async def attach_session(self, task_id: str) -> bool:
        raw = await self._call_core("attach_session", kwargs={"task_id": task_id})
        return bool(raw)

    async def session_exists(self, task_id: str) -> bool:
        raw = await self._call_core("session_exists", kwargs={"task_id": task_id})
        return bool(raw)

    async def kill_session(self, task_id: str) -> None:
        await self._call_core("kill_session", kwargs={"task_id": task_id})

    async def get_workspace_path(self, task_id: str) -> Path | None:
        raw = await self._call_core("get_workspace_path", kwargs={"task_id": task_id})
        if not isinstance(raw, str) or not raw.strip():
            return None
        return Path(raw)

    async def provision_workspace(self, *, task_id: str, repos: list[Any]) -> str:
        repo_payload: list[dict[str, Any]] = []
        for item in repos:
            if (
                hasattr(item, "repo_id")
                and hasattr(item, "repo_path")
                and hasattr(item, "target_branch")
            ):
                repo_payload.append(
                    {
                        "repo_id": str(item.repo_id),
                        "repo_path": str(item.repo_path),
                        "target_branch": str(item.target_branch),
                    }
                )
            elif isinstance(item, dict):
                repo_payload.append(
                    {
                        "repo_id": str(item.get("repo_id", "")),
                        "repo_path": str(item.get("repo_path", "")),
                        "target_branch": str(item.get("target_branch", "")),
                    }
                )
        raw = await self._call_core(
            "provision_workspace",
            kwargs={"task_id": task_id, "repos": repo_payload},
        )
        return str(raw)

    async def list_workspaces(self, *, task_id: str | None = None) -> list[Workspace]:
        kwargs: dict[str, Any] = {}
        if task_id is not None:
            kwargs["task_id"] = task_id
        raw = await self._call_core("list_workspaces", kwargs=kwargs)
        payloads = [item for item in raw if isinstance(item, dict)] if isinstance(raw, list) else []
        return [_workspace_from_payload(item) for item in payloads]

    async def get_workspace_repos(self, workspace_id: str) -> list[dict[str, Any]]:
        raw = await self._call_core("get_workspace_repos", kwargs={"workspace_id": workspace_id})
        if not isinstance(raw, list):
            return []
        return [dict(item) for item in raw if isinstance(item, dict)]

    async def get_repo_diff(self, workspace_id: str, repo_id: str) -> RepoDiff:
        raw = await self._call_core(
            "get_repo_diff",
            kwargs={"workspace_id": workspace_id, "repo_id": repo_id},
        )
        if not isinstance(raw, dict):
            raise RuntimeError("Core returned invalid repo diff payload")
        return _repo_diff_from_payload(raw)

    async def cleanup_orphan_workspaces(self, valid_task_ids: set[str]) -> list[str]:
        raw = await self._call_core(
            "cleanup_orphan_workspaces",
            kwargs={"valid_task_ids": sorted(valid_task_ids)},
        )
        if not isinstance(raw, list):
            return []
        return [str(item) for item in raw]

    async def run_workspace_janitor(
        self,
        valid_workspace_ids: set[str],
        *,
        prune_worktrees: bool = True,
        gc_branches: bool = True,
    ) -> JanitorResultView:
        raw = await self._call_core(
            "run_workspace_janitor",
            kwargs={
                "valid_workspace_ids": sorted(valid_workspace_ids),
                "prune_worktrees": prune_worktrees,
                "gc_branches": gc_branches,
            },
        )
        payload = raw if isinstance(raw, dict) else {}
        return JanitorResultView(
            worktrees_pruned=int(payload.get("worktrees_pruned", 0) or 0),
            branches_deleted=[
                str(item) for item in payload.get("branches_deleted", []) if isinstance(item, str)
            ],
            repos_processed=[
                str(item) for item in payload.get("repos_processed", []) if isinstance(item, str)
            ],
            total_cleaned=int(payload.get("total_cleaned", 0) or 0),
        )

    async def get_workspace_diff(self, task_id: str, *, base_branch: str) -> str:
        raw = await self._call_core(
            "get_workspace_diff",
            kwargs={"task_id": task_id, "base_branch": base_branch},
        )
        return str(raw) if raw is not None else ""

    async def get_workspace_commit_log(self, task_id: str, *, base_branch: str) -> list[str]:
        raw = await self._call_core(
            "get_workspace_commit_log",
            kwargs={"task_id": task_id, "base_branch": base_branch},
        )
        if not isinstance(raw, list):
            return []
        return [str(item) for item in raw]

    async def get_workspace_diff_stats(self, task_id: str, *, base_branch: str) -> str:
        raw = await self._call_core(
            "get_workspace_diff_stats",
            kwargs={"task_id": task_id, "base_branch": base_branch},
        )
        return str(raw) if raw is not None else ""

    async def rebase_workspace(self, task_id: str, base_branch: str) -> tuple[bool, str, list[str]]:
        raw = await self._call_core(
            "rebase_workspace",
            kwargs={"task_id": task_id, "base_branch": base_branch},
        )
        if isinstance(raw, list) and len(raw) == 3:
            return bool(raw[0]), str(raw[1]), [str(item) for item in raw[2]]
        return False, "Rebase failed", []

    async def abort_workspace_rebase(self, task_id: str) -> None:
        await self._call_core("abort_workspace_rebase", kwargs={"task_id": task_id})

    async def merge_repo(
        self,
        workspace_id: str,
        repo_id: str,
        *,
        strategy: MergeStrategy,
        pr_title: str | None = None,
        pr_body: str | None = None,
        commit_message: str | None = None,
    ) -> MergeResult:
        kwargs: dict[str, Any] = {
            "workspace_id": workspace_id,
            "repo_id": repo_id,
            "strategy": strategy.value,
            "pr_title": pr_title,
            "pr_body": pr_body,
            "commit_message": commit_message,
        }
        raw = await self._call_core("merge_repo", kwargs=kwargs)
        assert isinstance(raw, dict)
        return _merge_result_from_payload(raw)

    async def has_no_changes(self, task: Task | str | dict[str, Any]) -> bool:
        task_id = _task_id_from_input(task)
        raw = await self._call_core("has_no_changes", kwargs={"task_id": task_id})
        return bool(raw)

    async def close_exploratory(self, task: Task | str | dict[str, Any]) -> tuple[bool, str]:
        task_id = _task_id_from_input(task)
        raw = await self._call_core("close_exploratory", kwargs={"task_id": task_id})
        if isinstance(raw, list) and len(raw) == 2:
            return bool(raw[0]), str(raw[1])
        return False, "Close exploratory failed"

    async def merge_task_direct(self, task: Task | str | dict[str, Any]) -> tuple[bool, str]:
        task_id = _task_id_from_input(task)
        raw = await self._call_core("merge_task_direct", kwargs={"task_id": task_id})
        if isinstance(raw, list) and len(raw) == 2:
            return bool(raw[0]), str(raw[1])
        return False, "Merge failed"

    async def apply_rejection_feedback(
        self,
        task: Task | str | dict[str, Any],
        feedback: str | None,
        action: str,
    ) -> Task | None:
        task_id = _task_id_from_input(task)
        raw = await self._call_core(
            "apply_rejection_feedback",
            kwargs={"task_id": task_id, "feedback": feedback, "action": action},
        )
        if not isinstance(raw, dict):
            return None
        self._cache_task_runtime(raw)
        return _task_from_payload(raw)

    async def get_all_diffs(self, workspace_id: str) -> list[RepoDiff]:
        raw = await self._call_core("get_all_diffs", kwargs={"workspace_id": workspace_id})
        payloads = [item for item in raw if isinstance(item, dict)] if isinstance(raw, list) else []
        return [_repo_diff_from_payload(item) for item in payloads]

    async def queue_message(
        self,
        session_id: str,
        content: str,
        *,
        lane: str = "implementation",
        author: str | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> Any:
        raw = await self._call_core(
            "queue_message",
            kwargs={
                "session_id": session_id,
                "content": content,
                "lane": lane,
                "author": author,
                "metadata": metadata,
            },
        )
        return self._as_namespace(raw) if isinstance(raw, dict) else None

    async def get_queue_status(self, session_id: str, *, lane: str = "implementation") -> Any:
        raw = await self._call_core(
            "get_queue_status",
            kwargs={"session_id": session_id, "lane": lane},
        )
        if isinstance(raw, dict):
            return self._as_namespace(raw)
        return SimpleNamespace(has_queued=False)

    async def get_queued_messages(
        self,
        session_id: str,
        *,
        lane: str = "implementation",
    ) -> list[Any]:
        raw = await self._call_core(
            "get_queued_messages",
            kwargs={"session_id": session_id, "lane": lane},
        )
        payloads = [item for item in raw if isinstance(item, dict)] if isinstance(raw, list) else []
        return [self._as_namespace(item) for item in payloads]

    async def take_queued_message(self, session_id: str, *, lane: str = "implementation") -> Any:
        raw = await self._call_core(
            "take_queued_message",
            kwargs={"session_id": session_id, "lane": lane},
        )
        return self._as_namespace(raw) if isinstance(raw, dict) else None

    async def remove_queued_message(
        self,
        session_id: str,
        index: int,
        *,
        lane: str = "implementation",
    ) -> bool:
        raw = await self._call_core(
            "remove_queued_message",
            kwargs={"session_id": session_id, "index": index, "lane": lane},
        )
        return bool(raw)

    async def save_planner_draft(
        self,
        *,
        project_id: str,
        repo_id: str | None = None,
        tasks_json: list[dict[str, Any]],
        todos_json: list[dict[str, Any]] | None = None,
    ) -> Any:
        raw = await self._call_core(
            "save_planner_draft",
            kwargs={
                "project_id": project_id,
                "repo_id": repo_id,
                "tasks_json": tasks_json,
                "todos_json": todos_json,
            },
        )
        return self._as_namespace(raw) if isinstance(raw, dict) else None

    async def list_pending_planner_drafts(
        self,
        project_id: str,
        *,
        repo_id: str | None = None,
    ) -> list[Any]:
        raw = await self._call_core(
            "list_pending_planner_drafts",
            kwargs={"project_id": project_id, "repo_id": repo_id},
        )
        payloads = [item for item in raw if isinstance(item, dict)] if isinstance(raw, list) else []
        return [self._as_namespace(item) for item in payloads]

    async def update_planner_draft_status(self, proposal_id: str, status: Any) -> Any:
        raw_status = getattr(status, "value", status)
        raw = await self._call_core(
            "update_planner_draft_status",
            kwargs={"proposal_id": proposal_id, "status": raw_status},
        )
        return self._as_namespace(raw) if isinstance(raw, dict) else None

    async def get_execution(self, execution_id: str) -> Any:
        raw = await self._call_core("get_execution", kwargs={"execution_id": execution_id})
        return self._as_namespace(raw) if isinstance(raw, dict) else None

    async def get_execution_log_entries(self, execution_id: str) -> list[Any]:
        raw = await self._call_core(
            "get_execution_log_entries",
            kwargs={"execution_id": execution_id},
        )
        payloads = [item for item in raw if isinstance(item, dict)] if isinstance(raw, list) else []
        return [self._as_namespace(item) for item in payloads]

    async def get_latest_execution_for_task(self, task_id: str) -> Any:
        raw = await self._call_core(
            "get_latest_execution_for_task",
            kwargs={"task_id": task_id},
        )
        return self._as_namespace(raw) if isinstance(raw, dict) else None

    async def count_executions_for_task(self, task_id: str) -> int:
        raw = await self._call_core("count_executions_for_task", kwargs={"task_id": task_id})
        return int(raw or 0)

    async def decide_startup(self, cwd: Path) -> StartupSessionDecision:
        raw = await self._call_core("decide_startup", kwargs={"cwd": str(cwd)})
        payload = raw if isinstance(raw, dict) else {}
        preferred_path = payload.get("preferred_path")
        return StartupSessionDecision(
            project_id=payload.get("project_id"),
            preferred_repo_id=payload.get("preferred_repo_id"),
            preferred_path=Path(preferred_path) if isinstance(preferred_path, str) else None,
            suggest_cwd=bool(payload.get("suggest_cwd", False)),
            cwd_path=str(payload["cwd_path"]) if payload.get("cwd_path") is not None else None,
            cwd_is_git_repo=bool(payload.get("cwd_is_git_repo", False)),
        )

    async def dispatch_runtime_session(
        self,
        event: RuntimeSessionEvent,
        *,
        project_id: str | None = None,
        repo_id: str | None = None,
    ) -> RuntimeContextState:
        raw = await self._call_core(
            "dispatch_runtime_session",
            kwargs={
                "event": event.value,
                "project_id": project_id,
                "repo_id": repo_id,
            },
        )
        payload = raw if isinstance(raw, dict) else {}
        self._runtime_state = RuntimeContextState(
            project_id=(
                str(payload["project_id"]) if payload.get("project_id") is not None else None
            ),
            repo_id=str(payload["repo_id"]) if payload.get("repo_id") is not None else None,
        )
        return self._runtime_state

    @property
    def runtime_state(self) -> RuntimeContextState:
        return self._runtime_state

    def get_runtime_view(self, task_id: str) -> dict[str, Any] | None:
        return self._runtime_view(task_id)

    def get_running_task_ids(self) -> set[str]:
        return {
            task_id
            for task_id, view in self._runtime_by_task.items()
            if bool(view.get("is_running"))
        }

    def is_automation_running(self, task_id: str) -> bool:
        view = self._runtime_by_task.get(task_id)
        return bool(view and view.get("is_running"))

    async def reconcile_running_tasks(self, task_ids: list[str]) -> None:
        raw = await self._call_core("reconcile_running_tasks", kwargs={"task_ids": task_ids})
        payloads = [item for item in raw if isinstance(item, dict)] if isinstance(raw, list) else []
        self._cache_task_runtimes(payloads)

    async def resolve_task_base_branch(self, task: Task | str | dict[str, Any]) -> str:
        raw = await self._call_core(
            "resolve_task_base_branch",
            kwargs={"task_id": _task_id_from_input(task)},
        )
        return str(raw) if raw is not None else "main"

    async def prepare_auto_output(self, task: Task | str | dict[str, Any]) -> Any:
        raw = await self._call_core(
            "prepare_auto_output",
            kwargs={"task_id": _task_id_from_input(task)},
        )
        return self._as_namespace(raw) if isinstance(raw, dict) else raw

    async def recover_stale_auto_output(self, task: Task | str | dict[str, Any]) -> Any:
        raw = await self._call_core(
            "recover_stale_auto_output",
            kwargs={"task_id": _task_id_from_input(task)},
        )
        return self._as_namespace(raw) if isinstance(raw, dict) else raw

    def refresh_agent_health(self) -> None:
        return None

    def is_agent_available(self) -> bool:
        return True

    def get_agent_status_message(self) -> str | None:
        return None


@dataclass
class CoreBackedContext:
    """Minimal app context used by TUI screens when attached to core."""

    config: KaganConfig
    config_path: Path
    db_path: Path
    api: CoreBackedApi
    active_project_id: str | None = None
    active_repo_id: str | None = None

    async def close(self) -> None:
        return None
