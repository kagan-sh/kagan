---
title: Editor MCP setup
description: Configure MCP client files for common editors and coding agents
icon: material/monitor-dashboard
---

# Editor MCP setup

This guide provides ready-to-use client config snippets.

## Recommended starting profile

Use read-only access first:

```text
command: kagan
args: ["mcp", "--readonly", "--capability", "viewer"]
```

Switch to elevated profiles only when needed.

## Claude Code

Path: `~/.claude.json` (global) or `.mcp.json` (project)

```json
{
  "mcpServers": {
    "kagan": {
      "command": "kagan",
      "args": ["mcp", "--capability", "pair_worker"]
    }
  }
}
```

## VS Code (GitHub Copilot)

Path: `.vscode/mcp.json`

```json
{
  "servers": {
    "kagan": {
      "command": "kagan",
      "args": ["mcp", "--capability", "pair_worker"]
    }
  }
}
```

## Cursor

Path: `.cursor/mcp.json`

```json
{
  "mcpServers": {
    "kagan": {
      "command": "kagan",
      "args": ["mcp", "--capability", "pair_worker"]
    }
  }
}
```

## OpenCode

Path: `~/.config/opencode/opencode.json`

```json
{
  "mcpServers": {
    "kagan": {
      "command": "kagan",
      "args": ["mcp", "--capability", "pair_worker"]
    }
  }
}
```

## Codex

Path: `~/.codex/config.toml`

```toml
[mcp_servers.kagan]
command = "kagan"
args = ["mcp", "--capability", "pair_worker"]
```

## Gemini CLI

Path: `~/.gemini/settings.json`

```json
{
  "mcpServers": {
    "kagan": {
      "command": "kagan",
      "args": ["mcp", "--capability", "pair_worker"]
    }
  }
}
```

## Kimi CLI

Path: `~/.kimi/mcp.json`

```json
{
  "mcpServers": {
    "kagan": {
      "command": "kagan",
      "args": ["mcp", "--capability", "pair_worker"]
    }
  }
}
```

## Optional admin lane

Use only for trusted automation:

```text
kagan mcp --identity kagan_admin --capability maintainer
```

## Verify

1. Start Kagan in your project.
1. Start the client MCP server.
1. Run `tasks_list` from the client.
1. Confirm tasks are returned.

If verification fails, use [Troubleshooting](../troubleshooting.md).
