# GH-005 - PR Create/Link and REVIEW Gate

Status: Done
Owner: Codex
Completion: Implemented in `13bdb265` on 2026-02-14.
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

## Implementation Notes
- `create_pr_for_task` and `link_pr_to_task` operations are routed through plugin use cases.
- REVIEW transition enforcement checks linked PR state through plugin guardrails.
- Multi-repo REVIEW checks are scoped to the task owning repo.
