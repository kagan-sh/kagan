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

- `job_start` was accepted but scheduler admission is still pending.

Fix:

- Poll with `job_poll(wait=false)` until the state becomes running/terminal.

## `logs_truncated=true` or `logs_has_more=true` in `task_get`

Cause:

- `task_get` returns a bounded, recency-first log view for transport reliability.

Fix:

- Fetch additional history with `task_logs(task_id, offset, limit)`.
- Use `next_offset` until `has_more` is `false`.

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

## GitHub Plugin Issues

### `GH_CLI_NOT_AVAILABLE`

Cause:

- GitHub CLI (`gh`) is not installed or not in PATH.

Fix:

```bash
brew install gh            # macOS
sudo apt install gh        # Debian/Ubuntu
sudo dnf install gh        # Fedora/RHEL
```

### `GH_AUTH_REQUIRED`

Cause:

- Not authenticated with GitHub CLI.

Fix:

```bash
gh auth login
gh auth status
```

### `GH_NOT_CONNECTED`

Cause:

- Repository is not connected to GitHub in Kagan.

Fix:

1. Run `kagan_github_connect_repo` via MCP.
1. Or use TUI action palette: `.` → "Connect GitHub".

### `LEASE_HELD_BY_OTHER`

Cause:

- Another Kagan instance is working on this GitHub issue.

Fix:

1. Check holder info in error response.
1. If holder instance is gone, use `force_takeover: true`.
1. If holder info is unavailable, retry once, then use `force_takeover: true`.
1. If lease is over 2 hours old, takeover is automatic.

### Sync shows zero issues but GitHub has issues

Cause:

- gh CLI may not have access to the repository.

Fix:

1. Verify: `gh issue list --repo owner/repo`
1. Check GitHub App permissions if using automation.
1. Re-authenticate: `gh auth login`

## Nuclear cleanup

Use only when standard recovery fails.

```bash
kagan reset --force
```

This permanently removes local Kagan state.
