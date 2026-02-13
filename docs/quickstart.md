---
title: Quickstart
description: Install Kagan and complete your first task in under 5 minutes
icon: material/timer
---

# Quickstart

This guide gets you from install to first completed task.

## Prerequisites

- `uv` installed
- `git` installed
- A local Git repository

## 1. Install Kagan

```bash
uv tool install kagan
kagan --version
```

Expected output:

```text
kagan 0.5.0
```

Your version may be newer.

## 2. Launch Kagan inside your repository

```bash
cd /path/to/your-repo
kagan
```

Expected result:

- You see the Kagan welcome screen.
- You can open or create a project.

## 3. Open or create a project

1. If this is your first run, select `New Project`.
1. Add the current repository path.
1. Continue to the board.

Expected result:

- The board view opens with columns (`BACKLOG`, `IN_PROGRESS`, `REVIEW`, `DONE`).

## 4. Create your first task

1. Press `n`.
1. Enter title and description.
1. Select a task type: `AUTO` or `PAIR`.
1. Save with `F2` (or `Alt+S`).

Expected result:

- The task appears in `BACKLOG`.

## 5. Run the task

For `AUTO`:

1. Select the task.
1. Press `a` (or `Enter`).
1. Open Task Output with `Enter`.

For `PAIR`:

1. Select the task.
1. Press `Enter`.
1. Work in the interactive session backend.

Need help deciding? Use [AUTO vs PAIR](guides/modes-auto-vs-pair.md).

## 6. Review and finish

1. Move the task to `REVIEW`.
1. Open Task Output (`Enter`) and inspect Summary/Diff/Review output.
1. Approve or reject.
1. Merge when ready.

## Useful keys

- Help: `?` or `F1`
- Action palette: `.` or `Ctrl+P`
- Settings: `,`
- Debug log: `F12`

## If something fails

Use [Troubleshooting](troubleshooting.md).
