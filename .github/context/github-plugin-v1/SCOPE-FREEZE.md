# Scope Freeze

## In Scope (V1)
- Official bundled GitHub plugin.
- Repo connect and gh auth preflight.
- GitHub issue sync into Kanban tasks.
- Label-based lease/lock coordination for multi-developer concurrency.
- PR create/link/reconcile for REVIEW/DONE transitions.
- Minimal MCP admin operations for connect/sync/reconcile/repair.

## Out of Scope (V1)
- Webhooks and GitHub App event ingestion.
- Full bidirectional issue editing parity.
- Multi-PR orchestration per task.
- Separate distributed lock service outside GitHub labels/comments.
- Third-party plugin marketplace/registry.
- Cross-repo portfolio views.
- New service-per-operation architecture framework or plugin microservice split.

## Guardrails
- No new persistence system outside existing SQLite boundaries.
- No client-side state mutation bypassing core services.
- No broad architecture rewrites unrelated to GitHub plugin MVP.
- No namespace collisions with official `github.*` MCP tools.
- Lease metadata must remain human-readable in GitHub.
- Default sync mode for issue-backed tasks is explicit and documented.
- Apply persona quality gates from `PERSONA-QUALITY-GATES.md` with alpha pragmatism (no heavyweight platform process).
- Keep default implementation shape minimal: runtime handler -> `GhCliAdapter` -> existing core services.
- No always-on reconcile worker in V1; reconcile runs from explicit actions or lightweight scheduled entrypoints.
- GitHub command execution must use validated argv invocation, never shell interpolation.
- All added tests must satisfy the Test Value Gate: non-tautological, user-facing, and minimal.

## Exit Criteria
- Feature-complete MVP with passing focused, non-tautological tests.
- Docs for setup, limits, and expected behavior.
- Docs include lease policy and AUTO/PAIR sync policy.
- Docs include terminal compatibility notes and known quirks.
- Ready for alpha user feedback loop.
