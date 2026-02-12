---
title: Home
description: AI-powered Kanban TUI for autonomous development workflows
icon: material/home
hide:

  - navigation
---

# Kagan Docs

Build faster with a keyboard-first AI Kanban workflow.

## TL;DR

```bash
uv tool install kagan
cd your-project
kagan
```

Press ++question++ anytime in the app for context-aware shortcuts.

## I Want To...

| Goal                                | Open This                                            | Time      |
| ----------------------------------- | ---------------------------------------------------- | --------- |
| Install and run Kagan fast          | [5-Minute Quickstart](getting-started/quickstart.md) | 5 min     |
| Choose the right working mode       | [AUTO vs PAIR](how-to/task-modes.md)                 | 2 min     |
| Connect external agents over MCP    | [MCP Setup](how-to/mcp-setup.md)                     | 5 min     |
| Work across multiple repositories   | [Multi-Repo Guide](how-to/multi-repo.md)             | 4 min     |
| Fix common issues                   | [Troubleshooting](troubleshooting.md)                | 3 min     |
| Understand contributor architecture | [Architecture](reference/architecture.md)            | reference |
| See all shortcuts                   | [Keyboard Shortcuts](reference/keybindings.md)       | reference |
| Tune behavior and defaults          | [Configuration](reference/configuration.md)          | reference |
| Browse CLI commands quickly         | [Command Reference](getting-started/commands.md)     | reference |

## Fast paths

**From zero:** [Quickstart](getting-started/quickstart.md) -> create task (`n`) -> pick mode ([AUTO vs PAIR](how-to/task-modes.md))

**Already using Kagan:** [Commands](getting-started/commands.md) -> [MCP Setup](how-to/mcp-setup.md) -> [Configuration](reference/configuration.md)

## Product model

Kagan has one shared core process and two interfaces:

- **TUI**: keyboard-first day-to-day workflow
- **MCP server**: external AI agents read/update the same task state

Task status, review state, and project context stay consistent regardless of where actions originate. Contributor architecture: [Architecture](reference/architecture.md).
