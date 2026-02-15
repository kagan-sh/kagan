---
title: GitHub Plugin Operator Runbook
description: Operational procedures and checklists for GitHub plugin administration
icon: material/clipboard-check
---

# GitHub Plugin Operator Runbook

Operational procedures for GitHub-connected Kagan deployments.

## Operator Checklist: Initial Setup

### Prerequisites

- [ ] `gh` CLI installed (`gh --version`)
- [ ] `gh auth login` completed
- [ ] Verify access: `gh repo view <owner>/<repo>`
- [ ] Kagan project created with repository attached

### Connect Repository

- [ ] Run `kagan_github_connect_repo` for each managed repo
- [ ] Verify response: `code: "CONNECTED"` or `code: "ALREADY_CONNECTED"`
- [ ] Confirm `connection` metadata includes canonical `repo` (not legacy `name`), plus correct `owner` and `default_branch`

### Initial Sync

- [ ] Run `kagan_github_sync_issues` after connect
- [ ] Verify `stats` in response shows expected issue count
- [ ] Confirm tasks appear on board with `[GH-N]` prefix

## Operator Checklist: Routine Sync

Run periodically to keep board consistent with GitHub issues.

### Pre-Sync Checks

- [ ] Confirm `gh auth status` shows active session

### Sync Execution

```json
{
  "tool": "kagan_github_sync_issues",
  "arguments": {
    "project_id": "<project_id>"
  }
}
```

### Post-Sync Verification

- [ ] Check `stats.errors` is 0
- [ ] Compare `stats.total` with GitHub issue count
- [ ] Review `inserted`, `updated`, `reopened`, `closed` counts for expected changes

## Contract Scope

The frozen MCP V1 contract for the GitHub plugin currently exposes only:

- `kagan_github_contract_probe`
- `kagan_github_connect_repo`
- `kagan_github_sync_issues`

## Recovery Procedures

### Mapping Drift

If tasks and issues become desynchronized:

1. Re-run `kagan_github_sync_issues`
1. Sync will reconcile mappings based on issue numbers

### Connection Reset

To re-establish GitHub connection:

1. `kagan_github_connect_repo` is idempotent
1. Returns `ALREADY_CONNECTED` if valid connection exists
1. Will refresh if underlying metadata changed
1. If metadata lacks canonical `repo`, reconnect (legacy `name`-only metadata is invalid)

## Rate Limit Awareness

GitHub API rate limits (authenticated):

- Primary rate: ~5000 requests/hour
- Per-second throttle: ~30 requests/second

### Mitigation

- Batch sync operations (sync once, not per-issue)
- Monitor `gh api rate_limit` output

## Scheduled Operations Template

Kagan does not currently provide a CLI subcommand to invoke MCP tools directly.
`kagan mcp` starts an MCP server; it does not support `--call`.

To schedule GitHub sync:

1. Trigger sync from TUI (`github sync`) as part of your workflow, or
1. Use any MCP client (editor integration or standalone client) to call `kagan_github_sync_issues`
   against a running Kagan MCP server.

Example MCP call payload:

```json
{
  "tool": "kagan_github_sync_issues",
  "arguments": {
    "project_id": "<id>",
    "repo_id": "<optional>"
  }
}
```

## Monitoring Checklist

### Daily Checks

- [ ] `stats.errors` from sync is 0

### Weekly Checks

- [ ] Compare GitHub issue count with task count
- [ ] Review closed issues for tasks still in BACKLOG
- [ ] Audit `kagan:locked` labels match active workspaces
