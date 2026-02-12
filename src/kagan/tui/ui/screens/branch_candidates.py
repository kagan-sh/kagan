from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from kagan.core.git_utils import list_local_branches
from kagan.tui.ui.modals import BaseBranchModal
from kagan.tui.ui.screen_result import await_screen_result

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path
    from typing import Any

BRANCH_LOOKUP_TIMEOUT_WARNING = "Branch lookup timed out. Enter branch manually."


async def load_branch_candidates(
    project_root: Path,
    *,
    timeout_seconds: float,
    warn: Callable[[str], None],
) -> list[str]:
    """Load local branch candidates for branch pickers with timeout handling."""
    try:
        return await asyncio.wait_for(
            list_local_branches(project_root),
            timeout=timeout_seconds,
        )
    except TimeoutError:
        warn(BRANCH_LOOKUP_TIMEOUT_WARNING)
        return []


async def choose_branch_with_modal(
    app: Any,
    *,
    project_root: Path,
    current_value: str,
    title: str,
    description: str,
    timeout_seconds: float,
    warn: Callable[[str], None],
) -> str | None:
    """Open a branch picker modal after loading local branch candidates."""
    branches = await load_branch_candidates(
        project_root,
        timeout_seconds=timeout_seconds,
        warn=warn,
    )
    return await await_screen_result(
        app,
        BaseBranchModal(
            branches=branches,
            current_value=current_value,
            title=title,
            description=description,
        ),
    )
