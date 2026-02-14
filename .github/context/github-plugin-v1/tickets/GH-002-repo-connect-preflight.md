# GH-002 - Repo Connect and gh Preflight

Status: Backlog
Owner: Codex
Depends On: GH-001
Note: Not started in current repo state; implementation details below are draft planning notes.

## Outcome
User can connect a repo to GitHub with deterministic gh preflight checks.

## Scope
- Add connect operation.
- Validate `gh` install, auth status, and repo visibility.
- Persist repo-level GitHub connection metadata.

## Acceptance Criteria
- Clear machine-readable errors for each failed preflight step.
- Success path stores connection state and is idempotent.
- CLI/TUI errors include remediation text (what to run next).

## Verification
- Unit tests for success/failure scenarios.

## Implementation Summary

### Files Added
- `src/kagan/core/plugins/official/github/gh_adapter.py` — `GhRepoView` value object, `run_gh_auth_status()`, `run_gh_repo_view()`, `parse_gh_repo_view()` with full validation

### Files Modified
- `src/kagan/core/plugins/official/github/contract.py` — Added `GITHUB_METHOD_CONNECT_REPO` operation
- `src/kagan/core/plugins/official/github/plugin.py` — Registered `connect_repo` handler as mutating operation
- `src/kagan/core/plugins/official/github/runtime.py` — Implemented `handle_connect_repo()` with full preflight chain and `_resolve_connect_target()` for project/repo resolution

### Test Coverage (in `test_official_github_plugin.py`)
- gh CLI not available → `GH_CLI_NOT_AVAILABLE` error with install hint
- gh auth failure → `GH_AUTH_REQUIRED` error with `gh auth login` hint
- Repo access denied → `GH_REPO_ACCESS_DENIED` error with manual verify hint
- Invalid repo metadata → `GH_REPO_METADATA_INVALID` error
- Missing project → `GH_PROJECT_REQUIRED` error
- Missing repo → `GH_REPO_REQUIRED` error
- Idempotent connect → `ALREADY_CONNECTED` code on re-run

### Key Design Decisions
1. **Preflight chain**: Sequential checks (gh CLI → auth → repo access) with early return on first failure
2. **Machine-readable errors**: Every failure returns `code`, `message`, `hint` triple
3. **Idempotent upsert**: `ProjectService.upsert_repo_github_connection()` persists connection metadata in `Repo.scripts["kagan.github.connection"]`
4. **Metadata normalization**: `parse_gh_repo_view()` extracts host, owner, name, visibility, default branch from gh JSON
5. **Multi-repo resolution**: Single-repo projects auto-resolve; multi-repo requires explicit `repo_id`
