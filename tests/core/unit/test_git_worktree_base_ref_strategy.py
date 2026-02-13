from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.core.adapters.git.worktrees import GitWorktreeAdapter

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path

    from kagan.core.config import WorktreeBaseRefStrategyLiteral


def _make_adapter(
    *,
    strategy: WorktreeBaseRefStrategyLiteral,
    responses: dict[tuple[tuple[str, ...], bool | None], tuple[str, str]],
) -> tuple[GitWorktreeAdapter, list[tuple[tuple[str, ...], bool]]]:
    adapter = GitWorktreeAdapter(base_ref_strategy=strategy)
    calls: list[tuple[tuple[str, ...], bool]] = []

    async def fake_run_git(
        cwd: Path,
        args: Sequence[str],
        *,
        check: bool = True,
    ) -> tuple[str, str]:
        del cwd
        command = tuple(args)
        calls.append((command, check))
        if (command, check) in responses:
            return responses[(command, check)]
        if (command, None) in responses:
            return responses[(command, None)]
        if check:
            raise RuntimeError(f"Unexpected git command: {' '.join(args)}")
        return "", ""

    adapter._run_git = fake_run_git  # type: ignore[method-assign]
    return adapter, calls


async def test_create_worktree_uses_remote_when_strategy_remote() -> None:
    adapter, calls = _make_adapter(
        strategy="remote",
        responses={
            (("remote",), False): ("origin\n", ""),
            (("fetch", "origin", "main"), False): ("", ""),
            (("rev-parse", "--verify", "--quiet", "refs/heads/main"), False): ("abc\n", ""),
            (("rev-parse", "--verify", "--quiet", "refs/remotes/origin/main"), False): (
                "def\n",
                "",
            ),
            (("worktree", "add", "-b", "feat-1", "/tmp/worktree", "origin/main"), True): ("", ""),
        },
    )

    await adapter.create_worktree("/tmp/repo", "/tmp/worktree", "feat-1", "main")

    assert (("fetch", "origin", "main"), False) in calls
    assert (("worktree", "add", "-b", "feat-1", "/tmp/worktree", "origin/main"), True) in calls


async def test_create_worktree_uses_local_when_strategy_local() -> None:
    adapter, calls = _make_adapter(
        strategy="local",
        responses={
            (("rev-parse", "--verify", "--quiet", "refs/heads/main"), False): ("abc\n", ""),
            (("worktree", "add", "-b", "feat-1", "/tmp/worktree", "main"), True): ("", ""),
        },
    )

    await adapter.create_worktree("/tmp/repo", "/tmp/worktree", "feat-1", "main")

    assert not any(command[0] in {"remote", "fetch"} for command, _check in calls)
    assert (("worktree", "add", "-b", "feat-1", "/tmp/worktree", "main"), True) in calls


async def test_create_worktree_prefers_local_when_ahead_under_local_if_ahead() -> None:
    adapter, calls = _make_adapter(
        strategy="local_if_ahead",
        responses={
            (("remote",), False): ("origin\n", ""),
            (("fetch", "origin", "main"), False): ("", ""),
            (("rev-parse", "--verify", "--quiet", "refs/heads/main"), False): ("abc\n", ""),
            (("rev-parse", "--verify", "--quiet", "refs/remotes/origin/main"), False): (
                "def\n",
                "",
            ),
            (("rev-list", "--count", "refs/remotes/origin/main..refs/heads/main"), False): (
                "2\n",
                "",
            ),
            (("worktree", "add", "-b", "feat-1", "/tmp/worktree", "main"), True): ("", ""),
        },
    )

    await adapter.create_worktree("/tmp/repo", "/tmp/worktree", "feat-1", "main")

    assert (
        ("rev-list", "--count", "refs/remotes/origin/main..refs/heads/main"),
        False,
    ) in calls
    assert (("worktree", "add", "-b", "feat-1", "/tmp/worktree", "main"), True) in calls


async def test_create_worktree_prefers_remote_when_not_ahead_under_local_if_ahead() -> None:
    adapter, calls = _make_adapter(
        strategy="local_if_ahead",
        responses={
            (("remote",), False): ("origin\n", ""),
            (("fetch", "origin", "main"), False): ("", ""),
            (("rev-parse", "--verify", "--quiet", "refs/heads/main"), False): ("abc\n", ""),
            (("rev-parse", "--verify", "--quiet", "refs/remotes/origin/main"), False): (
                "def\n",
                "",
            ),
            (("rev-list", "--count", "refs/remotes/origin/main..refs/heads/main"), False): (
                "0\n",
                "",
            ),
            (("worktree", "add", "-b", "feat-1", "/tmp/worktree", "origin/main"), True): ("", ""),
        },
    )

    await adapter.create_worktree("/tmp/repo", "/tmp/worktree", "feat-1", "main")

    assert (("worktree", "add", "-b", "feat-1", "/tmp/worktree", "origin/main"), True) in calls
