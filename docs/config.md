# Configuration

Kagan configuration lives in the XDG config directory, created automatically on first run
(for example `~/.config/kagan/config.toml` on Linux).

## File Locations

| Path                         | Purpose         |
| ---------------------------- | --------------- |
| XDG config dir `config.toml` | Configuration   |
| XDG data dir `kagan.db`      | Task database   |
| XDG data dir `kagan.lock`    | Single-instance |
| Temp dir `kagan/worktrees/`  | Git worktrees   |

## General Settings

```toml
[general]
auto_review = true
auto_approve = false
require_review_approval = false
serialize_merges = false
default_base_branch = "main"
default_worker_agent = "claude"
default_pair_terminal_backend = "tmux"
max_concurrent_agents = 1
mcp_server_name = "kagan"
# default_model_claude = "claude-3-5-sonnet"  # Optional
# default_model_opencode = "opencode-default"  # Optional
```

| Setting                         | Default    | Purpose                                                                    |
| ------------------------------- | ---------- | -------------------------------------------------------------------------- |
| `auto_review`                   | `true`     | Run AI review on task completion                                           |
| `auto_approve`                  | `false`    | Skip permission prompts in the planner agent (workers always auto-approve) |
| `require_review_approval`       | `false`    | Require approved review before merge actions                               |
| `serialize_merges`              | `false`    | Serialize manual merges to reduce conflicts                                |
| `default_base_branch`           | `"main"`   | Base branch for worktrees and merges                                       |
| `default_worker_agent`          | `"claude"` | Default agent for new tickets                                              |
| `default_pair_terminal_backend` | `"tmux"`   | Default terminal backend for PAIR tasks (`tmux`, `vscode`, or `cursor`)    |
| `max_concurrent_agents`         | `1`        | Maximum parallel AUTO agents                                               |
| `mcp_server_name`               | `"kagan"`  | MCP server name used in tool registration/config                           |
| `default_model_claude`          | `None`     | Default Claude model alias or full name (optional)                         |
| `default_model_opencode`        | `None`     | Default OpenCode model (optional)                                          |

!!! note "Permission model"
**Worker agents** (AUTO tasks) always auto-approve tool calls because they run in
isolated git worktrees with path-confined file access. The `auto_approve` setting
only controls the **planner agent**, which is interactive and operates on the main
repository. **Reviewer** and **refiner** agents also always auto-approve.

## Agent Configuration

```toml
[agents.claude]
identity = "claude.com"
name = "Claude Code"
short_name = "claude"
protocol = "acp"
active = true

[agents.claude.run_command]
"*" = "npx claude-code-acp"

[agents.claude.interactive_command]
"*" = "claude"
```

| Field                 | Purpose                                  |
| --------------------- | ---------------------------------------- |
| `identity`            | Unique agent identifier                  |
| `name`                | Display name in UI                       |
| `short_name`          | Compact label for badges                 |
| `protocol`            | Protocol type (currently only `acp`)     |
| `active`              | Whether this agent is available          |
| `run_command`         | OS-specific command for AUTO mode (ACP)  |
| `interactive_command` | OS-specific command for PAIR mode (CLI)  |
| `model_env_var`       | Environment variable for model selection |

### OS-Specific Commands

Commands can be specified per-OS or with a wildcard:

```toml
[agents.myagent.run_command]
macos = "my-agent-mac"
linux = "my-agent-linux"
"*" = "my-agent"  # Fallback for any OS
```

### Multiple Agents

```toml
[agents.opencode]
identity = "opencode.ai"
name = "OpenCode"
short_name = "opencode"
active = true

[agents.opencode.run_command]
"*" = "opencode acp"

[agents.opencode.interactive_command]
"*" = "opencode"
```

## UI Settings

```toml
[ui]
skip_pair_instructions = false
```

| Setting                  | Default | Purpose                                                 |
| ------------------------ | ------- | ------------------------------------------------------- |
| `skip_pair_instructions` | `false` | Skip the PAIR instructions popup before session launch. |

## Refinement Settings

```toml
[refinement]
enabled = true
hotkey = "f2"
skip_length_under = 20
skip_prefixes = ["/", "!", "?"]
```

| Setting             | Default           | Purpose                                      |
| ------------------- | ----------------- | -------------------------------------------- |
| `enabled`           | `true`            | Enable prompt refinement feature             |
| `hotkey`            | `"f2"`            | Hotkey to trigger refinement in planner      |
| `skip_length_under` | `20`              | Skip refinement for inputs shorter than this |
| `skip_prefixes`     | `["/", "!", "?"]` | Prefixes that skip refinement                |

## AUTO Mode Signals

Agents communicate state transitions via XML signals:

| Signal                     | Effect               |
| -------------------------- | -------------------- |
| `<complete/>`              | Move task to REVIEW  |
| `<blocked reason="..."/>`  | Move task to BACKLOG |
| `<continue/>`              | Continue execution   |
| `<approve summary="..."/>` | Approve in review    |
| `<reject reason="..."/>`   | Reject in review     |

## Environment Variables

These variables are set when agents run:

| Variable                | Description              |
| ----------------------- | ------------------------ |
| `KAGAN_TASK_ID`         | Current task ID          |
| `KAGAN_TASK_TITLE`      | Current task title       |
| `KAGAN_WORKTREE_PATH`   | Path to task worktree    |
| `KAGAN_PROJECT_ROOT`    | Root of the repository   |
| `KAGAN_MCP_SERVER_NAME` | Override MCP server name |

## MCP Configuration Files

For MCP server integration, agents look for:

| Agent       | Config File        |
| ----------- | ------------------ |
| Claude Code | `.mcp.json`        |
| OpenCode    | `opencode.json`    |
| VS Code     | `.vscode/mcp.json` |
| Cursor      | `.cursor/mcp.json` |

The MCP server entry name defaults to `kagan` and can be changed via
`general.mcp_server_name` or `KAGAN_MCP_SERVER_NAME`.

## Minimal Config

A minimal working configuration:

```toml
[general]
default_base_branch = "main"
default_worker_agent = "claude"

[agents.claude]
identity = "claude.com"
name = "Claude Code"
short_name = "claude"
active = true

[agents.claude.run_command]
"*" = "npx claude-code-acp"

[agents.claude.interactive_command]
"*" = "claude"
```
