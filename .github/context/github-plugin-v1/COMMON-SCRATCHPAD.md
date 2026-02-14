# Common Scratchpad

## Initiative
- Name: `github-plugin-v1`
- Scope mode: MVP only
- Priority: shipped; now tracking post-delivery architecture notes

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
- Post-refactor module boundary is fixed to:
  - `plugin.py` -> `entrypoints/plugin_handlers.py` -> `application/use_cases.py`
  - ports (`core_gateway`, `gh_client`) with adapters (`core_gateway.py`, `gh_cli_client.py`)

## Risks
- Rate-limit behavior on large repos.
- Mapping drift when issues are renamed/reopened rapidly.
- UX confusion between local-only repos and GitHub-connected repos.
- Stale leases causing blocked work if holder disappears.

## Mitigations
- Incremental sync with checkpoints.
- Mapping drift recovery handled inline during sync (no explicit repair operation).
- Clear connected-repo badges and action labels in TUI.
- Lease expiry + maintainer takeover path.

## Completion Notes
1. Tickets GH-001..GH-010 are complete and landed.
2. Guardrails and contract alignment hardening landed post-ticket.
3. Architecture pivot landed with decoupled plugin layers and no compatibility shim path.
