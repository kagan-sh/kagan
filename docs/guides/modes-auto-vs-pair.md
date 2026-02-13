---
title: AUTO vs PAIR
description: Choose and run the right execution mode for each task
icon: material/robot
---

# AUTO vs PAIR

Use this guide to choose the right task mode and execute it correctly.

## Decide mode

| If this is true                           | Use    |
| ----------------------------------------- | ------ |
| Requirements are clear and bounded        | `AUTO` |
| Requirements are evolving or exploratory  | `PAIR` |
| You want asynchronous background progress | `AUTO` |
| You want direct interactive collaboration | `PAIR` |

## Run an AUTO task

1. Create a task (`n`).
1. Set task type to `AUTO`.
1. Start execution with `a` or `Enter`.
1. Monitor progress in Task Output (`Enter`).
1. Review in `REVIEW`, then approve/reject and merge.

## Run a PAIR task

1. Create a task (`n`).
1. Set task type to `PAIR`.
1. Open the session with `Enter`.
1. Work interactively in tmux / VS Code / Cursor.
1. Move through `IN_PROGRESS` and `REVIEW` manually.

## Switch an existing task from one mode to the other

1. Open task details (`v`).
1. Edit the task (`e`).
1. Change `task_type` to `AUTO` or `PAIR`.
1. Save (`F2` or `Alt+S`).

## Verify mode-related settings

```toml
[general]
default_worker_agent = "claude"
default_pair_terminal_backend = "tmux"
max_concurrent_agents = 3
```

Full options: [Configuration reference](../reference/configuration.md).

## If mode execution fails

- PAIR session backend issues: [Troubleshooting](../troubleshooting.md)
- MCP automation and task type mismatches: [MCP tools reference](../reference/mcp-tools.md)
