---
title: Admin MCP for Editors
description: Configure your editor to control Kagan via the Admin MCP server
icon: material/monitor-dashboard
---

# Admin MCP for Editors

Control Kagan directly from your editor or AI coding assistant by adding the
Admin MCP server to your client configuration. This gives your editor full
access to create tasks, launch agents, review PRs, and manage projects --
without switching to the TUI.

## When to use Admin MCP

| Scenario                                                    | Why Admin MCP                                                                           |
| ----------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| You want your AI assistant to create and manage Kagan tasks | The assistant calls `tasks_create`, `tasks_move`, etc. on your behalf                   |
| You prefer staying in your editor for reviews               | `review(action="approve")` or `review(action="merge")` from the editor                  |
| You want to orchestrate multiple agents from one place      | `jobs_submit` and `jobs_wait` let an editor-based agent launch and monitor AUTO workers |

## Editor configurations

Each snippet below registers Kagan as an MCP server in admin mode with full
`maintainer` capability. Adjust `--capability` to a narrower profile
(`viewer`, `pair_worker`, `operator`) if you want to limit what the AI
assistant can do. See [Capability profiles](../reference/mcp-tools.md#capability-profiles)
for the full hierarchy.

### Claude Code

Add to `~/.claude.json` (global) or `.mcp.json` (per-project):

```json
{
  "mcpServers": {
    "kagan_admin": {
      "command": "kagan",
      "args": [
        "mcp",
        "--identity", "kagan_admin",
        "--capability", "maintainer"
      ]
    }
  }
}
```

### VS Code (GitHub Copilot)

Add to `.vscode/mcp.json` in your workspace:

```json
{
  "servers": {
    "kagan_admin": {
      "command": "kagan",
      "args": [
        "mcp",
        "--identity", "kagan_admin",
        "--capability", "maintainer"
      ]
    }
  }
}
```

### Cursor

Add to `.cursor/mcp.json` in your project root:

```json
{
  "mcpServers": {
    "kagan_admin": {
      "command": "kagan",
      "args": [
        "mcp",
        "--identity", "kagan_admin",
        "--capability", "maintainer"
      ]
    }
  }
}
```

### OpenCode

Add to `~/.config/opencode/opencode.json`:

```json
{
  "mcpServers": {
    "kagan_admin": {
      "command": "kagan",
      "args": [
        "mcp",
        "--identity", "kagan_admin",
        "--capability", "maintainer"
      ]
    }
  }
}
```

### Codex

Add to `~/.codex/config.toml`:

```toml
[mcp_servers.kagan_admin]
command = "kagan"
args = ["mcp", "--identity", "kagan_admin", "--capability", "maintainer"]
```

### Gemini CLI

Add to `~/.gemini/settings.json`:

```json
{
  "mcpServers": {
    "kagan_admin": {
      "command": "kagan",
      "args": [
        "mcp",
        "--identity", "kagan_admin",
        "--capability", "maintainer"
      ]
    }
  }
}
```

### Kimi CLI

Add to `~/.kimi/mcp.json`:

```json
{
  "mcpServers": {
    "kagan_admin": {
      "command": "kagan",
      "args": [
        "mcp",
        "--identity", "kagan_admin",
        "--capability", "maintainer"
      ]
    }
  }
}
```

## Read-only variant

If you only want your assistant to read tasks and context without mutating
anything, swap the flags:

```bash
kagan mcp --readonly --capability viewer
```

Replace the `args` array in any snippet above with
`["mcp", "--readonly", "--capability", "viewer"]`.

## Verify the connection

1. Start Kagan: `kagan` (the core daemon must be running)
1. In your editor's AI assistant, ask it to call `tasks_list`
1. Confirm it returns tasks from your current project

If the assistant cannot connect, check [Troubleshooting](../troubleshooting.md)
for common MCP issues.

## Next

- Full tool catalog: [MCP Tools Reference](../reference/mcp-tools.md)
- General MCP setup and launch profiles: [MCP Setup](mcp-setup.md)
- Configuration options: [Configuration](../reference/configuration.md)
