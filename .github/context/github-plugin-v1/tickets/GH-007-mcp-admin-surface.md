# GH-007 - MCP Admin Operations and Contracts

Status: Backlog
Owner: Codex
Depends On: GH-003
Note: Not started in current repo state; implementation details below are draft planning notes.

## Outcome
Expose minimal admin-safe MCP operations for GitHub plugin lifecycle.

## Scope
- Add `kagan_github_*` MCP tool bindings.
- Enforce capability profile boundaries.
- Return machine-readable recovery hints.

## Acceptance Criteria
- Tools callable by intended admin profiles only.
- Contract tests cover success and policy-denied paths.
- Tool names/params/error codes are documented and treated as stable V1 contract.

## Verification
- Focused MCP contract tests for externally visible guarantees:
  - tool names + required params
  - profile/policy denied behavior
  - success/error payload codes and remediation hints

## Implementation Summary

### Files Modified
- `src/kagan/mcp/registrars.py` — Registered 3 MCP tool handlers:
  - `kagan_github_contract_probe` — read-only scaffold verification
  - `kagan_github_connect_repo` — mutating repo connection with preflight
  - `kagan_github_sync_issues` — mutating issue sync with incremental checkpoint
- `src/kagan/mcp/models.py` — Added typed response models:
  - `GitHubContractProbeResponse` (adapter info, echo)
  - `GitHubConnectRepoResponse` (preflight details, connection metadata, error codes)
  - `GitHubSyncIssuesResponse` (sync summary counts, checkpoint, per-issue results)
- `src/kagan/mcp/tools.py` — Added bridge functions dispatching to `kagan_github` capability:
  - `github_contract_probe()`
  - `github_connect_repo()`
  - `github_sync_issues()`

### Test File Added
- `tests/mcp/contract/test_mcp_github_tools.py` — focused contract tests covering:
  - V1 tool name stability (names are frozen contract)
  - Capability profile gating (MAINTAINER only)
  - Parameter schema stability
  - Error code/hint behavior and read-only vs mutating policy flags

### Key Design Decisions
1. **MAINTAINER-only**: All GitHub MCP tools require `CapabilityProfile.MAINTAINER`
2. **Bridge pattern**: MCP tools delegate to plugin runtime via `_command("kagan_github", ...)` — no duplicate logic
3. **Stable contract**: Tool names, param schemas, and response fields treated as V1 freeze
4. **Machine-readable errors**: All failure responses include `code`, `message`, `hint` for admin tooling

## Refinement Notes (Post-Review)
- Do not freeze private model/class names as part of V1 guarantees; freeze only external MCP schema/contract behavior.

## Contract Scope Clarification
- V1 contract freeze in this ticket applies to GitHub plugin admin tools only:
  - `kagan_github_contract_probe`
  - `kagan_github_connect_repo`
  - `kagan_github_sync_issues`
- Generic/core MCP catalog entries outside `kagan_github_*` (for example `tasks_wait`) are not part
  of this initiative's frozen surface and may evolve independently.
