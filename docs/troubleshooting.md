---
title: Troubleshooting
description: Symptom-first fixes for common Kagan issues
icon: material/bug
---

# Troubleshooting

Use exact symptom text to find fixes quickly.

## `Core process is running, but runtime metadata is incomplete.`

Cause:

- Core runtime files are stale or partially missing.

Fix:

```bash
kagan core stop
kagan core start
kagan core status
```

## `AUTH_STALE_TOKEN`

Cause:

- MCP client token became stale after core restart.

Fix:

1. Restart/reconnect the MCP client process.
1. If needed, restart core lifecycle:

```bash
kagan core stop
kagan core start
```

## `DISCONNECTED`

Cause:

- MCP client cannot reach the core endpoint.

Fix:

1. Launch Kagan once in the project (`kagan`) or start core manually.
1. Re-run MCP:

```bash
kagan mcp
```

## `START_PENDING`

Cause:

- `jobs_submit` accepted the request but scheduler admission is still pending.

Fix:

- Poll with `jobs_wait` or `jobs_get` until the state becomes running/terminal.

## `PAIR terminal backend is set to tmux, but tmux was not found in PATH.`

Cause:

- PAIR backend is `tmux`, but `tmux` is not installed or not in `PATH`.

Fix:

```bash
brew install tmux            # macOS
sudo apt install tmux        # Debian/Ubuntu
sudo dnf install tmux        # Fedora/RHEL
```

Or set a different backend:

```toml
[general]
default_pair_terminal_backend = "vscode"
```

## `Unsupported external PAIR launcher: <backend>`

Cause:

- PAIR backend value is invalid.

Fix:

Set one of the supported values:

```toml
[general]
default_pair_terminal_backend = "tmux"   # or "vscode" or "cursor"
```

## `Another Kagan instance is already running in this repository.`

Cause:

- A second Kagan instance holds the repository lock.

Fix:

1. Close the other instance.
1. Start Kagan again in this repo.
1. If lock state is stale, run reset as a last resort:

```bash
kagan reset
```

## `Git is required but was not found on your system.`

Cause:

- Git is missing.

Fix:

Install Git, then relaunch Kagan.

- macOS: `brew install git`
- Debian/Ubuntu: `sudo apt install git`
- Fedora/RHEL: `sudo dnf install git`

## `Git user identity is not configured.`

Cause:

- Global Git identity is missing.

Fix:

```bash
git config --global user.name "Your Name"
git config --global user.email "you@example.com"
```

## UI rendering issues in terminal

Cause:

- Terminal is too small or does not support expected features.

Fix:

1. Resize terminal to at least `80x20`.
1. Use a modern terminal with truecolor support.
1. Open debug log from TUI with `F12` for extra diagnostics.

## Nuclear cleanup

Use only when standard recovery fails.

```bash
kagan reset --force
```

This permanently removes local Kagan state.
