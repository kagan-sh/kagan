# Multi-Repo Projects

Kagan supports projects that span multiple git repositories. Each workspace can create worktrees
for all or a subset of repos, with per-repo diffs and merges.

## Create A Project

1. Launch Kagan.
2. From the welcome screen, choose **New Project**.
3. Enter a project name and add repository paths.
4. Mark a primary repo (first repo is primary by default).

## Start A Workspace

1. Open a task card.
2. Choose **Start Workspace**.
3. Select the repos to include for this task.
4. Kagan creates worktrees for the selected repos under the workspace directory.

## View Diffs And Merge

The workspace view shows per-repo status:

- **Diff** opens a tabbed view with per-repo diffs.
- **Merge** merges a single repo or all repos, depending on selection.

## Data Storage

Kagan stores all data outside your repositories:

- Database: `~/.local/share/kagan/kagan.db` (XDG-compliant)
- Config: `~/.config/kagan/config.toml`
- Worktrees: system temp directory (e.g. `/var/tmp/kagan/worktrees/`)

No `.kagan/` directory is created inside your repos.

## Alpha Note (No Migration)

This is an alpha feature set. Legacy single-repo data is not migrated automatically. If you
used older versions, start with a fresh database in the XDG location.
