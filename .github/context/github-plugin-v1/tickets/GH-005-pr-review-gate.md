# GH-005 - PR Create/Link and REVIEW Gate

Status: Todo
Owner: Codex
Depends On: GH-004

## Outcome
Moving tasks into REVIEW requires a linked PR and offers create/link actions.

## Scope
- Add create PR and link PR operations.
- Enforce REVIEW guardrail when PR missing.
- Persist PR linkage with existing merge/task models.
- Enforce active lease check before REVIEW transition in connected repos.

## Acceptance Criteria
- REVIEW transition blocked without PR.
- Create/link action path is single-step and predictable.
- REVIEW transition blocked when lease is held by another active instance.

## Verification
- Unit and TUI tests for transition gate behavior.
- Prioritize user-observable outcomes (blocked transition reason, create/link success path) over internal transition helper coverage.
