# GH-001 - Official Plugin Scaffold and Operation Contract

Status: Done
Owner: Codex
Completion: Merged in `35a301a54fae3a0d4ed78a150025b65015ee441a` on 2026-02-14.
Depends On: -

## Outcome
Introduce the first official bundled GitHub plugin package and register it through bootstrap.

## Scope
- Add plugin module namespace under `src/kagan/core/plugins/github/`.
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

### Final Module Shape (After Refactor Pivot)
- `src/kagan/core/plugins/github/contract.py` — canonical capability/method constants.
- `src/kagan/core/plugins/github/plugin.py` — `GitHubPlugin` registration + lazy dispatch.
- `src/kagan/core/plugins/github/entrypoints/plugin_handlers.py` — operation handlers and request mapping.
- `src/kagan/core/plugins/github/application/use_cases.py` — orchestration and policy.
- `src/kagan/core/plugins/github/domain/*` — typed request models and repo script state adapters.
- `src/kagan/core/plugins/github/ports/*` + `src/kagan/core/plugins/github/adapters/*` — boundary interfaces and concrete implementations.

### Test Coverage
- `tests/core/unit/test_plugin_sdk.py` validates registration ownership, contract probe surface,
  and lazy handler import behavior.
- `tests/core/unit/test_api_github_typed.py` validates typed API dispatch to plugin operations.

### Key Design Decisions
1. **Plugin ID**: `official.github`, capability: `kagan_github`
2. **Lazy loading**: Handler module imported only on invocation via `importlib.import_module()`.
3. **Capability profile**: All operations gated to `CapabilityProfile.MAINTAINER`
4. **Contract stability**: canonical method names are centralized in `contract.py` for V1 freeze.
5. **Decoupling pivot**: remove monolithic runtime/service path and keep isolated entrypoint/use-case/port layers.

## Refinement Notes (Post-Review)
- Avoid extra wrapper layers added only for style/readability if they do not change behavior.
- Test contract behavior, not private helper names or type alias internals.
