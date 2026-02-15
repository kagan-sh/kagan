# GitHub Plugin Architecture

## Scope

This document describes the implemented GitHub plugin boundaries and runtime flow.

- Focus: stable V1 contract and predictable runtime behavior.
- Internal module boundaries can evolve, but contract constants and MCP V1 tools are stable.

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

## Runtime Flow

TUI path:

1. `CoreBackedApi.github_*` typed calls
1. Core request `("tui", "api_call")`
1. `handle_tui_api_call` allowlisted dispatch
1. `KaganAPI.github_*` (`GitHubApiMixin`)
1. `PluginRegistry.resolve_operation("kagan_github", method)`
1. `src/kagan/core/plugins/github/plugin.py` lazy-loads `entrypoints/plugin_handlers.py`
1. `plugin_handlers.py` maps request dicts to typed inputs and calls `GitHubPluginUseCases`
1. `application/use_cases.py` orchestrates domain policies and writes through ports
1. `adapters/*` implement ports using `AppContext` services and gh CLI helpers

MCP path:

1. `kagan_github_*` MCP tool
1. Core bridge call
1. `KaganAPI.github_*` typed method
1. Same plugin handler/use-case flow as TUI

## Module Layout

- `src/kagan/core/plugins/github/contract.py`
- `src/kagan/core/plugins/github/plugin.py`
- `src/kagan/core/plugins/github/entrypoints/plugin_handlers.py`
- `src/kagan/core/plugins/github/application/use_cases.py`
- `src/kagan/core/plugins/github/domain/models.py`
- `src/kagan/core/plugins/github/domain/repo_state.py`
- `src/kagan/core/plugins/github/ports/core_gateway.py`
- `src/kagan/core/plugins/github/ports/gh_client.py`
- `src/kagan/core/plugins/github/adapters/core_gateway.py`
- `src/kagan/core/plugins/github/adapters/gh_cli_client.py`
- `src/kagan/core/plugins/github/gh_adapter.py`
- `src/kagan/core/plugins/github/sync.py`
- `src/kagan/core/plugins/github/lease.py`
- `src/kagan/core/api_github.py`

Connection metadata policy:

- Canonical field: `repo`
- Legacy `name`-only metadata is rejected

## Module Graph (ASCII)

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
                              | entrypoints/       |
                              | plugin_handlers.py |
                              +---------+----------+
                                        |
                                        v
                              +---------+----------+
                              | application/       |
                              | use_cases.py       |
                              +----+----------+----+
                                   |          |
                    +--------------+          +----------------+
                    v                                   v
         +----------+---------+                 +-------+------+
         | ports/core_gateway |                 | ports/gh_client
         +----------+---------+                 +-------+------+
                    |                                   |
                    v                                   v
         +----------+---------+                 +-------+------+
         | adapters/core_     |                 | adapters/    |
         | gateway.py         |                 | gh_cli_client|
         +----------+---------+                 +-------+------+
                    |                                   |
                    v                                   v
              +-----+------+                     +------+------+
              | Core/SQLite |                     | GitHub API |
              +------------+                     +-------------+
```

## Boundary Rules

- `plugin.py` is only a registration and lazy-dispatch entrypoint.
- `plugin_handlers.py` only maps payloads and delegates to typed use cases.
- `use_cases.py` owns orchestration logic and response shaping.
- Repo script JSON encoding/decoding is isolated to `domain/repo_state.py`.
- DB writes flow through core service adapters (`core_gateway.py`) only.
- No `src/kagan/core/plugins/github/operations/*` compatibility layer.
