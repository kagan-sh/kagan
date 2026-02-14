---
title: GitHub Plugin V1
description: Connect Kagan to GitHub for issue sync, PR workflows, and board automation
icon: material/github
---

# GitHub Plugin V1

Connect Kagan to GitHub repositories for bidirectional task management. GitHub issues become Kagan tasks, and PR workflows automate board transitions.

## Prerequisites

- `gh` CLI installed ([cli.github.com](https://cli.github.com/))
- `gh auth login` completed with repo access
- A Kagan project with an associated repository

## Setup

### 1. Verify gh CLI authentication

```bash
gh auth status
```

Expected output shows authenticated user and active scopes.

### 2. Connect a repository

From TUI: `Ctrl+G` (or action palette `.` → "Connect GitHub")

Via MCP:

```json
{
  "tool": "kagan_github_connect_repo",
  "arguments": {
    "project_id": "<project_id>"
  }
}
```

Single-repo projects auto-resolve. Multi-repo projects require `repo_id`.

Preflight checks:

1. gh CLI availability
2. Authentication status
3. Repository access and metadata

### 3. Sync issues to board

From TUI: `Ctrl+S` (or action palette `.` → "Sync GitHub Issues")

Via MCP:

```json
{
  "tool": "kagan_github_sync_issues",
  "arguments": {
    "project_id": "<project_id>"
  }
}
```

Sync is idempotent and safe to re-run.

## Issue-to-Task Mapping

| GitHub Issue State | Kagan Task Status |
| ------------------ | ----------------- |
| `OPEN`             | `BACKLOG`         |
| `CLOSED`           | `DONE`            |

Task titles include issue attribution: `[GH-123] Original Title`

## AUTO/PAIR Mode Labels

Task execution mode is resolved from issue labels:

| Label               | Task Type |
| ------------------- | --------- |
| `kagan:mode:auto`   | AUTO      |
| `kagan:mode:pair`   | PAIR      |

Resolution order:

1. Issue labels (if present)
2. Repo default (if configured)
3. V1 default: **PAIR**

Conflicting labels (both present): PAIR wins deterministically.

### Configure repo default mode

Store in repo scripts via MCP or direct DB:

```json
{
  "kagan.github.default_mode": "AUTO"
}
```

## Lease Coordination

By default, only one Kagan instance can work a GitHub issue at a time.

### How it works

- Label `kagan:locked` indicates an active lease
- Marker comment stores holder metadata (instance ID, expiry)
- Lease duration: 1 hour (renewable)
- Stale threshold: 2 hours (takeover allowed after)

### Lease operations (MCP)

**Acquire lease:**

```json
{
  "tool": "kagan_github_acquire_lease",
  "arguments": {
    "project_id": "<project_id>",
    "issue_number": 123
  }
}
```

**Check lease state:**

```json
{
  "tool": "kagan_github_get_lease_state",
  "arguments": {
    "project_id": "<project_id>",
    "issue_number": 123
  }
}
```

**Force takeover:**

```json
{
  "tool": "kagan_github_acquire_lease",
  "arguments": {
    "project_id": "<project_id>",
    "issue_number": 123,
    "force_takeover": true
  }
}
```

**Release lease:**

```json
{
  "tool": "kagan_github_release_lease",
  "arguments": {
    "project_id": "<project_id>",
    "issue_number": 123
  }
}
```

### Blocked by another instance

If lease acquisition fails:

- Response includes `holder` with instance info and expiry
- Use `force_takeover: true` only when certain the other instance is gone

## PR Workflows

### Create PR for task

```json
{
  "tool": "kagan_github_create_pr_for_task",
  "arguments": {
    "project_id": "<project_id>",
    "task_id": "<task_id>",
    "title": "Optional custom title",
    "draft": false
  }
}
```

Requirements:

- Task must have an active workspace with a branch
- Branch must be pushed to GitHub

### Link existing PR

```json
{
  "tool": "kagan_github_link_pr_to_task",
  "arguments": {
    "project_id": "<project_id>",
    "task_id": "<task_id>",
    "pr_number": 456
  }
}
```

### Reconcile PR status

```json
{
  "tool": "kagan_github_reconcile_pr_status",
  "arguments": {
    "project_id": "<project_id>",
    "task_id": "<task_id>"
  }
}
```

PR state → task status transitions:

| PR State | Task Transition     |
| -------- | ------------------- |
| `MERGED` | → `DONE`            |
| `CLOSED` | → `IN_PROGRESS`     |
| `OPEN`   | No status change    |

Reconcile is idempotent; re-run produces the same result.

## Known Limits

### V1 Alpha Constraints

- **Polling-based**: No webhook support. Sync and reconcile are manual or scheduled.
- **Rate limits**: gh CLI inherits GitHub API rate limits (~5000/hour authenticated).
- **Single PR per task**: V1 enforces one active PR link per task.
- **Label management**: Kagan uses labels (`kagan:locked`, `kagan:mode:*`) on issues.

### Not implemented in V1

- Webhook-driven real-time sync
- Multi-PR per task support
- GitHub App installation verification in preflight
- Automatic lease renewal background worker

## Error Codes

| Code                      | Meaning                               | Fix                                    |
| ------------------------- | ------------------------------------- | -------------------------------------- |
| `GH_CLI_NOT_AVAILABLE`    | gh CLI not installed                  | `brew install gh`                      |
| `GH_AUTH_REQUIRED`        | Not authenticated                     | `gh auth login`                        |
| `GH_REPO_ACCESS_DENIED`   | Cannot access repository              | Check repo permissions                 |
| `GH_PROJECT_REQUIRED`     | Missing project_id                    | Provide valid project_id               |
| `GH_REPO_REQUIRED`        | Multi-repo needs repo_id              | Specify repo_id for multi-repo project |
| `GH_NOT_CONNECTED`        | Repo not connected to GitHub          | Run connect_repo first                 |
| `GH_SYNC_FAILED`          | Issue fetch failed                    | Check gh CLI auth and repo access      |
| `LEASE_HELD_BY_OTHER`     | Another instance holds lease          | Wait or use force_takeover             |
| `LEASE_NOT_HELD`          | Cannot release unowned lease          | Only holder can release                |
| `GH_NO_LINKED_PR`         | Task has no linked PR                 | Create or link a PR first              |
| `GH_PR_CREATE_FAILED`     | PR creation failed                    | Push branch, check permissions         |
| `GH_WORKSPACE_REQUIRED`   | Task has no workspace                 | Create workspace before PR             |

## MCP Tool Reference

### V1 Contract (Frozen)

| Tool Name                          | Type     | Profile    |
| ---------------------------------- | -------- | ---------- |
| `kagan_github_contract_probe`      | read     | MAINTAINER |
| `kagan_github_connect_repo`        | mutating | MAINTAINER |
| `kagan_github_sync_issues`         | mutating | MAINTAINER |
| `kagan_github_acquire_lease`       | mutating | MAINTAINER |
| `kagan_github_release_lease`       | mutating | MAINTAINER |
| `kagan_github_get_lease_state`     | read     | MAINTAINER |
| `kagan_github_create_pr_for_task`  | mutating | MAINTAINER |
| `kagan_github_link_pr_to_task`     | mutating | MAINTAINER |
| `kagan_github_reconcile_pr_status` | mutating | MAINTAINER |

All tools require `MAINTAINER` capability profile.
