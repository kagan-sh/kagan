# GH-010 - Docs and Operator Runbook

Status: Todo
Owner: Codex
Depends On: GH-006, GH-007, GH-008, GH-009

## Outcome
Contributors and alpha users can set up and operate GitHub plugin V1 without guesswork.

## Scope
- Setup guide for gh auth and repo connect.
- Known limits (polling, no webhooks, rate-limit behavior).
- Lease/lock behavior and takeover policy.
- AUTO/PAIR sync mode policy and defaults.
- Troubleshooting and repair flow.
- Terminal compatibility notes for supported alpha baseline environments.
- Explicit alpha tradeoffs (what is intentionally not implemented yet).

## Acceptance Criteria
- Docs are concise, actionable, and match implemented behavior.
- Includes operator checklist for routine sync/reconcile usage.
- Includes persona quality-gate checklist results for this initiative.
- Includes a concise test-value rubric: only non-tautological, user-visible behavior tests are expected.

## Verification
- Manual docs walkthrough from clean environment.
