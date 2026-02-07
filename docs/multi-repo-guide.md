# Multi-Repo Projects

Kagan supports projects that span multiple git repositories. Each workspace can create worktrees
for all or a subset of repos, with per-repo diffs and merges.

## Create A Project

1. Launch Kagan.
1. From the Welcome screen, choose **New Project** with ++n++.
1. Enter a project name and add repository paths.
1. The first repo is the primary repo by default.

## Pick An Active Repo

Use ++ctrl+r++ to open the repo picker anytime.

In the picker, use ++up++/++down++ or ++j++/++k++ to navigate, ++enter++ to select, ++n++ to add a repo, and ++esc++ to cancel.

## Base Branches

Use ++b++ to set a task base branch and ++shift+b++ to set a global default branch. These branches are used for diffs and merges.

## View Diffs And Merge

Open Task Details with ++v++ and look for the **Workspace Repos** section.

Use the **Diff** button to open a per-repo diff and the **Merge** button to open the merge dialog. The merge dialog uses buttons and checkboxes for selection.

## Data Storage

Kagan stores all data outside your repositories:

- Database: `~/.local/share/kagan/kagan.db` (XDG-compliant)
- Config: `~/.config/kagan/config.toml`
- Worktrees: system temp directory (e.g. `/var/tmp/kagan/worktrees/`)

No `.kagan/` directory is created inside your repos.

## Alpha Note (No Migration)

This is an alpha feature set. Legacy single-repo data is not migrated automatically. If you
used older versions, start with a fresh database in the XDG location.
