# Ticket Index

| Ticket | Title | Status | Owner | Depends On | Completion Commit |
|---|---|---|---|---|---|
| [GH-001](tickets/GH-001-plugin-scaffold.md) | Official plugin scaffold and operation contract | Done | Codex | - | `35a301a5` |
| [GH-002](tickets/GH-002-repo-connect-preflight.md) | Repo connect and gh preflight | Done | Codex | GH-001 | `d8f1c94b` |
| [GH-003](tickets/GH-003-issue-sync-mapping.md) | Issue sync and mapping projection | Done | Codex | GH-002 | `9f256ddf` |
| [GH-004](tickets/GH-004-tui-connected-repo-ux.md) | TUI connected-repo UX and sync controls | Done | Codex | GH-003 | `2a0c25af` |
| [GH-005](tickets/GH-005-pr-review-gate.md) | PR create/link and REVIEW gate | Done | Codex | GH-004 | `13bdb265` |
| [GH-006](tickets/GH-006-pr-reconcile.md) | PR reconcile and board transitions | Done | Codex | GH-005 | `9ce82e62` |
| [GH-007](tickets/GH-007-mcp-admin-surface.md) | MCP admin operations and contracts | Done | Codex | GH-003 | `7781002a` |
| [GH-008](tickets/GH-008-lease-lock-coordination.md) | Lease/lock coordination via labels/comments | Done | Codex | GH-003 | `4427cfa6` |
| [GH-009](tickets/GH-009-task-mode-policy.md) | AUTO/PAIR sync mode policy | Done | Codex | GH-003 | `40e69e78` |
| [GH-010](tickets/GH-010-docs-runbook.md) | Docs and operator runbook | Done | Codex | GH-006, GH-007, GH-008, GH-009 | `7326c988` |

Reference: `PERSONA-QUALITY-GATES.md` applies to all tickets.
Post-ticket architecture pivot/refactor completion commits:
- `1d25b44` — harden boundaries and align docs/tests
- `67a0e9d` — split runtime monolith into operation modules
- `e9575a2` — decouple plugin with use-case and port layers
