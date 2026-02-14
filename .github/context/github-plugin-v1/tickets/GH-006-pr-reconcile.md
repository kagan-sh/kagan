# GH-006 - PR Reconcile and Board Transitions

Status: Done
Owner: Codex
Completion: Implemented in `9ce82e62` on 2026-02-14.
Depends On: GH-005

## Outcome
PR status reconciliation updates task state automatically.

## Scope
- Poll PR status for linked tasks.
- Map merged -> DONE and closed-unmerged -> IN_PROGRESS.
- Surface stale/error states clearly.

## Acceptance Criteria
- Reconcile is idempotent and safe to re-run.
- Board transitions are deterministic.
- Reconcile failures are surfaced with clear retry guidance (no silent drift).

## Verification
- Unit tests for open/merged/closed transitions.
- Keep tests focused on idempotent user-visible board outcomes, not polling-loop internals.

## Implementation Notes
- `reconcile_pr_status` operation is implemented in plugin use cases.
- Reconcile applies deterministic state transitions:
  - merged PR -> `DONE`
  - closed unmerged PR -> `IN_PROGRESS`
- Failure paths return machine-readable error codes and retry hints.
