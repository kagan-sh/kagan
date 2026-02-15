---
title: Plugin SDK Lifecycle
description: Provider-neutral plugin scaffold lifecycle and conformance checklist
icon: material/puzzle
---

# Plugin SDK Lifecycle

Provider-neutral manifest, registration, policy, dispatch, and validation contracts for alpha plugins. GitHub provider plugin behavior (import, sync, export) is out of scope.

## Lifecycle

```mermaid
flowchart TD
  M["1. Manifest declaration<br/>(PluginManifest: id, name, version, entrypoint)"] --> R["2. Registry admission<br/>(validate shape/ID, reject duplicates)"]
  R --> O["3. Operation registration<br/>(>= 1 PluginOperation, globally unique capability/method)"]
  O --> P["4. Policy hook registration (optional)<br/>(PluginPolicyHook for owned operations)"]
  P --> E["5. Request policy evaluation<br/>(profile gate, then plugin hooks in order)"]
  E --> D["6. Dispatch<br/>(CoreHost routes capability.method to handler)"]
  D --> F["7. Failure handling<br/>(rollback manifest + operations + hooks)"]
```

## Conformance checklist

- [ ] Manifest validates against `PluginManifest` (`extra="forbid"`; required fields present)
- [ ] Plugin ID matches `^[a-z][a-z0-9_.-]{2,63}$`
- [ ] Plugin registers at least one operation
- [ ] Operation capability/method names match SDK patterns, no collisions
- [ ] Optional policy hooks only for owned operations
- [ ] Handlers return JSON-serializable dict payloads
- [ ] No provider-specific behavior in scaffold validation plugins

## Validation

```bash
uv run pytest tests/core/unit/test_plugin_sdk.py -v
```

Built-in provider-neutral stub: `src/kagan/core/plugins/examples/noop.py`.
