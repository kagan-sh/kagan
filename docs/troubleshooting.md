---
title: Troubleshooting
description: Fast fixes for common issues
icon: material/bug
---

# Troubleshooting

## TL;DR

1. Check terminal size (`>= 80x20`)
1. Check agent binaries in `PATH`
1. Open debug log with `F12`
1. Use `kagan reset` for stale local state

## Quick fix table

| Symptom                                      | Fast fix                                         |
| -------------------------------------------- | ------------------------------------------------ |
| Agent not detected                           | Verify CLI binary in `PATH`, restart terminal    |
| PAIR session won't open                      | Install/select backend (`tmux`, VS Code, Cursor) |
| Instance lock error                          | Close duplicate instance or run `kagan reset`    |
| Merge conflict in REVIEW                     | Resolve in merge worktree, retry merge           |
| MCP says `AUTH_STALE_TOKEN`                  | Restart MCP client or run `kagan core restart`   |
| UI looks broken                              | Resize terminal to at least `80x20`              |
| `kagan core status` says metadata incomplete | Run `kagan core stop` then `kagan core start`    |

## Windows

### Native extension install errors (`vcruntime`, `cl.exe`)

Install [Microsoft Visual C++ Redistributable](https://go.microsoft.com/fwlink/?LinkID=135170).

### Recommended install path

```powershell
iwr -useb uvget.me/install.ps1 -OutFile install.ps1; .\install.ps1 kagan
```

### PAIR backend

Windows defaults to VS Code. Override:

```toml
[general]
default_pair_terminal_backend = "vscode"  # or "cursor"
```

## macOS / Linux

### `tmux` not found

```bash
brew install tmux            # macOS
sudo apt install tmux        # Debian / Ubuntu
sudo dnf install tmux        # Fedora / RHEL
```

Or switch backend:

```toml
[general]
default_pair_terminal_backend = "vscode"
```

## General

- **Agent not detected**: verify binary (`which claude`, etc.), restart terminal, check `F12` debug log.
- **Instance lock error**: close other Kagan instances for same repo, or `kagan reset`.
- **Merge conflicts**: open resolve from Task Details, resolve in worktree, retry. Consider `serialize_merges = true`.

### Reset local state

```bash
kagan reset         # interactive
kagan reset --force # delete all local state
```

`kagan reset` stops a running core daemon before deleting state.

Data paths: `~/.local/share/kagan/kagan.db`, `~/.config/kagan/config.toml`, system temp dir (`/var/tmp/kagan/worktrees/`).
