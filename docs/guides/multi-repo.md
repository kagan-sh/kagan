---
title: Multi-repo
description: Run one Kagan project across multiple repositories
icon: material/source-repository-multiple
---

# Multi-repo

Use this guide to manage one project that spans multiple repositories.

## 1. Create a project with multiple repos

1. Launch Kagan.
1. Select `New Project`.
1. Add repository paths.
1. Finish project creation.

The first repository becomes the initial active repo.

## 2. Switch active repository

Press `Ctrl+R` to open the repo picker.

- Navigate: `Up` / `Down` or `j` / `k`
- Select: `Enter`
- Add repo: `n`
- Cancel: `Esc`

## 3. Set branch targets

- Press `b` to set task-level base branch.
  Repo base branches are auto-synced from the currently checked out branch.

These values drive diff and merge targets, with task-level branch taking priority.

## 4. Review changes per repo

1. Open Task Details (`v`).
1. Go to `Workspace Repos`.
1. Run `Diff` and `Merge` actions per repository.

## 5. Verify persisted state location

Kagan state is external to your repositories.

- Database: `kagan.db` in Kagan data directory
- Config: `config.toml` in Kagan config directory
- Worktrees: under configured worktree base directory

Kagan does not write a `.kagan/` folder into your repos.
