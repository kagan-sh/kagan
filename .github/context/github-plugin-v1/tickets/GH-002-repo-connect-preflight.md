# GH-002 - Repo Connect and gh Preflight

Status: Done
Owner: Codex
Completion: Implemented in `d8f1c94b` on 2026-02-14.
Depends On: GH-001

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

### Files Added/Refined
- `src/kagan/core/plugins/github/gh_adapter.py` — gh preflight helpers and validated metadata parsing.
- `src/kagan/core/plugins/github/adapters/gh_cli_client.py` — GitHub client adapter used by use cases.

### Files Modified
- `src/kagan/core/plugins/github/contract.py` — Added `GITHUB_METHOD_CONNECT_REPO` operation
- `src/kagan/core/plugins/github/plugin.py` — Registered `connect_repo` operation as mutating.
- `src/kagan/core/plugins/github/entrypoints/plugin_handlers.py` — `handle_connect_repo()` maps payload to typed request.
- `src/kagan/core/plugins/github/application/use_cases.py` — connect orchestration, idempotency, and error shaping.
- `src/kagan/core/plugins/github/domain/repo_state.py` — canonical connection metadata encoding/decoding.

### Test Coverage
- `tests/core/unit/test_github_connect_repo.py` covers:
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
3. **Idempotent upsert**: core gateway persists connection metadata in `Repo.scripts["kagan.github.connection"]`
4. **Metadata normalization**: `parse_gh_repo_view()` extracts host, owner, name, visibility, default branch from gh JSON
5. **Multi-repo resolution**: Single-repo projects auto-resolve; multi-repo requires explicit `repo_id`
