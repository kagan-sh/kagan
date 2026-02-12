from __future__ import annotations

import platform
import subprocess

import pytest

from kagan.core.git_utils import init_git_repo

_IS_WINDOWS = platform.system() == "Windows"


@pytest.mark.asyncio
@pytest.mark.skipif(_IS_WINDOWS, reason="core.excludesfile test relies on Unix path conventions")
async def test_init_git_repo_forces_stage_when_gitignore_is_globally_ignored(
    tmp_path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Global ignore patterns must not prevent bootstrapping .gitignore."""
    repo_root = tmp_path / "repo"
    repo_root.mkdir()

    excludes_file = tmp_path / "global_excludes"
    excludes_file.write_text(".gitignore\n")

    global_gitconfig = tmp_path / "gitconfig"
    global_gitconfig.write_text(
        "\n".join(
            [
                "[user]",
                "  name = Kagan Test",
                "  email = kagan@example.com",
                "[core]",
                f"  excludesfile = {excludes_file}",
                "",
            ]
        )
    )

    monkeypatch.setenv("GIT_CONFIG_GLOBAL", str(global_gitconfig))

    result = await init_git_repo(repo_root, base_branch="main")

    assert result.success is True
    assert (repo_root / ".git").exists()
    assert (repo_root / ".gitignore").exists()

    head = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repo_root,
        check=False,
        capture_output=True,
        text=True,
    )
    assert head.returncode == 0
    assert head.stdout.strip()
