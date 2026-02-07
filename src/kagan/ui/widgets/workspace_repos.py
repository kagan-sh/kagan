"""Widget to display repos in a workspace."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

from textual.containers import Horizontal, Vertical
from textual.widgets import Button, Label, Static

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.app import KaganApp


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

    def __init__(self, workspace_id: str) -> None:
        super().__init__()
        self.workspace_id = workspace_id
        self.repos: list[dict] = []

    def compose(self) -> ComposeResult:
        yield Label("WORKSPACE REPOS", classes="section-header")
        with Vertical(id="repo-list"):
            pass

    async def on_mount(self) -> None:
        await self.refresh_repos()

    async def refresh_repos(self) -> None:
        """Refresh the repo list."""
        app = cast("KaganApp", self.app)
        workspace_service = app.ctx.workspace_service
        self.repos = await workspace_service.get_workspace_repos(self.workspace_id)

        repo_list = self.query_one("#repo-list", Vertical)
        await repo_list.remove_children()

        for repo in self.repos:
            item = WorkspaceRepoItem(
                repo_id=repo["repo_id"],
                repo_name=repo["repo_name"],
                target_branch=repo["target_branch"],
                has_changes=repo["has_changes"],
                diff_stats=repo.get("diff_stats"),
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
        from kagan.ui.modals import DiffModal

        app = cast("KaganApp", self.app)
        diff_service = app.ctx.diff_service
        repo_diff = await diff_service.get_repo_diff(self.workspace_id, repo_id)
        await self.app.push_screen(DiffModal(diffs=[repo_diff]))

    async def _open_merge_dialog(self) -> None:
        from kagan.ui.modals import MergeDialog

        await self.app.push_screen(MergeDialog(self.workspace_id, self.repos))
