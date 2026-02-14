# GH-009 - AUTO/PAIR Sync Mode Policy

Status: Done
Owner: Codex
Completion: Implemented in `40e69e78` on 2026-02-14.
Depends On: GH-003

## Outcome
Synced GitHub issues map to task execution mode deterministically.

## Scope
- Define mode labels:
  - `kagan:mode:auto`
  - `kagan:mode:pair`
- Add repo default sync mode fallback.
- Set V1 default fallback to `PAIR`.
- Ensure mode mapping is documented and visible.
- Define authority chain:
  - issue labels decide first
  - repo default decides when labels are absent
  - repo default is set by maintainers/admins

## Acceptance Criteria
- Label precedence is deterministic and tested.
- Missing-label fallback uses configured repo default.
- Behavior is clear in user docs.
- Conflicting labels resolve deterministically (`pair` wins) with warning telemetry/log.

## Verification
- Unit tests for label combinations and fallback behavior.
- TUI display test for synced mode indicator.
- Keep coverage minimal and contract-focused; avoid tautological tests for enum/constants wiring.

## Implementation Notes
- Mode mapping is resolved during sync in plugin domain/sync logic.
- Label precedence is deterministic (`kagan:mode:pair` overrides `kagan:mode:auto`).
- Repo default mode remains explicit fallback when mode labels are absent.
