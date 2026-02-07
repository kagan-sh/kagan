"""Smoke tests for multi-repo workflows."""

from __future__ import annotations

import asyncio
from pathlib import Path

import pytest

from kagan.bootstrap import create_app_context
from kagan.config import KaganConfig
from tests.helpers.git import init_git_repo_with_commit


async def _run_git(cwd: Path, *args: str) -> None:
    proc = await asyncio.create_subprocess_exec(
        "git",
        *args,
        cwd=cwd,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    _stdout, stderr = await proc.communicate()
    if proc.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed: {stderr.decode().strip()}")


async def _init_repo_with_origin(tmp_path: Path, name: str) -> Path:
    repo_path = tmp_path / name
    repo_path.mkdir()
    await init_git_repo_with_commit(repo_path)

    remote_path = tmp_path / f"{name}-remote.git"
    await _run_git(tmp_path, "init", "--bare", str(remote_path))
    await _run_git(repo_path, "remote", "add", "origin", str(remote_path))
    await _run_git(repo_path, "push", "-u", "origin", "main")
    return repo_path


@pytest.fixture
async def multi_repo_context(tmp_path: Path):
    repo_a = await _init_repo_with_origin(tmp_path, "repo-a")
    repo_b = await _init_repo_with_origin(tmp_path, "repo-b")
    config_path = tmp_path / "config.toml"
    db_path = tmp_path / "kagan.db"

    ctx = await create_app_context(
        config_path,
        db_path,
        config=KaganConfig(),
        project_root=repo_a,
    )
    try:
        yield ctx, repo_a, repo_b
    finally:
        await ctx.close()


@pytest.mark.asyncio
async def test_project_and_workspace_multi_repo(multi_repo_context):
    ctx, repo_a, repo_b = multi_repo_context
    project_id = await ctx.project_service.create_project(
        name="Multi Repo Project",
        repo_paths=[repo_a, repo_b],
    )

    repos = await ctx.project_service.get_project_repos(project_id)
    assert {repo.path for repo in repos} == {str(repo_a), str(repo_b)}

    task = await ctx.task_service.create_task(
        title="Test Task",
        description="",
        project_id=project_id,
    )
    workspace_id = await ctx.workspace_service.provision_for_project(
        task_id=task.id,
        project_id=project_id,
    )

    workspace_repos = await ctx.workspace_service.get_workspace_repos(workspace_id)
    assert len(workspace_repos) == 2
    for repo in workspace_repos:
        worktree_path = Path(repo["worktree_path"])
        assert worktree_path.exists()
        assert (worktree_path / ".git").exists()


@pytest.mark.asyncio
async def test_diff_and_merge_multi_repo(multi_repo_context):
    ctx, repo_a, repo_b = multi_repo_context
    project_id = await ctx.project_service.create_project(
        name="Diff Merge Project",
        repo_paths=[repo_a, repo_b],
    )
    task = await ctx.task_service.create_task(
        title="Change Something",
        description="",
        project_id=project_id,
    )
    workspace_id = await ctx.workspace_service.provision_for_project(
        task_id=task.id,
        project_id=project_id,
    )
    workspace_repos = await ctx.workspace_service.get_workspace_repos(workspace_id)
    target_repo = workspace_repos[0]
    worktree_path = Path(target_repo["worktree_path"])

    (worktree_path / "change.txt").write_text("hello\n")
    await _run_git(worktree_path, "add", "change.txt")
    await _run_git(worktree_path, "commit", "-m", "Add change")

    diffs = await ctx.diff_service.get_all_diffs(workspace_id)
    assert diffs

    merge_result = await ctx.merge_service.merge_repo(workspace_id, target_repo["repo_id"])
    assert merge_result.success
    assert merge_result.commit_sha is not None


@pytest.mark.asyncio
async def test_release_cleans_worktrees(multi_repo_context):
    ctx, repo_a, repo_b = multi_repo_context
    project_id = await ctx.project_service.create_project(
        name="Release Project",
        repo_paths=[repo_a, repo_b],
    )
    task = await ctx.task_service.create_task(
        title="Cleanup Task",
        description="",
        project_id=project_id,
    )
    workspace_id = await ctx.workspace_service.provision_for_project(
        task_id=task.id,
        project_id=project_id,
    )
    workspace_repos = await ctx.workspace_service.get_workspace_repos(workspace_id)
    worktree_paths = [Path(repo["worktree_path"]) for repo in workspace_repos]
    assert all(path.exists() for path in worktree_paths)

    await ctx.workspace_service.release(workspace_id, cleanup=True)

    assert not any(path.exists() for path in worktree_paths)
