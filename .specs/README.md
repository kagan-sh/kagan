# Specs

Behavior specs for Kagan, one folder per feature. `kagan-supervision-board/` covers the whole
plugin today.

## Files and authority order

1. [`requirements.md`](./kagan-supervision-board/requirements.md) — what must be true. EARS-style
   acceptance criteria with stable numbers (R1–R17); code and tests trace to these. When documents
   disagree, this one wins.
2. [`design.md`](./kagan-supervision-board/design.md) — how it's built: architecture constraints,
   the metadata model, and the key flows, each traced to the requirements it satisfies.
3. [`mental-model.md`](./kagan-supervision-board/mental-model.md) — why it exists: intent,
   anti-goals, and ideas already evaluated and rejected. Judge new feature proposals against this
   before writing requirements.

## Read order for newcomers

`mental-model.md` → the Map in [`AGENTS.md`](../AGENTS.md) → `src/domain/task/metadata.ts` → `src/server.ts`.

## Update rule

A behavior change lands with matching `requirements.md`, `design.md`, `docs/`, and README updates
in the same change — cite the requirement numbers (e.g. R9.16) it adds or alters. Implementation
task breakdowns are working artifacts: keep them in `.plans/` (gitignored), never committed here.
