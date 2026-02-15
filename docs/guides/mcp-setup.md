---
title: MCP setup
description: Connect external AI clients to Kagan over MCP
icon: material/server-network
---

# MCP setup

Use this guide when an external AI client should operate on Kagan tasks.

## Prerequisites

- Kagan is installed
- You can launch `kagan` in your project
- Your client supports MCP stdio servers

## 1. Start the MCP server

```bash
kagan mcp
```

Common variants:

```bash
kagan mcp --readonly
kagan mcp --capability pair_worker
kagan mcp --session-id task:TASK-123
kagan mcp --identity kagan_admin --capability maintainer
```

## 2. Add Kagan to your MCP client

Use this minimal server definition:

```text
command: kagan
args: ["mcp"]
```

For editor-specific config files and full snippets, use [Editor MCP setup](editor-mcp-setup.md).

## 3. Verify connectivity

1. Keep `kagan mcp` running.
1. From your client, call `task_list`.
1. Confirm tasks from your active project are returned.
1. Call `task_get(task_id, include_logs=true)` for a known task.
1. If logs are truncated or `logs_has_more=true`, call `task_logs(task_id, offset, limit)`.

## 4. Choose a safe capability profile

| Profile       | Use when                                        |
| ------------- | ----------------------------------------------- |
| `viewer`      | Read-only inspection                            |
| `pair_worker` | Task progress automation with constrained scope |
| `operator`    | Day-to-day task operations                      |
| `maintainer`  | Admin/destructive flows in trusted environments |

## 5. Handle common MCP recovery states

- `AUTH_STALE_TOKEN`: reconnect client or restart core lifecycle (`kagan core stop`, then `kagan core start`)
- `DISCONNECTED`: core endpoint is unavailable; start Kagan or restart core
- `START_PENDING`: task admission is pending; poll with `job_poll(wait=false)`

Details: [MCP tools reference](../reference/mcp-tools.md).
