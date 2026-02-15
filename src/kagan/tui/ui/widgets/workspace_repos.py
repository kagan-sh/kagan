"""Widget to display repos in a workspace."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from textual.containers import Horizontal, Vertical
from textual.widgets import Button, Label, Static

if TYPE_CHECKING:
    from collections.abc import Awaitable, Callable

    from textual.app import ComposeResult


class WorkspaceRepoItem(Static):
    """Display a single repo in a workspace."""

    def __init__(
        self,
        repo_id: str,
        repo_name: str,
        target_branch: str,
        has_changes: bool,
        diff_stats: dict | None,
    ) -> None:
        super().__init__()
        self.repo_id = repo_id
        self.repo_name = repo_name
        self.target_branch = target_branch
        self.has_changes = has_changes
        self.diff_stats = diff_stats or {}

    def compose(self) -> ComposeResult:
        with Horizontal(classes="repo-item"):
            status = "●" if self.has_changes else "○"
            status_class = "has-changes" if self.has_changes else "no-changes"

            yield Label(f"{status} {self.repo_name}", classes=f"repo-name {status_class}")
            yield Label(f"→ {self.target_branch}", classes="target-branch")

            if self.has_changes:
                stats = self.diff_stats
                yield Label(
                    f"+{stats.get('insertions', 0)} -{stats.get('deletions', 0)}",
                    classes="diff-stats",
                )
                yield Button("Diff", classes="btn-diff", id=f"diff-{self.repo_id}")
                yield Button("Merge", classes="btn-merge", id=f"merge-{self.repo_id}")
            else:
                yield Label("No changes", classes="no-changes-label")


class WorkspaceReposWidget(Static):
    """Widget displaying all repos in a workspace."""

    def __init__(
        self,
        workspace_id: str,
        *,
        load_repos: Callable[[str], Awaitable[list[dict[str, Any]]]],
        load_repo_diff: Callable[[str, str], Awaitable[Any | None]] | None,
    ) -> None:
        super().__init__()
        self.workspace_id = workspace_id
        self._load_repos = load_repos
        self._load_repo_diff = load_repo_diff
        self.repos: list[dict[str, Any]] = []

    def compose(self) -> ComposeResult:
        yield Label("WORKSPACE REPOS", classes="section-header")
        with Vertical(id="repo-list"):
            pass

    async def on_mount(self) -> None:
        await self.refresh_repos()

    async def refresh_repos(self) -> None:
        try:
            self.repos = await self._load_repos(self.workspace_id)
        except Exception as exc:
            self.notify(f"Failed to load workspace repos: {exc}", severity="warning")
            return

        repo_list = self.query_one("#repo-list", Vertical)
        await repo_list.remove_children()

        for repo in self.repos:
            repo_id = str(repo.get("repo_id", "")).strip()
            if not repo_id:
                continue
            repo_name = str(repo.get("repo_name", repo_id))
            target_branch = str(repo.get("target_branch", "unknown"))
            has_changes = bool(repo.get("has_changes", False))
            diff_stats = repo.get("diff_stats")
            item = WorkspaceRepoItem(
                repo_id=repo_id,
                repo_name=repo_name,
                target_branch=target_branch,
                has_changes=has_changes,
                diff_stats=diff_stats if isinstance(diff_stats, dict) else None,
            )
            await repo_list.mount(item)

    async def on_button_pressed(self, event: Button.Pressed) -> None:
        if not event.button.id:
            return

        if event.button.id.startswith("diff-"):
            repo_id = event.button.id.removeprefix("diff-")
            await self._open_diff(repo_id)
        elif event.button.id.startswith("merge-"):
            await self._open_merge_dialog()

    async def _open_diff(self, repo_id: str) -> None:
        from kagan.tui.ui.modals import DiffModal

        if self._load_repo_diff is None:
            self.notify("Diff service unavailable", severity="warning")
            return
        repo_diff = await self._load_repo_diff(self.workspace_id, repo_id)
        if repo_diff is None:
            self.notify("No diff available for this repository", severity="warning")
            return
        await self.app.push_screen(DiffModal(diffs=[repo_diff]))

    async def _open_merge_dialog(self) -> None:
        from kagan.tui.ui.modals import MergeDialog

        await self.app.push_screen(MergeDialog(self.workspace_id, self.repos))
