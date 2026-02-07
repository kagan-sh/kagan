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
auto_start = false
auto_approve = false
auto_merge = false
auto_retry_on_merge_conflict = true
require_review_approval = false
serialize_merges = false
default_base_branch = "main"
default_worker_agent = "claude"
max_concurrent_agents = 1
max_iterations = 10
iteration_delay_seconds = 2.0
```

| Setting                   | Default    | Purpose                                            |
| ------------------------- | ---------- | -------------------------------------------------- |
| `auto_start`              | `false`    | Auto-run agents for IN_PROGRESS tickets on startup |
| `auto_approve`            | `false`    | Skip permission prompts for AI actions             |
| `auto_merge`              | `false`    | Auto-merge tickets after review passes             |
| `default_base_branch`     | `"main"`   | Base branch for worktrees and merges               |
| `default_worker_agent`    | `"claude"` | Default agent for new tickets                      |
| `max_concurrent_agents`   | `3`        | Maximum parallel AUTO agents                       |
| `max_iterations`          | `10`       | Max agent iterations before BACKLOG                |
| `iteration_delay_seconds` | `2.0`      | Delay between agent iterations                     |
| `default_model_claude`    | `None`     | Default Claude model alias or full name            |
| `default_model_opencode`  | `None`     | Default OpenCode model                             |

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

| Field                 | Purpose                                      |
| --------------------- | -------------------------------------------- |
| `identity`            | Unique agent identifier                      |
| `name`                | Display name in UI                           |
| `short_name`          | Compact label for badges                     |
| `protocol`            | Protocol type (currently only `acp`)         |
| `active`              | Whether this agent is available              |
| `run_command`         | OS-specific command for AUTO mode (ACP)      |
| `interactive_command` | OS-specific command for PAIR mode (CLI)      |
| `model_env_var`       | Environment variable for model selection     |

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
skip_tmux_gateway = false
```

| Setting             | Default | Purpose                                             |
| ------------------- | ------- | --------------------------------------------------- |
| `skip_tmux_gateway` | `false` | Skip the tmux info modal when opening PAIR sessions |

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
| `<continue/>`              | Continue iteration   |
| `<approve summary="..."/>` | Approve in review    |
| `<reject reason="..."/>`   | Reject in review     |

## Environment Variables

These variables are set when agents run:

| Variable              | Description            |
| --------------------- | ---------------------- |
| `KAGAN_TASK_ID`       | Current task ID        |
| `KAGAN_TASK_TITLE`    | Current task title     |
| `KAGAN_WORKTREE_PATH` | Path to task worktree  |
| `KAGAN_PROJECT_ROOT`  | Root of the repository |

## MCP Configuration Files

For MCP server integration, agents look for:

| Agent       | Config File     |
| ----------- | --------------- |
| Claude Code | `.mcp.json`     |
| OpenCode    | `opencode.json` |

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
