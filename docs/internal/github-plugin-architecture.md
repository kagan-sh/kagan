# GitHub Plugin Architecture (Current + Target)

## Scope

This document tracks the implemented GitHub plugin architecture and the target
decoupled module shape.

- Current focus: stable V1 contract and predictable runtime behavior.
- Breaking changes are allowed in internal module boundaries.

## Contract Boundaries

Reserved official namespace (never registered by Kagan):

- `github.*`

Kagan plugin capability:

- `kagan_github`

Registered plugin methods (internal capability surface):

- `contract_probe`
- `connect_repo`
- `sync_issues`
- `acquire_lease`
- `release_lease`
- `get_lease_state`
- `create_pr_for_task`
- `link_pr_to_task`
- `reconcile_pr_status`

Frozen MCP V1 tools (public admin surface):

- `kagan_github_contract_probe`
- `kagan_github_connect_repo`
- `kagan_github_sync_issues`

## Current Runtime Flow

TUI path:

1. `CoreBackedApi.github_*` typed calls
1. Core request `("tui", "api_call")`
1. `handle_tui_api_call` allowlisted dispatch
1. `KaganAPI.github_*` (`GitHubApiMixin`)
1. `PluginRegistry.resolve_operation("kagan_github", method)`
1. Plugin handler (`plugin.py`) lazy-loads `operations` and dispatches by method
1. Operation module (`operations/*.py`) orchestrates resolver + state helpers + adapters

MCP path:

1. `kagan_github_*` MCP tool
1. Core bridge call
1. `KaganAPI.github_*` typed method
1. Same plugin/operations path as TUI

## Current Module Layout

- `src/kagan/core/plugins/github/contract.py`
- `src/kagan/core/plugins/github/plugin.py`
- `src/kagan/core/plugins/github/operations/__init__.py`
- `src/kagan/core/plugins/github/operations/common.py`
- `src/kagan/core/plugins/github/operations/resolver.py`
- `src/kagan/core/plugins/github/operations/state.py`
- `src/kagan/core/plugins/github/operations/connect.py`
- `src/kagan/core/plugins/github/operations/sync.py`
- `src/kagan/core/plugins/github/operations/lease.py`
- `src/kagan/core/plugins/github/operations/pr.py`
- `src/kagan/core/plugins/github/gh_adapter.py`
- `src/kagan/core/plugins/github/sync.py`
- `src/kagan/core/plugins/github/lease.py`
- `src/kagan/core/api_github.py`

Connection metadata policy:

- Canonical field: `repo`
- Legacy `name`-only metadata is rejected

## Current Architecture (ASCII)

```text
+--------------------+       +---------------------+
| TUI / MCP Frontends| ----> | KaganAPI.github_*   |
+--------------------+       +----------+----------+
                                        |
                                        v
                              +---------+----------+
                              | PluginRegistry      |
                              | (kagan_github.*)    |
                              +---------+----------+
                                        |
                                        v
                              +---------+----------+
                              | plugin.py          |
                              | (lazy dispatch)    |
                              +---------+----------+
                                        |
                                        v
                              +---------+----------+
                              | operations/*.py    |
                              | (connect/sync/     |
                              |  lease/pr)         |
                              +----+----------+----+
                                   |          |
                    +--------------+          +----------------+
                    v                                   v
         +----------+---------+                 +-------+------+
         | resolver/state     |                 | gh_adapter   |
         | helpers            |                 | + sync/lease |
         +----------+---------+                 +-------+------+
                    |                                   |
                    v                                   v
              +-----+------+                     +------+------+
              | Core/SQLite |                     | GitHub API |
              +------------+                     +-------------+
```

## Coupling Still Present

- Operation handlers still depend on `AppContext` concrete services.
- Repo script keys are still JSON payloads stored in `Repo.scripts`.
- Operation handlers still pass untyped `dict[str, Any]` payloads.
- Lease/sync/PR policies are split across helper modules, not explicit domain ports.

## Target Decoupled Layout

Proposed end-state modules:

- `src/kagan/core/plugins/github/domain/models.py`
- `src/kagan/core/plugins/github/domain/policies.py`
- `src/kagan/core/plugins/github/application/use_cases.py`
- `src/kagan/core/plugins/github/ports/gh_client.py`
- `src/kagan/core/plugins/github/ports/repo_store.py`
- `src/kagan/core/plugins/github/ports/task_gateway.py`
- `src/kagan/core/plugins/github/adapters/gh_cli_client.py`
- `src/kagan/core/plugins/github/adapters/core_gateway.py`
- `src/kagan/core/plugins/github/entrypoints/plugin_handlers.py`

Design rules:

- Entry points perform validation and mapping only.
- Use cases own orchestration and invariants.
- Ports define dependency contracts; adapters implement them.
- Domain layer has no dependency on `AppContext`, MCP, or TUI.
- No compatibility shims for removed internal APIs.

## Recommended Architecture (ASCII)

```text
+--------------------+       +-----------------------+
| TUI / MCP Entrypoints| --> | plugin_handlers.py    |
+--------------------+       +-----------+-----------+
                                        |
                                        v
                              +---------+----------+
                              | application/       |
                              | use_cases.py       |
                              +----+----------+----+
                                   |          |
                    +--------------+          +-----------------+
                    v                                 v
            +-------+--------+                +-------+--------+
            | ports/task_... |                | ports/gh_client|
            +-------+--------+                +-------+--------+
                    |                                 |
                    v                                 v
            +-------+--------+                +-------+--------+
            | adapters/core_ |                | adapters/gh_cli|
            | gateway.py     |                | _client.py     |
            +-------+--------+                +-------+--------+
                    |                                 |
                    v                                 v
              +-----+------+                    +-----+------+
              | Core/DB    |                    | GitHub API |
              +------------+                    +------------+
```

## Migration Plan (Breaking-Change Friendly)

1. Introduce typed DTOs and port interfaces for repo/task/gh operations.
1. Replace `operations/*.py` direct `AppContext` usage with gateway interfaces.
1. Keep `plugin.py` and `api_github.py` as thin entrypoint adapters.
1. Replace raw repo-script JSON handling with typed repository state adapter.
1. Remove obsolete helper aliases and doc references in the same change set.
