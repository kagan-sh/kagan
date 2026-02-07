"""Conflict resolution instruction builder for rebase conflicts."""

from __future__ import annotations


def build_conflict_resolution_instructions(
    source_branch: str,
    target_branch: str,
    conflict_files: list[str],
    repo_name: str = "",
) -> str:
    """Build agent-ready instructions for resolving rebase conflicts.

    Args:
        source_branch: The branch being rebased (task branch).
        target_branch: The branch being rebased onto (base branch).
        conflict_files: List of conflicted file paths (may be prefixed with repo name).
        repo_name: Optional repository name for context.

    Returns:
        Formatted instruction string for the agent.
    """
    repo_ctx = f" in {repo_name}" if repo_name else ""
    file_list = "\n".join(f"  - {f}" for f in conflict_files) if conflict_files else "  (unknown)"

    return f"""## Rebase Conflict Resolution Required

A rebase of `{source_branch}` onto `{target_branch}`{repo_ctx} produced conflicts
in {len(conflict_files)} file(s):

{file_list}

### Steps to resolve

1. Run `git rebase {target_branch}` to begin the rebase.
2. For each conflicted file, open it, resolve the conflict markers
   (`<<<<<<<`, `=======`, `>>>>>>>`), and save.
3. Stage resolved files: `git add <file>`.
4. Continue the rebase: `GIT_EDITOR=true git rebase --continue`.
5. Repeat steps 2-4 if additional commits produce conflicts.

### Important

- Preserve the intent of both sides when resolving conflicts.
- Run any relevant tests after resolving to verify correctness.
- Do NOT use `git rebase --skip` unless you are certain the commit is unnecessary.
"""
