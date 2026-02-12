"""Merge dialog with per-repo support."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Horizontal, Vertical
from textual.widgets import Button, Checkbox, Label, Select, Static

from kagan.core.services.merges import MergeResult, MergeStrategy
from kagan.tui.ui.modals.base import KaganModalScreen

if TYPE_CHECKING:
    from textual.app import ComposeResult


class RepoMergeRow(Static):
    """Row for a single repo in the merge dialog."""

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
        with Horizontal(classes="merge-row"):
            yield Checkbox(
                self.repo_name,
                value=self.has_changes,
                disabled=not self.has_changes,
                id=f"check-{self.repo_id}",
            )
            yield Label(f"-> {self.target_branch}", classes="target-branch")

            if self.has_changes:
                yield Label(
                    f"+{self.diff_stats.get('insertions', 0)} "
                    f"-{self.diff_stats.get('deletions', 0)}",
                    classes="diff-stats",
                )
            else:
                yield Label("No changes", classes="no-changes")


class MergeDialog(KaganModalScreen[list[MergeResult] | None]):
    """Dialog for merging workspace changes."""

    def __init__(self, workspace_id: str, repos: list[dict]) -> None:
        super().__init__()
        self.workspace_id = workspace_id
        self.repos = repos
        self.results: list[MergeResult] | None = None

    def compose(self) -> ComposeResult:
        with Vertical(id="merge-container"):
            yield Label("MERGE WORKSPACE", classes="dialog-title")
            yield Static("Select repos to merge:", classes="dialog-subtitle")

            with Vertical(id="repo-list"):
                for repo in self.repos:
                    yield RepoMergeRow(
                        repo_id=repo["repo_id"],
                        repo_name=repo["repo_name"],
                        target_branch=repo["target_branch"],
                        has_changes=repo["has_changes"],
                        diff_stats=repo.get("diff_stats"),
                    )

            yield Select(
                [
                    ("Direct Merge", MergeStrategy.DIRECT),
                    ("Create Pull Request", MergeStrategy.PULL_REQUEST),
                ],
                id="strategy-select",
                prompt="Merge Strategy",
            )

            with Horizontal(id="button-row"):
                yield Button("Cancel", variant="default", id="cancel-btn")
                yield Button("Merge Selected", variant="primary", id="merge-btn")

    async def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "cancel-btn":
            self.dismiss(None)
        elif event.button.id == "merge-btn":
            await self._do_merge()

    async def _do_merge(self) -> None:
        """Perform the merge operation."""
        strategy_select = self.query_one("#strategy-select", Select)
        raw_strategy = strategy_select.value
        strategy = raw_strategy if isinstance(raw_strategy, MergeStrategy) else MergeStrategy.DIRECT

        selected_repos: list[str] = []
        for repo in self.repos:
            checkbox = self.query_one(f"#check-{repo['repo_id']}", Checkbox)
            if checkbox.value:
                selected_repos.append(repo["repo_id"])

        if not selected_repos:
            self.notify("No repos selected", severity="warning")
            return

        results: list[MergeResult] = []
        for repo_id in selected_repos:
            results.append(
                await self.ctx.api.merge_repo(
                    self.workspace_id,
                    repo_id,
                    strategy=strategy,
                )
            )

        self.results = results
        self.dismiss(results)
