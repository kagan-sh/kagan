# store: rules for changes here

Read the root `AGENTS.md` first. These rules add to it.

- Schema changes go in the `MIGRATIONS` array in `src/store.rs`. Append a new entry and never edit an applied one; `user_version` is the count applied. Check: review. `src/tests/schema.rs` runs the migrations, so a broken one fails there.
- Only `kg-core` may depend on this crate. Check: `scripts/crate-boundaries.sh`.
- Read columns by name, not by position. Status words are typed parameters, not hand-written literals. Check: review.
- Tests use an in-memory SQLite database. Check: review.
