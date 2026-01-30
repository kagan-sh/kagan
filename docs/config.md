# Configuration

Config file: `.kagan/config.toml` (created on first run).

## Paths

- `.kagan/state.db`
- `.kagan/config.toml`
- `.kagan/kagan.lock`
- `.kagan/worktrees/`

## General

```toml
[general]
auto_start = false
auto_approve = false
auto_merge = false
default_base_branch = "main"
default_worker_agent = "claude"
max_concurrent_agents = 3
max_iterations = 10
iteration_delay_seconds = 2.0
```

| Setting                   | Purpose                          |
| ------------------------- | -------------------------------- |
| `auto_start`              | Run AUTO agents automatically    |
| `auto_approve`            | Auto-approve permission prompts  |
| `auto_merge`              | Auto-merge after review          |
| `default_base_branch`     | Base branch for worktrees/merges |
| `default_worker_agent`    | Default agent for tickets        |
| `max_concurrent_agents`   | Parallel AUTO agents             |
| `max_iterations`          | Max iterations per AUTO ticket   |
| `iteration_delay_seconds` | Delay between iterations         |

## Agents

```toml
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

| Field                 | Purpose                      |
| --------------------- | ---------------------------- |
| `identity`            | Unique agent ID              |
| `name`                | Display name                 |
| `short_name`          | Compact label                |
| `protocol`            | Protocol type (default: acp) |
| `active`              | Enable agent                 |
| `run_command`         | AUTO mode ACP command        |
| `interactive_command` | PAIR mode CLI command        |

### Multiple agents

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

## AUTO signals

| Signal                     | Effect             |
| -------------------------- | ------------------ |
| `<complete/>`              | Move to REVIEW     |
| `<blocked reason="..."/>`  | Move to BACKLOG    |
| `<continue/>`              | Continue iteration |
| `<approve summary="..."/>` | Approve review     |
| `<reject reason="..."/>`   | Reject review      |

## Minimal config

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

## Environment variables

| Variable              | Description   |
| --------------------- | ------------- |
| `KAGAN_TICKET_ID`     | Ticket ID     |
| `KAGAN_TICKET_TITLE`  | Ticket title  |
| `KAGAN_WORKTREE_PATH` | Worktree path |
| `KAGAN_PROJECT_ROOT`  | Repo root     |

## MCP config files

- Claude Code: `.mcp.json`
- OpenCode: `opencode.json`

## Refinement

```toml
[refinement]
enabled = true
hotkey = "ctrl+e"
skip_length_under = 20
skip_prefixes = ["/", "!", "?"]
```

| Setting             | Purpose                                        |
| ------------------- | ---------------------------------------------- |
| `enabled`           | Enable prompt refinement feature               |
| `hotkey`            | Hotkey to trigger refinement                   |
| `skip_length_under` | Skip refinement for inputs shorter than this   |
| `skip_prefixes`     | Prefixes that skip refinement (commands, etc.) |

## UI

```toml
[ui]
skip_tmux_gateway = false
```

| Setting             | Purpose                                            |
| ------------------- | -------------------------------------------------- |
| `skip_tmux_gateway` | Skip tmux gateway info modal when opening sessions |
