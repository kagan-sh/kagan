# GH-007 - MCP Admin Operations and Contracts

Status: Done
Owner: Codex
Completion: Implemented in `7781002a` on 2026-02-14.
Depends On: GH-003

## Outcome
Expose minimal admin-safe MCP operations for GitHub plugin lifecycle.

## Scope
- Add `kagan_github_*` MCP tool bindings.
- Enforce capability profile boundaries.
- Return machine-readable recovery hints.

## Acceptance Criteria
- [x] Tools callable by intended admin profiles only.
- [x] Contract tests cover success and policy-denied paths.
- [x] Tool names/params/error codes are documented and treated as stable V1 contract.

## V1 Contract Specification (Frozen)

### Tool Names
| Tool Name | Type | Description |
|-----------|------|-------------|
| `kagan_github_contract_probe` | read-only | Scaffold verification probe |
| `kagan_github_connect_repo` | mutating | Connect repo to GitHub with preflight |
| `kagan_github_sync_issues` | mutating | Sync GitHub issues to Kagan tasks |

### Parameter Schemas

**kagan_github_contract_probe**
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `echo` | string | No | Value to echo back for verification |

**kagan_github_connect_repo**
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_id` | string | Yes | Project ID |
| `repo_id` | string | No | Repo ID (required for multi-repo projects) |

**kagan_github_sync_issues**
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project_id` | string | Yes | Project ID |
| `repo_id` | string | No | Repo ID (required for multi-repo projects) |

### Error Codes
| Code | Tool(s) | Description |
|------|---------|-------------|
| `GH_PROJECT_REQUIRED` | connect_repo, sync_issues | project_id missing or invalid |
| `GH_REPO_REQUIRED` | connect_repo, sync_issues | repo_id required for multi-repo |
| `GH_CLI_NOT_AVAILABLE` | connect_repo, sync_issues | gh CLI not installed |
| `GH_AUTH_REQUIRED` | connect_repo | gh CLI not authenticated |
| `GH_REPO_ACCESS_DENIED` | connect_repo | Repository not accessible |
| `GH_REPO_METADATA_INVALID` | connect_repo | Stored connection metadata invalid |
| `GH_NOT_CONNECTED` | sync_issues | Repo not connected to GitHub |
| `GH_SYNC_FAILED` | sync_issues | Issue fetch failed |
| `ALREADY_CONNECTED` | connect_repo | Repo already connected (idempotent) |
| `CONNECTED` | connect_repo | Successfully connected |
| `SYNCED` | sync_issues | Successfully synced |

## Implementation Summary

### Files Modified
- `src/kagan/mcp/registrars.py` — Registered 3 MCP tool handlers via `register_github_tools()`:
  - `kagan_github_contract_probe` — read-only scaffold verification
  - `kagan_github_connect_repo` — mutating repo connection with preflight
  - `kagan_github_sync_issues` — mutating issue sync with incremental checkpoint
- `src/kagan/mcp/models.py` — Added typed response models:
  - `GitHubContractProbeResponse` (adapter info, echo)
  - `GitHubConnectRepoResponse` (connection metadata, error codes)
  - `GitHubSyncIssuesResponse` (sync stats)
  - `GitHubConnectionMetadata` (full_name, owner, repo, default_branch, visibility)
  - `GitHubSyncStats` (total, inserted, updated, reopened, closed, no_change, errors)
- `src/kagan/mcp/tools.py` — Added CoreClientBridge methods:
  - `github_contract_probe()`
  - `github_connect_repo()`
  - `github_sync_issues()`

### Test Files Added
- `tests/plugins/github/test_mcp_github_tools_contract.py` — focused contract tests covering:
  - V1 tool name stability (names are frozen contract)
  - Capability profile gating (MAINTAINER only)
  - Parameter schema stability
  - Tool annotation verification (read-only vs mutating)
- Updated `tests/mcp/contract/test_mcp_v2_tool_catalog.py` — added GitHub tools to catalog
- Updated `tests/mcp/contract/test_mcp_tool_annotations.py` — added GitHub tool annotations

### Key Design Decisions
1. **MAINTAINER-only**: All GitHub MCP tools require `CapabilityProfile.MAINTAINER`
2. **Bridge pattern**: MCP tools delegate to plugin runtime via `_command("kagan_github", ...)` — no duplicate logic
3. **Stable contract**: Tool names, param schemas, and response fields treated as V1 freeze
4. **Machine-readable errors**: All failure responses include `code`, `message`, `hint` for admin tooling

## Contract Scope Clarification
- V1 contract freeze in this ticket applies to GitHub plugin admin tools only:
  - `kagan_github_contract_probe`
  - `kagan_github_connect_repo`
  - `kagan_github_sync_issues`
- Generic/core MCP catalog entries outside `kagan_github_*` (for example `tasks_wait`) are not part
  of this initiative's frozen surface and may evolve independently.
