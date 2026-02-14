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
- [ ] Confirm `connection` metadata includes correct `owner`, `name`, `default_branch`

### Initial Sync

- [ ] Run `kagan_github_sync_issues` after connect
- [ ] Verify `stats` in response shows expected issue count
- [ ] Confirm tasks appear on board with `[GH-N]` prefix

## Operator Checklist: Routine Sync

Run periodically to keep board consistent with GitHub issues.

### Pre-Sync Checks

- [ ] Confirm `gh auth status` shows active session
- [ ] Verify no ongoing PR reconciliations for critical tasks

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

## Operator Checklist: PR Reconciliation

Run to sync PR status to task board state.

### For Each Task in REVIEW

```json
{
  "tool": "kagan_github_reconcile_pr_status",
  "arguments": {
    "project_id": "<project_id>",
    "task_id": "<task_id>"
  }
}
```

### Expected Transitions

| Observed PR State | Expected Task Status |
| ----------------- | -------------------- |
| `MERGED`          | `DONE`               |
| `CLOSED`          | `IN_PROGRESS`        |
| `OPEN`            | No change            |

### Post-Reconcile Checks

- [ ] Verify merged PRs moved tasks to DONE
- [ ] Verify closed PRs (unmerged) returned tasks to IN_PROGRESS
- [ ] Check for error responses indicating stale PR links

## Lease Troubleshooting

### Symptom: `LEASE_HELD_BY_OTHER`

1. Check holder info in response:
   - `holder.instance_id`: hostname:pid of holder
   - `holder.expires_at`: lease expiry timestamp
   - `holder.github_user`: authenticated user if known

2. If holder instance is no longer running:
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

3. If lease expired more than 2 hours ago, takeover is automatic.

### Symptom: Orphan `kagan:locked` Label

If issue has `kagan:locked` label but no lease comment:

1. This is an orphan state from incomplete cleanup.
2. `acquire_lease` will allow takeover.
3. Alternatively, manually remove the label in GitHub.

## Recovery Procedures

### Mapping Drift

If tasks and issues become desynchronized:

1. Re-run `kagan_github_sync_issues`
2. Sync will reconcile mappings based on issue numbers

### Stale PR Links

If task has a linked PR that was deleted:

1. Run `kagan_github_reconcile_pr_status` (will return error)
2. Unlink stale PR via database or future `unlink_pr` tool
3. Create or link new PR

### Connection Reset

To re-establish GitHub connection:

1. `kagan_github_connect_repo` is idempotent
2. Returns `ALREADY_CONNECTED` if valid connection exists
3. Will refresh if underlying metadata changed

## Rate Limit Awareness

GitHub API rate limits (authenticated):

- Primary rate: ~5000 requests/hour
- Per-second throttle: ~30 requests/second

### Mitigation

- Batch sync operations (sync once, not per-issue)
- Space reconcile calls for large task sets
- Monitor `gh api rate_limit` output

## Scheduled Operations Template

For cron/scheduler integration:

```bash
# Sync issues every 15 minutes
*/15 * * * * kagan mcp --call kagan_github_sync_issues --args '{"project_id":"<id>"}'

# Reconcile PRs hourly
0 * * * * for task in $(kagan task list --status REVIEW --format ids); do
    kagan mcp --call kagan_github_reconcile_pr_status --args "{\"project_id\":\"<id>\",\"task_id\":\"$task\"}"
done
```

## Monitoring Checklist

### Daily Checks

- [ ] `stats.errors` from sync is 0
- [ ] No stale tasks in REVIEW (PRs merged but not reconciled)
- [ ] No orphan leases (issues locked with no active work)

### Weekly Checks

- [ ] Compare GitHub issue count with task count
- [ ] Review closed issues for tasks still in BACKLOG
- [ ] Audit `kagan:locked` labels match active workspaces
