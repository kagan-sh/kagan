# Configuration

All options are passed through OpenCode's plugin config — the array form of a plugin entry:

```json
{
  "plugin": [
    [
      "/path/to/kagan",
      {
        "inProgressLimit": 3,
        "intakeAgent": "plan",
        "validatorAgent": "review",
        "helperRetries": 2,
        "sendBackStopThreshold": 3,
        "squashMerge": true,
        "setupCommand": "bun install",
        "checkCommand": "bun run check"
      }
    ]
  ]
}
```

| Option                  | Type                        | Default         | Effect                                                                                                                                                                                                                                     |
| ----------------------- | --------------------------- | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `inProgressLimit`       | integer ≥ 1                 | `2`             | Max tasks in In Progress at once (root tasks only).                                                                                                                                                                                        |
| `validatorModels`       | `{ providerID, modelID }[]` | session default | Rotated reviewer models. When set, prefers entries whose `providerID` differs from the task's builder model, then rotates by generation (`(generation - 1) % n`). Falls back to the full list if every entry shares the builder provider.  |
| `intakeAgent`           | string                      | session default | OpenCode agent used for the `task prep` child session.                                                                                                                                                                                     |
| `validatorAgent`        | string                      | session default | OpenCode agent used for the `review` child session.                                                                                                                                                                                        |
| `helperRetries`         | integer ≥ 0                 | `1`             | Automatic respawns of a failed `task prep` / `review` helper before it's marked failed.                                                                                                                                                    |
| `sendBackStopThreshold` | integer ≥ 1                 | `3`             | After this many iterations, pressing send-back opens a stop dialog with choices to iterate again, take over the session, or leave it in review.                                                                                            |
| `squashMerge`           | boolean                     | `true`          | Squash the task branch into a single commit on merge. Set `false` for a standard merge that preserves the branch's individual commits.                                                                                                     |
| `setupCommand`          | string                      | unset (inert)   | Shell command run once in the fresh task worktree at creation time (e.g. `bun install`). Its exit code and output tail are recorded and shown as a `setup ok`/`setup failed` card badge. A failure never blocks task creation.             |
| `checkCommand`          | string                      | unset (inert)   | Shell command run once in the task worktree when a task enters Review. Its exit code and output tail are recorded as evidence for the validator and shown as a `check ok`/`check failed` card badge. Never gates approval or column moves. |

`checkCommand` also controls a deterministic advisory overlay: when it is unset, the intake mode rationale shown on the selected card and in the findings-review header appends "(no automatic check configured - lean assisted)". The mode recommendation itself — `autonomous`, `assisted`, or `manual` — is advisory only and never gates a move or approval.

`setupCommand` and `checkCommand` have a 300-second timeout and keep the last 4000 characters of
combined stdout/stderr.

Everything else is chosen per task in the create dialog: title, description, the implementing
model, and the base branch.
