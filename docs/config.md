# Configuration

Kagan reads optional configuration from `.kagan/config.toml`. If the file is missing,
defaults are used.

## Default paths

- State database: `.kagan/state.db`
- Config file: `.kagan/config.toml`
- Lock file: `.kagan/kagan.lock`
- Worktrees: `.kagan/worktrees/`

## General settings

```toml
[general]
max_concurrent_agents = 3
# Branch to use for worktrees and merges
default_base_branch = "main"
# Auto-run the scheduler loop
auto_start = false
# Max iterations per ticket in auto mode
max_iterations = 10
# Delay between iterations (seconds)
iteration_delay_seconds = 2.0
```

## Agents

Agents are ACP-compatible processes that Kagan can start. The first `active = true`
agent is used as the default.

```toml
[agents.claude]
identity = "anthropic.claude"
name = "Claude Code"
short_name = "claude"
protocol = "acp"
active = true

[agents.claude.run_command]
"*" = "claude"
# You can also provide OS-specific values:
# linux = "claude"
# macos = "claude"
# windows = "claude.exe"
```

## Hats (optional role prompts)

Hats let you add a role-specific system prompt that is appended to agent iterations.
Assigning hats to tickets is currently an advanced workflow. At the moment, only
`system_prompt` is consumed by the agent loop.

```toml
[hats.backend]
agent_command = "claude"
args = ["--model", "sonnet"]
system_prompt = "You are a backend engineer."
```

## Minimal config

```toml
[general]
max_concurrent_agents = 3
```
