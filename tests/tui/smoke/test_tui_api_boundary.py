"""Enforce that TUI code accesses services through the API boundary only.

Direct ``self.ctx.<service>`` access is forbidden.  All service access must go
through either ``self.ctx.api.<method>()`` (preferred) or the bridge pattern
``self.ctx.api.ctx.<service>.<method>()`` (intermediate migration step).
"""

from __future__ import annotations

import re
from pathlib import Path

TUI_SRC = Path("src/kagan/tui")

# Services that MUST be accessed through the API boundary (or api.ctx bridge).
_SERVICES = (
    "task_service",
    "session_service",
    "runtime_service",
    "workspace_service",
    "merge_service",
    "review_service",
    "diff_service",
    "job_service",
    "automation_service",
    "project_service",
    "agent_health",
    "execution_service",
    "audit_repository",
    "planner_repository",
)

# Matches direct `ctx.<service>` but NOT `api.ctx.<service>` (bridge pattern).
_DIRECT_ACCESS = re.compile(r"(?<!api\.)ctx\.(" + "|".join(_SERVICES) + r")\b")


def test_no_direct_service_access_in_tui() -> None:
    """TUI source files must not access services directly on ctx."""
    violations: list[str] = []
    for py_file in sorted(TUI_SRC.rglob("*.py")):
        rel = str(py_file.relative_to(TUI_SRC))
        for i, line in enumerate(py_file.read_text().splitlines(), 1):
            # Skip comments
            stripped = line.lstrip()
            if stripped.startswith("#"):
                continue
            if _DIRECT_ACCESS.search(line):
                violations.append(f"  {rel}:{i}: {stripped}")

    assert not violations, (
        "TUI files must not access services directly on ctx. "
        "Use ctx.api.<method>() or ctx.api.ctx.<service> as bridge.\n" + "\n".join(violations)
    )
