# GH-010 - Docs and Operator Runbook

Status: Done
Owner: Codex
Completion: Implemented in `7326c988` on 2026-02-14.
Depends On: GH-006, GH-007, GH-008, GH-009

## Outcome
Contributors and alpha users can set up and operate GitHub plugin V1 without guesswork.

## Scope
- Setup guide for gh auth and repo connect.
- Known limits (polling, no webhooks, rate-limit behavior).
- Lease/lock behavior and takeover policy.
- AUTO/PAIR sync mode policy and defaults.
- Troubleshooting and mapping-drift recovery flow (via normal sync path).
- Terminal compatibility notes for supported alpha baseline environments.
- Explicit alpha tradeoffs (what is intentionally not implemented yet).

## Acceptance Criteria
- Docs are concise, actionable, and match implemented behavior.
- Includes operator checklist for routine sync/reconcile usage.
- Includes persona quality-gate checklist results for this initiative.
- Includes a concise test-value rubric: only non-tautological, user-visible behavior tests are expected.

## Verification
- Manual docs walkthrough from clean environment.
