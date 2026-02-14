# GH-003 - Issue Sync and Mapping Projection

Status: Done
Owner: Codex
Completion: Implemented in `9f256ddf` on 2026-02-14.
Depends On: GH-002

## Outcome
GitHub issues are synchronized into Kagan task projections.

## Scope
- Add incremental sync operation and checkpoint tracking.
- Upsert task projection from issue metadata.
- Maintain issue-to-task mapping with inline drift recovery during sync.
- Resolve task mode from labels/default policy during sync.

## Acceptance Criteria
- Sync is idempotent.
- Re-running sync without remote changes produces no task churn.
- Mapping recovery path exists for drift.

## Verification
- Focused unit tests for user-visible sync outcomes:
  - insert/update/reopen/close projection behavior
  - idempotent no-churn re-sync behavior
  - deterministic mode resolution and mapping drift recovery outcomes

## Implementation Summary

### Files Added
- `src/kagan/core/plugins/github/sync.py` - Sync state, mapping, checkpoint, and mode resolution logic

### Files Modified
- `src/kagan/core/plugins/github/contract.py` - Added `GITHUB_METHOD_SYNC_ISSUES` operation
- `src/kagan/core/plugins/github/plugin.py` - Registered sync_issues handler
- `src/kagan/core/plugins/github/gh_adapter.py` - Added `GhIssue`, `run_gh_issue_list()`, `parse_gh_issue_list()`
- `src/kagan/core/plugins/github/entrypoints/plugin_handlers.py` - `handle_sync_issues()` typed request mapping
- `src/kagan/core/plugins/github/application/use_cases.py` - sync orchestration with checkpoint filtering
- `src/kagan/core/plugins/github/domain/repo_state.py` - typed checkpoint/mapping persistence adapters

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
5. **Drift recovery**: If mapped task is deleted, sync recreates it and rewrites mapping atomically.

## Refinement Notes (Post-Review)
- Avoid over-testing parse/checkpoint internals when end-to-end sync behavior already covers them.
- Prefer a small, high-signal suite over broad helper-level coverage.
