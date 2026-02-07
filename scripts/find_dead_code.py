"""Find legacy single-repo patterns after multi-repo refactor."""

from __future__ import annotations

import sys
from pathlib import Path

LEGACY_PATTERNS = [
    "task.repo_id",
    "workspace.repo_id",
    "Task.repo_id",
    "Workspace.repo_id",
    "get_repo_path",
    "get_single_repo",
    "task_repo_id",
    "workspace_repo_id",
    "dual_write",
    "migrate_single",
    "migrate_to_xdg",
]


def find_legacy_usage(source_dir: Path) -> list[tuple[Path, int, str]]:
    """Find usages of legacy patterns."""
    findings: list[tuple[Path, int, str]] = []
    for py_file in source_dir.rglob("*.py"):
        if "__pycache__" in py_file.parts:
            continue
        content = py_file.read_text(encoding="utf-8")
        for line_no, line in enumerate(content.splitlines(), 1):
            for pattern in LEGACY_PATTERNS:
                if pattern in line:
                    findings.append((py_file, line_no, line.strip()))
    return findings


def main() -> int:
    source_dir = Path("src/kagan")
    findings = find_legacy_usage(source_dir)

    if findings:
        print("LEGACY PATTERNS FOUND:")
        for path, line_no, content in findings:
            print(f"  {path}:{line_no}: {content}")
        return 1

    print("No legacy patterns found.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
