---
title: Command Reference
description: CLI commands for launching, managing, and debugging Kagan
icon: material/console
---

# Command Reference

## TL;DR

```bash
kagan              # Launch TUI
kagan mcp          # Start MCP bridge
kagan core status  # Check core health
```

## App entrypoints

| Command        | What it does                           |
| -------------- | -------------------------------------- |
| `kagan`        | Launch TUI (auto-starts/attaches core) |
| `kagan tui`    | Explicit TUI launch                    |
| `kagan mcp`    | Start MCP server bridge                |
| `kagan list`   | List projects                          |
| `kagan update` | Update Kagan                           |
| `kagan reset`  | Interactive reset                      |

## Core lifecycle

| Command             | What it does                      |
| ------------------- | --------------------------------- |
| `kagan core status` | Show transport, endpoint, and PID |
| `kagan core stop`   | Stop running core process         |

The core daemon is a singleton shared by TUI and MCP. It autostarts when `general.core_autostart = true` and auto-stops after `general.core_idle_timeout_seconds` with no connected clients.

## MCP launch options

| Command                                | Use case             |
| -------------------------------------- | -------------------- |
| `kagan mcp --readonly`                 | Read-only assistant  |
| `kagan mcp --capability pair_worker`   | Worker-limited lane  |
| `kagan mcp --identity kagan_admin`     | Admin lane           |
| `kagan mcp --session-id task:TASK-001` | Bind to task session |

## Prompt tools

| Command                                 | What it does             |
| --------------------------------------- | ------------------------ |
| `kagan tools enhance "..."`             | Refine prompt text       |
| `kagan tools enhance -f prompt.md`      | Refine prompt from file  |
| `kagan tools enhance "..." -t opencode` | Use specific target tool |

## See also

- [5-Minute Quickstart](quickstart.md)
- [MCP Setup](../how-to/mcp-setup.md)
- [MCP Tools Reference](../reference/mcp-tools.md)
- [Troubleshooting](../troubleshooting.md)
