# GH-001 - Official Plugin Scaffold and Operation Contract

Status: Backlog
Owner: Codex
Depends On: -
Note: Not started in current repo state; implementation details below are draft planning notes.

## Outcome
Introduce the first official bundled GitHub plugin package and register it through bootstrap.

## Scope
- Add plugin module namespace under `src/kagan/core/plugins/official/github/`.
- Define canonical internal capability and method names.
- Register plugin during app bootstrap as official first-party module.

## Acceptance Criteria
- Plugin registers without replacing existing behavior.
- Contract is documented in code comments and tests.
- No collisions with existing core dispatch map.
- Core CLI startup path does not eagerly import GitHub plugin runtime modules.

## Verification
- Focused unit tests for user-visible scaffold behavior:
  - plugin registration ownership/collision behavior
  - lazy runtime loading on invocation
  - contract probe response surface

## Implementation Summary

### Files Added
- `src/kagan/core/plugins/official/github/__init__.py` — Public API exports
- `src/kagan/core/plugins/official/github/contract.py` — Canonical capability/method contracts (`GITHUB_PLUGIN_ID`, `GITHUB_PLUGIN_CAPABILITY`, `GitHubOperationContract`, `operation_contract_keys()`)
- `src/kagan/core/plugins/official/github/plugin.py` — `OfficialGitHubPlugin` scaffold with `PluginManifest`, lazy runtime loading via `importlib`, and `register_official_github_plugin()` entry point
- `src/kagan/core/plugins/official/github/gh_adapter.py` — `GhCliAdapterInfo`, `resolve_gh_cli()`, typed value objects
- `src/kagan/core/plugins/official/github/runtime.py` — `handle_contract_probe()` handler

### Test File Added
- `tests/core/unit/test_official_github_plugin.py` — 8 unit tests covering:
  - Plugin registration and capability ownership
  - Lazy runtime loading (no eager import of runtime modules)
  - Collision detection with existing core dispatch map
  - Contract probe handler returns correct metadata
  - Mutating vs read-only operation flags

### Key Design Decisions
1. **Plugin ID**: `official.github`, capability: `kagan_github`
2. **Lazy loading**: Runtime module imported only when operations are invoked via `importlib.import_module()`; `lru_cache` prevents re-imports
3. **Capability profile**: All operations gated to `CapabilityProfile.MAINTAINER`
4. **Contract stability**: Method names defined as `Literal` types in `contract.py` for V1 freeze
5. **Operation dispatch**: Prefer direct dispatch table mappings; add wrapper shims only when they enforce distinct policy/error boundaries.

## Refinement Notes (Post-Review)
- Avoid extra wrapper layers added only for style/readability if they do not change behavior.
- Test contract behavior, not private helper names or type alias internals.
