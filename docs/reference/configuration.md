# Configuration

All options are passed through OpenCode's plugin config — the array form of a plugin entry:

```json
{
  "plugin": [
    [
      "@kagan-sh/kagan",
      {
        "inProgressLimit": 3,
        "intakeAgent": "plan",
        "validatorAgent": "review",
        "helperRetries": 2,
        "sendBackStopThreshold": 3,
        "squashMerge": true,
        "commands": {
          "setup": [{ "name": "deps", "cwd": ".", "command": "bun install" }],
          "check": [{ "name": "verify", "cwd": ".", "command": "bun run verify" }]
        }
      }
    ]
  ]
}
```

| Option                  | Type                        | Default         | Effect                                                                                                                                                                                                                                    |
| ----------------------- | --------------------------- | --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `inProgressLimit`       | integer ≥ 1                 | `2`             | Max tasks in In Progress at once (root tasks only).                                                                                                                                                                                       |
| `validatorModels`       | `{ providerID, modelID }[]` | session default | Rotated reviewer models. When set, prefers entries whose `providerID` differs from the task's builder model, then rotates by generation (`(generation - 1) % n`). Falls back to the full list if every entry shares the builder provider. |
| `intakeAgent`           | string                      | session default | OpenCode agent used for the `task prep` child session.                                                                                                                                                                                    |
| `validatorAgent`        | string                      | session default | OpenCode agent used for the `review` child session.                                                                                                                                                                                       |
| `helperRetries`         | integer ≥ 0                 | `1`             | Automatic respawns of a failed `task prep` / `review` helper before it's marked failed.                                                                                                                                                   |
| `sendBackStopThreshold` | integer ≥ 1                 | `3`             | After this many iterations, pressing send-back opens a stop dialog with choices to iterate again, take over the session, or leave it in review.                                                                                           |
| `squashMerge`           | boolean                     | `true`          | Squash the task branch into a single commit on merge. Set `false` for a standard merge that preserves the branch's individual commits.                                                                                                    |
| `commands.setup`        | command[]                   | unset (inert)   | Setup commands run once in the fresh task worktree when the task scope includes the command `cwd`. Ran commands are recorded as evidence. A failure never blocks task creation.                                                           |
| `commands.check`        | command[]                   | unset (inert)   | Check commands evaluated when a task enters Review. A command runs when changed files are under its `cwd`, or when changed files match one of its repo-relative `scope` regexes. Never gates approval or column moves.                    |

Each command has this shape:

```json
{
  "name": "alpha check",
  "cwd": "project-alpha",
  "command": "npm run verify",
  "scope": ["^\\.github/", "^scripts/"]
}
```

`cwd` is relative to the task worktree root. `scope` is optional and contains regular expressions matched against repo-relative changed paths; it is for shared files outside `cwd` that should trigger the command.

Monorepo example:

```json
{
  "plugin": [
    [
      "@kagan-sh/kagan",
      {
        "commands": {
          "setup": [
            { "name": "alpha deps", "cwd": "project-alpha", "command": "npm ci" },
            { "name": "beta deps", "cwd": "project-beta", "command": "npm ci" }
          ],
          "check": [
            {
              "name": "alpha check",
              "cwd": "project-alpha",
              "command": "npm run verify",
              "scope": ["^\\.github/", "^scripts/"]
            },
            {
              "name": "beta check",
              "cwd": "project-beta",
              "command": "npm run verify",
              "scope": ["^\\.github/", "^scripts/"]
            }
          ]
        }
      }
    ]
  ]
}
```

Configured command `cwd` values become task scopes in the create-task dialog. If there is one static scope it is preselected; if there are several, choose at least one scope or enter custom scope text. Custom free-form scope text is saved on the task. It triggers setup commands only when it exactly matches a configured `cwd`.

Configured checks also control a deterministic advisory overlay: when no check command is configured, the intake mode rationale shown on the selected card and in the findings-review header appends "(no automatic check configured - lean assisted)". The mode recommendation itself — `autonomous`, `assisted`, or `manual` — is advisory only and never gates a move or approval.

Setup and check commands have a 300-second timeout and keep the last 4000 characters of combined
stdout/stderr per command.

Open settings with `/kagan-settings` or `,` on the board. Settings can edit the Kagan plugin options
and save them to project `opencode.json`; restart OpenCode or reopen the project for saved settings
to apply.

Everything else is chosen per task in the create dialog: title, description, the implementing
model, and the base branch.
