# GH-006 - PR Reconcile and Board Transitions

Status: Todo
Owner: Codex
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
