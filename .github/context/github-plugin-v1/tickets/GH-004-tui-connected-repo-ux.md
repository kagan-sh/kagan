# GH-004 - TUI Connected-Repo UX and Sync Controls

Status: Done
Owner: Codex
Completion: Implemented in `2a0c25af` on 2026-02-14.
Depends On: GH-003

## Outcome
Kanban UI clearly reflects GitHub-connected repo mode and sync operations.

## Scope
- Add connected-state indicator in board header.
- Add sync actions and status feedback.
- Show issue metadata on cards/details.

## Acceptance Criteria
- User can discover sync/connect status without hidden menus.
- Connected-mode actions are obvious and minimal.
- Sync/reconcile actions remain responsive (no blocking UI interactions).
- Keyboard navigation can reach all new GitHub actions.

## Verification
- TUI smoke tests for visibility and action triggers.
- Tests must remain minimal and user-facing (no widget-internal tautology assertions).

## Implementation Notes
- Header/status widgets now surface connected and synced state explicitly.
- Connected-repo actions are reachable through keyboard-driven command flows.
- TUI remains a thin client over core GitHub API operations.
