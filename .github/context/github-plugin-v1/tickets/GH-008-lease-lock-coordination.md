# GH-008 - Lease/Lock Coordination via Labels and Comments

Status: Done
Owner: Codex
Completion: Implemented in `4427cfa6` on 2026-02-14.
Depends On: GH-003

## Outcome
Only one active Kagan instance can work a GitHub issue at a time by default.

## Scope
- Introduce default-enforced lease flow for GitHub-connected repos.
- Use label `kagan:locked` as lock signal.
- Use marker comment for lock holder metadata:
  - owner, instance id, lease expiry timestamp.
- Implement acquire/renew/release/takeover paths.
- Lease owner identity is instance-based; same GitHub user across devices still contends.
- Add repo-level maintainer opt-out switch for exceptional workflows.

## Acceptance Criteria
- Second instance cannot acquire an active lease without takeover path.
- Lease holder information is visible and actionable in TUI/MCP errors.
- Stale lease can be reclaimed deterministically.
- Default behavior prevents simultaneous active work across accounts and devices.

## Verification
- Unit tests for lease state transitions and contention.
- TUI/MCP tests for blocked + takeover flows.
- Avoid duplicative helper-level lease parsing tests when contention/takeover behavior is already covered end-to-end.

## Implementation Notes
- Lease operations (`acquire_lease`, `release_lease`, `get_lease_state`) are implemented in plugin use cases.
- Lock signal remains `kagan:locked` with marker-comment holder metadata.
- Cross-device contention is enforced via `kagan_instance_id` lease identity.
