---
title: CLI reference
description: Complete command and option reference for the kagan CLI
icon: material/console
---

# CLI reference

Last verified: 2026-02-12 against `kagan 0.5.0`.

## Root command

Usage:

```text
kagan [OPTIONS] COMMAND [ARGS]...
```

Options:

| Option      | Type | Description           |
| ----------- | ---- | --------------------- |
| `--version` | flag | Show version and exit |
| `--help`    | flag | Show help message     |

Behavior:

- Running `kagan` with no subcommand starts `kagan tui`.

Commands:

| Command  | Description                        |
| -------- | ---------------------------------- |
| `core`   | Manage the core process            |
| `list`   | List projects and associated repos |
| `mcp`    | Run MCP server (stdio)             |
| `reset`  | Remove local Kagan state           |
| `tools`  | Stateless utilities                |
| `tui`    | Run TUI explicitly                 |
| `update` | Check/install updates from PyPI    |

## `kagan tui`

Usage:

```text
kagan tui [OPTIONS]
```

Options:

| Option                | Type  | Description                               |
| --------------------- | ----- | ----------------------------------------- |
| `--db TEXT`           | value | Path to SQLite database                   |
| `--skip-preflight`    | flag  | Skip pre-flight checks (development only) |
| `--skip-update-check` | flag  | Skip update check on startup              |
| `--help`              | flag  | Show help message                         |

## `kagan list`

Usage:

```text
kagan list [OPTIONS]
```

Options:

| Option   | Type | Description       |
| -------- | ---- | ----------------- |
| `--help` | flag | Show help message |

## `kagan core`

Usage:

```text
kagan core [OPTIONS] COMMAND [ARGS]...
```

Subcommands:

| Command  | Description                       |
| -------- | --------------------------------- |
| `start`  | Start core process if not running |
| `status` | Show running core status          |
| `stop`   | Stop running core process         |

### `kagan core start`

Usage:

```text
kagan core start [OPTIONS]
```

Options:

| Option         | Type | Description                      |
| -------------- | ---- | -------------------------------- |
| `--foreground` | flag | Run core in foreground and block |
| `--help`       | flag | Show help message                |

### `kagan core status`

Usage:

```text
kagan core status [OPTIONS]
```

Options:

| Option   | Type | Description       |
| -------- | ---- | ----------------- |
| `--help` | flag | Show help message |

### `kagan core stop`

Usage:

```text
kagan core stop [OPTIONS]
```

Options:

| Option   | Type | Description       |
| -------- | ---- | ----------------- |
| `--help` | flag | Show help message |

## `kagan mcp`

Usage:

```text
kagan mcp [OPTIONS]
```

Options:

| Option                              | Type  | Description                                                                      |
| ----------------------------------- | ----- | -------------------------------------------------------------------------------- |
| `--readonly`                        | flag  | Expose read-only coordination tools                                              |
| `--session-id TEXT`                 | value | Bind server instance to a session/task                                           |
| `--capability TEXT`                 | value | Capability profile: `viewer`, `planner`, `pair_worker`, `operator`, `maintainer` |
| `--endpoint TEXT`                   | value | Override core endpoint discovery                                                 |
| `--identity TEXT`                   | value | Identity lane: `kagan`, `kagan_admin`                                            |
| `--enable-internal-instrumentation` | flag  | Enable diagnostics tool (support/advanced use)                                   |
| `--help`                            | flag  | Show help message                                                                |

## `kagan update`

Usage:

```text
kagan update [OPTIONS]
```

Options:

| Option         | Type | Description                          |
| -------------- | ---- | ------------------------------------ |
| `-f, --force`  | flag | Skip confirmation prompt             |
| `--check`      | flag | Check for updates without installing |
| `--prerelease` | flag | Include pre-release versions         |
| `--help`       | flag | Show help message                    |

## `kagan reset`

Usage:

```text
kagan reset [OPTIONS]
```

Options:

| Option        | Type | Description              |
| ------------- | ---- | ------------------------ |
| `-f, --force` | flag | Skip confirmation prompt |
| `--help`      | flag | Show help message        |

## `kagan tools`

Usage:

```text
kagan tools [OPTIONS] COMMAND [ARGS]...
```

Subcommands:

| Command   | Description                              |
| --------- | ---------------------------------------- |
| `enhance` | Enhance prompts for AI coding assistants |

### `kagan tools enhance`

Usage:

```text
kagan tools enhance [OPTIONS] [PROMPT]
```

Options:

| Option                | Type         | Description           |
| --------------------- | ------------ | --------------------- |
| \`-t, --tool \[claude | opencode\]\` | value                 |
| `-f, --file PATH`     | value        | Read prompt from file |
| `--help`              | flag         | Show help message     |
