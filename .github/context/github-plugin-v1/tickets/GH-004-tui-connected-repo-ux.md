# GH-004 - TUI Connected-Repo UX and Sync Controls

Status: Todo
Owner: Codex
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
