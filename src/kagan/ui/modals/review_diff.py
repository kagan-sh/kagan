"""Diff rendering and parsing for the review modal."""

from __future__ import annotations

import re
from typing import TYPE_CHECKING, cast

from textual.containers import Horizontal
from textual.widgets import DataTable, RichLog, Static

from kagan.constants import MODAL_TITLE_MAX_LENGTH
from kagan.ui.utils.diff import colorize_diff_line

if TYPE_CHECKING:
    from kagan.app import KaganApp
    from kagan.services.diffs import RepoDiff
    from kagan.ui.modals.review import ReviewModal


class ReviewDiffMixin:
    """Diff and commit presentation helpers."""

    def _populate_commits(self: ReviewModal, commits: list[str]) -> None:
        table = self.query_one("#commits-table", DataTable)
        table.clear()
        if not table.columns:
            table.add_columns("Repo", "Hash", "Message")
        table.cursor_type = "row"
        table.zebra_stripes = True

        if not commits:
            table.add_row("—", "—", "No commits found")
            return

        for line in commits:
            repo, sha, message = self._parse_commit_line(line)
            table.add_row(repo or "—", sha or "—", message or "—")

    async def _populate_diff_pane(self: ReviewModal, workspaces: list) -> None:
        app = cast("KaganApp", self.app)
        diff_service = getattr(app.ctx, "diff_service", None)

        if workspaces and diff_service is not None:
            diffs = await diff_service.get_all_diffs(workspaces[0].id)
            self._populate_file_diffs(diffs)
            total_additions = sum(diff.total_additions for diff in diffs)
            total_deletions = sum(diff.total_deletions for diff in diffs)
            total_files = sum(len(diff.files) for diff in diffs)
            self._set_stats(total_additions, total_deletions, total_files)
            return

        total_additions, total_deletions, total_files = self._parse_diff_totals(self._diff_stats)
        self._set_stats(total_additions, total_deletions, total_files)
        self._render_diff_text(self._diff_text)

    def _populate_file_diffs(self: ReviewModal, diffs: list[RepoDiff]) -> None:
        table = self.query_one("#diff-files", DataTable)
        table.clear()
        if not table.columns:
            table.add_columns("File", "+", "-")
        table.cursor_type = "row"
        table.zebra_stripes = True

        self._file_diffs.clear()
        multi_repo = len(diffs) > 1
        for diff in diffs:
            for file in diff.files:
                key = f"{diff.repo_name}:{file.path}"
                self._file_diffs[key] = file
                label = f"{diff.repo_name}/{file.path}" if multi_repo else file.path
                table.add_row(label, str(file.additions), str(file.deletions), key=key)

        if not self._file_diffs:
            table.add_class("hidden")
            self._render_diff_text(self._diff_text)
            return

        table.remove_class("hidden")
        first_key = next(iter(self._file_diffs))
        self._show_file_diff(first_key)

    def _show_file_diff(self: ReviewModal, key: str) -> None:
        file_diff = self._file_diffs.get(key)
        if file_diff is None:
            self._render_diff_text(self._diff_text)
            return
        self._render_diff_text(file_diff.diff_content)

    def _render_diff_text(self: ReviewModal, diff_text: str) -> None:
        diff_log = self.query_one("#diff-log", RichLog)
        diff_log.clear()
        for line in diff_text.splitlines() or ["(No diff available)"]:
            diff_log.write(colorize_diff_line(line))
        diff_log.scroll_home(animate=False)

    def _set_stats(self: ReviewModal, additions: int, deletions: int, files: int) -> None:
        self.query_one("#review-stats", Horizontal).remove_class("hidden")
        self.query_one("#stat-additions", Static).update(f"+ {additions} Additions")
        self.query_one("#stat-deletions", Static).update(f"- {deletions} Deletions")
        self.query_one("#stat-files", Static).update(f"{files} Files Changed")

    def _parse_diff_totals(self: ReviewModal, diff_stats: str) -> tuple[int, int, int]:
        if not diff_stats:
            return 0, 0, 0
        pattern = re.compile(r"\+(\d+)\s+-(\d+)\s+\((\d+)\s+file")
        total_line = ""
        for line in diff_stats.splitlines():
            if line.lower().startswith("total:"):
                total_line = line
                break
        if total_line:
            match = pattern.search(total_line)
            if match:
                return int(match.group(1)), int(match.group(2)), int(match.group(3))
        additions = deletions = files = 0
        for line in diff_stats.splitlines():
            match = pattern.search(line)
            if match:
                additions += int(match.group(1))
                deletions += int(match.group(2))
                files += int(match.group(3))
        return additions, deletions, files

    def _parse_commit_line(self: ReviewModal, line: str) -> tuple[str, str, str]:
        repo = ""
        rest = line.strip()
        if rest.startswith("["):
            end = rest.find("]")
            if end != -1:
                repo = rest[1:end]
                rest = rest[end + 1 :].strip()
        parts = rest.split(" ", 1)
        sha = parts[0] if parts else ""
        message = parts[1] if len(parts) > 1 else ""
        return repo, sha, message

    async def _open_diff_modal(self: ReviewModal) -> None:
        from kagan.ui.modals import DiffModal

        app = cast("KaganApp", self.app)
        workspaces = await self._worktree.list_workspaces(task_id=self._task_model.id)
        title = (
            f"Diff: {self._task_model.short_id} {self._task_model.title[:MODAL_TITLE_MAX_LENGTH]}"
        )
        diff_service = getattr(app.ctx, "diff_service", None)

        if not workspaces or diff_service is None:
            diff_text = self._diff_text or await self._worktree.get_diff(
                self._task_model.id, self._base_branch
            )
            await self.app.push_screen(
                DiffModal(title=title, diff_text=diff_text, task=self._task_model),
                callback=self._on_diff_result,
            )
            return

        diffs = await diff_service.get_all_diffs(workspaces[0].id)
        await self.app.push_screen(
            DiffModal(title=title, diffs=diffs, task=self._task_model),
            callback=self._on_diff_result,
        )

    def _on_diff_result(self: ReviewModal, result: str | None) -> None:
        if result == "approve":
            self.action_approve()
        elif result == "reject":
            self.action_reject()
