---
title: Multi-Repo Guide
description: Run one project across multiple repositories
icon: material/source-repository-multiple
---

# Multi-Repo Guide

Run one project across multiple repositories with shared task state.

## TL;DR

1. Create/open project with multiple repo paths
1. Switch active repo with `Ctrl+R`
1. Set base branches (`b` task-level, `Shift+B` global)
1. Review diffs/merge per repo in Task Details (`v`)

## Create a multi-repo project

1. Launch Kagan
1. From Welcome, press `n` for **New Project**
1. Add repository paths (first repo becomes primary)

## Switch active repo

Press `Ctrl+R` to open repo picker.

- Navigate: `Up`/`Down` or `j`/`k`
- Select: `Enter`
- Add repo: `n`
- Cancel: `Esc`

## Base branch controls

- `b`: set base branch for current task
- `Shift+B`: set default base branch for repo

These drive diff and merge targets.

## Diff and merge per repo

1. Open Task Details: `v`
1. Find **Workspace Repos**
1. Use **Diff** and **Merge** actions for each repo

## Storage model

Kagan stores state outside your code repositories:

- Database: `~/.local/share/kagan/kagan.db`
- Config: `~/.config/kagan/config.toml`
- Worktrees: system temp dir (example `/var/tmp/kagan/worktrees/`)

No `.kagan/` folder is created inside your repos.
