---
title: Upgrades and reset
description: Update Kagan safely and reset local state when needed
icon: material/update
---

# Upgrades and reset

Use this guide for version upgrades and local state recovery.

## Check for updates

```bash
kagan update --check
```

## Install updates

```bash
kagan update
```

Options:

- `kagan update --force`: skip confirmation prompt
- `kagan update --prerelease`: include pre-release versions

## Skip startup update checks

Set environment variable:

```bash
export KAGAN_SKIP_UPDATE_CHECK=1
```

## Reset local state

`kagan reset` is destructive. It removes local config/data/cache/worktree state.

```bash
kagan reset
```

Force full reset without prompts:

```bash
kagan reset --force
```

## Before running reset

1. Confirm no active work you need in Kagan-managed worktrees.
1. Export or copy any logs or local state you need.
1. Stop other Kagan sessions first.

## After reset

1. Run `kagan` again.
1. Recreate or reopen your project.
1. Reconfigure MCP clients if needed.
