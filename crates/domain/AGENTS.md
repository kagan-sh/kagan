# domain: rules for changes here

Read the root `AGENTS.md` first. These rules add to it.

- No GPUI here, not even through a dependency. Check: `scripts/crate-boundaries.sh` fails when a GPUI crate is reachable from `kg-domain`.
- Ids, records, statuses, `Op`, `OpResult`, `SetupFile`, the theme file format and the saved preference keys are defined here once. Other crates use them and never define a second copy. Check: review.
- Every user-visible feature is an `Op`, dispatched by `Workspace::dispatch`. A changed `Op` also updates `docs/behaviour.md`. Check: review.
- Validation of an `Op`'s inputs lives in `check`. Check: review.
