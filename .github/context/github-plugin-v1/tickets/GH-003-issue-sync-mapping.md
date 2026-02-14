# GH-003 - Issue Sync and Mapping Projection

Status: Backlog
Owner: Codex
Depends On: GH-002
Note: Not started in current repo state; implementation details below are draft planning notes.

## Outcome
GitHub issues are synchronized into Kagan task projections.

## Scope
- Add incremental sync operation and checkpoint tracking.
- Upsert task projection from issue metadata.
- Maintain issue-to-task mapping and repair hooks.
- Resolve task mode from labels/default policy during sync.

## Acceptance Criteria
- Sync is idempotent.
- Re-running sync without remote changes produces no task churn.
- Mapping recovery path exists for drift.

## Verification
- Focused unit tests for user-visible sync outcomes:
  - insert/update/reopen/close projection behavior
  - idempotent no-churn re-sync behavior
  - deterministic mode resolution and mapping repair outcomes

## Implementation Summary

### Files Added
- `src/kagan/core/plugins/official/github/sync.py` - Sync state, mapping, and mode resolution logic

### Files Modified
- `src/kagan/core/plugins/official/github/contract.py` - Added `GITHUB_METHOD_SYNC_ISSUES` operation
- `src/kagan/core/plugins/official/github/plugin.py` - Registered sync_issues handler
- `src/kagan/core/plugins/official/github/gh_adapter.py` - Added `GhIssue`, `run_gh_issue_list()`, `parse_gh_issue_list()`
- `src/kagan/core/plugins/official/github/runtime.py` - Implemented `handle_sync_issues()` handler

### Test File Added
- `tests/core/unit/test_github_issue_sync.py` - focused tests covering:
  - Insert/update/reopen/close task projection behavior from issue state
  - Idempotency (re-running sync without changes = no churn)
  - Mode resolution from labels/defaults
  - Mapping drift recovery behavior

### Key Design Decisions
1. **Checkpoint storage**: Uses `Repo.scripts` JSON storage for `kagan.github.sync_checkpoint`
2. **Issue mapping**: Uses `Repo.scripts` for `kagan.github.issue_mapping` with bidirectional lookup
3. **Mode resolution**: Labels `kagan:mode:auto` and `kagan:mode:pair` control TaskType; defaults to PAIR
4. **Task title format**: `[GH-{number}] {title}` for clear attribution
5. **Drift recovery**: If mapped task is deleted, sync recreates it with updated mapping

## Refinement Notes (Post-Review)
- Avoid over-testing parse/checkpoint internals when end-to-end sync behavior already covers them.
- Prefer a small, high-signal suite over broad helper-level coverage.
