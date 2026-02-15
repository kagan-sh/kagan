---
title: Plugin UI Schema (TUI)
description: Declarative, allowlisted plugin UI contributions for the Kagan TUI
---

# Plugin UI Schema (TUI)

Kagan supports a schema-driven way for plugins to contribute UI into the TUI without shipping any
plugin-provided UI code. Plugins provide declarative data (actions, forms, badges). The core
validates and sanitizes it, then the TUI renders it.

This preserves architecture boundaries:

- Core remains the orchestration brain and the single source of state.
- TUI is a thin client: it renders a catalog and invokes actions through core APIs.
- Plugins cannot inject executable UI logic, styling, or arbitrary markup.

## Enablement (Allowlist)

Only allowlisted plugins are permitted to contribute UI into the TUI.

Configure in `kagan.toml`:

```toml
[ui]
tui_plugin_ui_allowlist = ["github"]
```

If a plugin is not on the allowlist, core will exclude it from the TUI catalog and reject invokes.

## Core \<-> TUI Flow

1. TUI requests a catalog with `plugin_ui_catalog(project_id, repo_id?)`.
1. Core gathers `ui_describe` from allowlisted plugins, validates/sanitizes, and returns a merged catalog.
1. TUI renders:
   - actions in supported surfaces
   - badges in supported surfaces
   - forms when an action references a form
1. When the user runs an action, TUI calls `plugin_ui_invoke(project_id, repo_id?, plugin_id, action_id, inputs?)`.
1. Core resolves `action_id -> operation (capability, method)` server-side and executes the plugin operation.

The client never supplies the operation reference, and core does not trust client-side mutation.

## `ui_describe` Plugin Contract

Plugins contribute UI by registering an operation with method name `ui_describe` (snake_case).
It must be:

- non-mutating
- safe to call frequently

`ui_describe` returns a JSON object with `schema_version: "1"` and any of `actions`, `forms`, `badges`.
Unknown keys are ignored. Invalid objects are dropped individually; the catalog still returns a valid
shape.

## Schema (V1)

Allowed surfaces:

- `kanban.repo_actions`
- `kanban.task_actions`
- `header.badges`

Allowed field kinds:

- `text`
- `select`
- `boolean`

Badge states:

- `ok`
- `warn`
- `error`
- `info`

### Example

```json
{
  "schema_version": "1",
  "actions": [
    {
      "plugin_id": "github",
      "action_id": "connect_repo",
      "surface": "kanban.repo_actions",
      "label": "Connect GitHub Repo",
      "command": "github connect",
      "help": "Connect the selected repo to GitHub.",
      "operation": {"capability": "github", "method": "github_connect_repo"},
      "form_id": "connect_repo_form",
      "confirm": false
    }
  ],
  "forms": [
    {
      "form_id": "connect_repo_form",
      "title": "Connect GitHub Repo",
      "fields": [
        {"name": "project_id", "kind": "text", "required": true},
        {
          "name": "repo_id",
          "kind": "select",
          "required": false,
          "options": [{"label": "repo-1", "value": "repo-1"}]
        }
      ]
    }
  ],
  "badges": [
    {
      "plugin_id": "github",
      "badge_id": "connection",
      "surface": "header.badges",
      "label": "GitHub",
      "state": "info",
      "text": "Sync stale"
    }
  ]
}
```

## Invocation Result Contract

`plugin_ui_invoke` returns a stable envelope:

```json
{
  "ok": true,
  "code": "OK",
  "message": "OK",
  "data": {},
  "refresh": {"repo": true, "tasks": true, "sessions": false}
}
```

The TUI uses `refresh` hints to call existing refresh flows (no client-side fallback mutation).
