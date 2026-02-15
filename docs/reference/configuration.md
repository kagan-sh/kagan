---
title: Configuration reference
description: Full schema for config.toml, locations, and environment overrides
icon: material/cog
---

# Configuration reference

Kagan reads configuration from `config.toml` in the Kagan config directory.

## Location and path overrides

Default directories are platform-specific (`platformdirs`), with environment-variable overrides.

| Purpose                 | Default location            | Override                 |
| ----------------------- | --------------------------- | ------------------------ |
| Config directory        | `user_config_dir("kagan")`  | `KAGAN_CONFIG_DIR`       |
| Data directory          | `user_data_dir("kagan")`    | `KAGAN_DATA_DIR`         |
| Cache directory         | `user_cache_dir("kagan")`   | `KAGAN_CACHE_DIR`        |
| Worktree base directory | system temp base + `/kagan` | `KAGAN_WORKTREE_BASE`    |
| Core runtime directory  | data dir + `/core`          | `KAGAN_CORE_RUNTIME_DIR` |

Main files:

- `config.toml`
- `profiles.toml`
- `kagan.db`
- core runtime files: `endpoint.json`, `token`, `core.lease.json`, `core.instance.lock`

## Minimal example

```toml
[general]
default_worker_agent = "claude"
default_pair_terminal_backend = "tmux"
max_concurrent_agents = 3
```

## `[general]`

| Key                                  | Type           | Default                          | Notes                                                                          |
| ------------------------------------ | -------------- | -------------------------------- | ------------------------------------------------------------------------------ |
| `max_concurrent_agents`              | integer        | `3`                              | Concurrent AUTO execution cap                                                  |
| `mcp_server_name`                    | string         | `"kagan"`                        | MCP server registration name                                                   |
| `worktree_base_ref_strategy`         | string         | `"remote"`                       | Base ref preference for worktree add/diff: `remote`, `local_if_ahead`, `local` |
| `auto_review`                        | boolean        | `true`                           | Run AI review on completion                                                    |
| `auto_approve`                       | boolean        | `false`                          | Skip planner permission prompts                                                |
| `require_review_approval`            | boolean        | `false`                          | Require review approval before merge                                           |
| `serialize_merges`                   | boolean        | `true`                           | Queue merge actions                                                            |
| `default_worker_agent`               | string         | `"claude"`                       | Default worker agent                                                           |
| `default_pair_terminal_backend`      | string         | `"tmux"` (`"vscode"` on Windows) | Allowed: `tmux`, `vscode`, `cursor`                                            |
| `default_model_claude`               | string or null | `null`                           | Optional default model                                                         |
| `default_model_opencode`             | string or null | `null`                           | Optional default model                                                         |
| `default_model_codex`                | string or null | `null`                           | Optional default model                                                         |
| `default_model_gemini`               | string or null | `null`                           | Optional default model                                                         |
| `default_model_kimi`                 | string or null | `null`                           | Optional default model                                                         |
| `default_model_copilot`              | string or null | `null`                           | Optional display preference                                                    |
| `core_idle_timeout_seconds`          | integer        | `180`                            | Core auto-stop timeout after idle                                              |
| `core_autostart`                     | boolean        | `true`                           | Start core automatically when client connects                                  |
| `core_transport_preference`          | string         | `"auto"`                         | Allowed: `auto`, `socket`, `tcp`                                               |
| `tasks_wait_default_timeout_seconds` | integer        | `1800`                           | Default timeout for `task_wait` (30 minutes)                                   |
| `tasks_wait_max_timeout_seconds`     | integer        | `3600`                           | Max allowed timeout for `task_wait` (60 minutes)                               |

## `[agents.<name>]`

Example:

```toml
[agents.claude]
identity = "claude.com"
name = "Claude Code"
short_name = "claude"
protocol = "acp"
active = true
model_env_var = "ANTHROPIC_MODEL"

[agents.claude.run_command]
"*" = "npx claude-code-acp"

[agents.claude.interactive_command]
"*" = "claude"
```

Fields:

| Key                   | Type    | Notes                            |
| --------------------- | ------- | -------------------------------- |
| `identity`            | string  | Unique provider/agent identity   |
| `name`                | string  | Display name                     |
| `short_name`          | string  | Compact label                    |
| `protocol`            | string  | Currently `acp`                  |
| `active`              | boolean | Enable/disable this agent        |
| `model_env_var`       | string  | Env var used for model selection |
| `run_command`         | table   | OS-keyed commands for AUTO mode  |
| `interactive_command` | table   | OS-keyed commands for PAIR mode  |

OS keys for command tables: `macos`, `linux`, `windows`, `*`.

## `[refinement]`

| Key                 | Type        | Default           | Notes                        |
| ------------------- | ----------- | ----------------- | ---------------------------- |
| `enabled`           | boolean     | `true`            | Enable prompt refinement     |
| `hotkey`            | string      | `"f2"`            | Hotkey to trigger refinement |
| `skip_length_under` | integer     | `20`              | Skip short inputs            |
| `skip_prefixes`     | string list | `['/', '!', '?']` | Skip command-like inputs     |

## `[ui]`

| Key                        | Type        | Default                | Notes                                                        |
| -------------------------- | ----------- | ---------------------- | ------------------------------------------------------------ |
| `skip_pair_instructions`   | boolean     | `false`                | Skip PAIR instruction modal                                  |
| `tui_plugin_ui_allowlist`  | string list | `["official.github"]`  | Plugin IDs allowed to contribute declarative UI to the TUI   |

## Environment variables passed into PAIR sessions

| Variable                | Meaning                   |
| ----------------------- | ------------------------- |
| `KAGAN_TASK_ID`         | Current task ID           |
| `KAGAN_TASK_TITLE`      | Current task title        |
| `KAGAN_WORKTREE_PATH`   | Worktree path             |
| `KAGAN_PROJECT_ROOT`    | Project root path         |
| `KAGAN_CWD`             | Current working directory |
| `KAGAN_MCP_SERVER_NAME` | MCP server name override  |
