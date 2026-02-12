from __future__ import annotations

import contextlib
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from kagan.core.adapters.db.schema import Repo, WorkspaceRepo
    from kagan.core.adapters.db.schema import Workspace as DbWorkspace
    from kagan.core.adapters.git.worktrees import GitWorktreeProtocol


class WorkspaceMergeOpsMixin:
    _git: GitWorktreeProtocol
    _merge_worktrees_dir: Path

    async def _get_latest_workspace_for_task(self, task_id: str) -> DbWorkspace | None:
        del task_id
        raise NotImplementedError

    async def _get_primary_workspace_repo(self, workspace_id: str) -> WorkspaceRepo | None:
        del workspace_id
        raise NotImplementedError

    async def _get_workspace_repo_rows(self, workspace_id: str) -> list[tuple[WorkspaceRepo, Repo]]:
        del workspace_id
        raise NotImplementedError

    async def get_agent_working_dir(self, workspace_id: str) -> Path:
        del workspace_id
        raise NotImplementedError

    async def get_merge_worktree_path(self, task_id: str, base_branch: str = "main") -> Path:
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            raise ValueError(f"Workspace not found for task {task_id}")
        primary_repo = await self._get_primary_workspace_repo(workspace.id)
        if primary_repo is None:
            raise ValueError(f"Workspace {workspace.id} has no repos")
        return await self._ensure_merge_worktree(primary_repo.repo_id, base_branch, workspace)

    async def prepare_merge_conflicts(
        self, task_id: str, base_branch: str = "main"
    ) -> tuple[bool, str]:
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            return False, f"Workspace not found for task {task_id}"
        branch_name = workspace.branch_name

        primary_repo = await self._get_primary_workspace_repo(workspace.id)
        if primary_repo is None:
            return False, f"Workspace {workspace.id} has no repos"
        merge_path = await self._ensure_merge_worktree(primary_repo.repo_id, base_branch, workspace)
        if await self._merge_in_progress(merge_path):
            return True, "Merge already in progress"

        try:
            await self._reset_merge_worktree(merge_path, base_branch)
            await self._git.run_git(
                "merge",
                "--squash",
                branch_name,
                cwd=merge_path,
                check=False,
            )
            status_out, _ = await self._git.run_git("status", "--porcelain", cwd=merge_path)
            if any(marker in status_out for marker in ("UU ", "AA ", "DD ")):
                return True, "Merge conflicts prepared"

            await self._git.run_git("merge", "--abort", cwd=merge_path, check=False)
            return False, "No conflicts detected"
        except Exception as exc:
            return False, f"Prepare failed: {exc}"

    async def rebase_onto_base(
        self, task_id: str, base_branch: str = "main"
    ) -> tuple[bool, str, list[str]]:
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            return False, f"Workspace not found for task {task_id}", []

        repo_rows = await self._get_workspace_repo_rows(workspace.id)
        if not repo_rows:
            return False, f"Workspace {workspace.id} has no repos", []

        try:
            for workspace_repo, repo in repo_rows:
                if not workspace_repo.worktree_path:
                    continue
                target_branch = workspace_repo.target_branch or base_branch
                wt_path = Path(workspace_repo.worktree_path)
                has_remote = await self._has_remote(wt_path)
                if has_remote:
                    await self._git.run_git(
                        "fetch", "origin", target_branch, cwd=wt_path, check=False
                    )
                rebase_ref = f"origin/{target_branch}" if has_remote else target_branch

                if await self._rebase_in_progress(wt_path):
                    conflict_files = await self._collect_rebase_conflicts(wt_path, repo.name)
                    return (
                        False,
                        (
                            f"Rebase already in progress for {repo.name}; resolve conflicts or "
                            "abort the rebase"
                        ),
                        conflict_files,
                    )

                status_out, _ = await self._git.run_git("status", "--porcelain", cwd=wt_path)
                if status_out.strip():
                    await self._git.run_git("add", "-A", cwd=wt_path)
                    await self._git.run_git(
                        "commit",
                        "-m",
                        f"chore: adding uncommitted agent changes ({repo.name})",
                        cwd=wt_path,
                    )

                stdout, stderr = await self._git.run_git(
                    "rebase",
                    rebase_ref,
                    cwd=wt_path,
                    check=False,
                )
                if await self._rebase_in_progress(wt_path):
                    conflict_files = await self._collect_rebase_conflicts(wt_path, repo.name)
                    return (
                        False,
                        (
                            f"Rebase conflict in {repo.name} ({len(conflict_files)} file(s)); "
                            "resolve or abort"
                        ),
                        conflict_files,
                    )

                combined_output = f"{stdout}\n{stderr}".strip().lower()
                if "fatal:" in combined_output or "error:" in combined_output:
                    failure = combined_output.strip() or "rebase failed"
                    return False, f"Rebase failed in {repo.name}: {failure}", []

            return True, f"Successfully rebased onto {base_branch}", []
        except Exception as exc:
            with contextlib.suppress(Exception):
                for workspace_repo, _repo in repo_rows:
                    if not workspace_repo.worktree_path:
                        continue
                    await self._git.run_git(
                        "rebase",
                        "--abort",
                        cwd=Path(workspace_repo.worktree_path),
                        check=False,
                    )
            return False, f"Rebase failed: {exc}", []

    async def abort_rebase(self, task_id: str) -> tuple[bool, str]:
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            return False, f"Workspace not found for task {task_id}"

        repo_rows = await self._get_workspace_repo_rows(workspace.id)
        if not repo_rows:
            return False, f"Workspace {workspace.id} has no repos"

        aborted: list[str] = []
        for workspace_repo, repo in repo_rows:
            if not workspace_repo.worktree_path:
                continue
            wt_path = Path(workspace_repo.worktree_path)
            if not await self._rebase_in_progress(wt_path):
                continue
            await self._git.run_git("rebase", "--abort", cwd=wt_path, check=False)
            aborted.append(repo.name)

        if not aborted:
            return False, "No rebase in progress"

        aborted_list = ", ".join(aborted)
        return True, f"Aborted rebase in {len(aborted)} repo(s): {aborted_list}"

    async def get_files_changed_on_base(self, task_id: str, base_branch: str = "main") -> list[str]:
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            return []

        repo_rows = await self._get_workspace_repo_rows(workspace.id)
        try:
            files: list[str] = []
            for workspace_repo, repo in repo_rows:
                if not workspace_repo.worktree_path:
                    continue
                target_branch = workspace_repo.target_branch or base_branch
                wt_path = Path(workspace_repo.worktree_path)
                merge_base_out, _ = await self._git.run_git(
                    "merge-base",
                    "HEAD",
                    f"origin/{target_branch}",
                    cwd=wt_path,
                    check=False,
                )
                if not merge_base_out.strip():
                    continue

                merge_base = merge_base_out.strip()
                diff_out, _ = await self._git.run_git(
                    "diff",
                    "--name-only",
                    merge_base,
                    f"origin/{target_branch}",
                    cwd=wt_path,
                )
                if not diff_out.strip():
                    continue

                repo_files = [line.strip() for line in diff_out.split("\n") if line.strip()]
                files.extend([f"{repo.name}:{path}" for path in repo_files])

            return files
        except Exception:
            return []

    async def _ensure_merge_worktree(
        self, repo_id: str, base_branch: str, workspace: DbWorkspace
    ) -> Path:
        merge_path = self._merge_worktrees_dir / repo_id
        merge_path.parent.mkdir(parents=True, exist_ok=True)

        if merge_path.exists():
            return merge_path

        worktree_path = await self.get_agent_working_dir(workspace.id)
        repo_root = self._resolve_repo_root(worktree_path)
        await self._git.run_git(
            "worktree",
            "add",
            "-B",
            self._merge_branch_name(repo_id),
            str(merge_path),
            base_branch,
            cwd=repo_root,
        )
        return merge_path

    async def _reset_merge_worktree(self, merge_path: Path, base_branch: str) -> Path:
        await self._git.run_git("fetch", "origin", base_branch, cwd=merge_path, check=False)

        base_ref = base_branch
        if await self._ref_exists(f"refs/remotes/origin/{base_branch}", cwd=merge_path):
            base_ref = f"origin/{base_branch}"

        await self._git.run_git(
            "checkout", self._merge_branch_name(merge_path.name), cwd=merge_path
        )
        await self._git.run_git("reset", "--hard", base_ref, cwd=merge_path)
        return merge_path

    async def _ref_exists(self, ref: str, cwd: Path) -> bool:
        stdout, _ = await self._git.run_git(
            "rev-parse",
            "--verify",
            "--quiet",
            ref,
            cwd=cwd,
            check=False,
        )
        return bool(stdout.strip())

    async def _merge_in_progress(self, cwd: Path) -> bool:
        stdout, _ = await self._git.run_git(
            "rev-parse",
            "-q",
            "--verify",
            "MERGE_HEAD",
            cwd=cwd,
            check=False,
        )
        return bool(stdout.strip())

    async def _rebase_in_progress(self, cwd: Path) -> bool:
        stdout, _ = await self._git.run_git(
            "rev-parse",
            "-q",
            "--verify",
            "REBASE_HEAD",
            cwd=cwd,
            check=False,
        )
        if stdout.strip():
            return True

        for path_name in ("rebase-apply", "rebase-merge"):
            path_out, _ = await self._git.run_git(
                "rev-parse",
                "--git-path",
                path_name,
                cwd=cwd,
                check=False,
            )
            if path_out.strip() and Path(path_out.strip()).exists():
                return True

        return False

    async def _collect_rebase_conflicts(self, cwd: Path, repo_name: str) -> list[str]:
        stdout, _ = await self._git.run_git(
            "diff",
            "--name-only",
            "--diff-filter=U",
            cwd=cwd,
            check=False,
        )
        files = [line.strip() for line in stdout.splitlines() if line.strip()]
        if not files:
            status_out, _ = await self._git.run_git("status", "--porcelain", cwd=cwd, check=False)
            files = []
            for line in status_out.splitlines():
                if line.startswith(("UU ", "AA ", "DD ", "AU ", "UA ", "DU ", "UD ")):
                    files.append(line[3:].strip())

        return [f"{repo_name}:{path}" for path in files]

    def _resolve_repo_root(self, worktree_path: Path) -> Path:
        git_file = worktree_path / ".git"
        if not git_file.exists():
            return worktree_path
        content = git_file.read_text().strip()
        if not content.startswith("gitdir:"):
            return worktree_path
        git_dir = content.split(":", 1)[1].strip()
        return Path(git_dir).parent.parent.parent

    async def _has_remote(self, cwd: Path) -> bool:
        """Check if repo has an origin remote."""
        stdout, _ = await self._git.run_git("remote", cwd=cwd, check=False)
        return "origin" in {r.strip() for r in stdout.splitlines() if r.strip()}

    def _merge_branch_name(self, repo_id: str) -> str:
        return f"kagan/merge-worktree-{repo_id[:8]}"
