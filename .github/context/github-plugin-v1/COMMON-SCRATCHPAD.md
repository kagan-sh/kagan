# Common Scratchpad

## Initiative
- Name: `github-plugin-v1`
- Scope mode: MVP only
- Priority: ship official plugin UX first

## Locked Decisions
- Official plugin bundled in core.
- Third-party plugin runtime remains opt-in and disabled by default.
- gh CLI is the only GitHub transport in V1.
- No webhooks in V1.
- GitHub Issues are source of truth for issue-backed tasks.
- Lease/lock coordination is default-on for connected repos.
- Lock signal uses `kagan:locked` label + marker comment metadata.
- Lease identity is per `kagan_instance_id` (same GitHub user on another device still contends).
- AUTO/PAIR sync mode uses issue labels with repo-default fallback.
- V1 mode fallback defaults to `PAIR`.
- Label conflict rule: `kagan:mode:pair` wins and emits sync warning.

## Notes
- Keep implementation centered on existing plugin registry + core services.
- Avoid introducing package registry/signing workflows in this initiative.
- TUI should expose only high-signal GitHub actions in connected repos.
- Apply `PERSONA-QUALITY-GATES.md` with MVP pragmatism.

## Risks
- Rate-limit behavior on large repos.
- Mapping drift when issues are renamed/reopened rapidly.
- UX confusion between local-only repos and GitHub-connected repos.
- Stale leases causing blocked work if holder disappears.

## Mitigations
- Incremental sync with checkpoints.
- Explicit mapping repair operation.
- Clear connected-repo badges and action labels in TUI.
- Lease expiry + maintainer takeover path.

## Next Actions
1. Implement official `kagan_github` plugin skeleton and operation contract. (GH-001 — backlog)
2. Add repo connect + gh preflight + issue sync baseline. (GH-002, GH-003 — backlog)
3. Implement TUI connected-repo UX and sync controls. (GH-004 — unblocked)
4. Implement lease/lock policy and UI affordances. (GH-008 — unblocked)
5. Implement AUTO/PAIR sync mode policy. (GH-009 — unblocked)
6. Implement PR create/link and REVIEW gate. (GH-005 — blocked by GH-004)
7. Implement PR reconcile and board transitions. (GH-006 — blocked by GH-005)
8. Write docs and operator runbook. (GH-010 — blocked by GH-006, GH-008, GH-009)
