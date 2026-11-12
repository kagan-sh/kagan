# domain: rules for changes here

Read the root `AGENTS.md` first. These rules add to it.

- No GPUI here, not even through a dependency. Check: `scripts/crate-boundaries.sh` fails when a GPUI crate is reachable from `kg-domain`.
- The types other crates share are defined here once. Other crates import them and never define a second copy. Check: review.
- Input checks for an `Op` live in `src/op/check.rs`. Check: review.
- `Op` enums derive clap's `Subcommand`, so their doc comments are `kg` help text. A changed doc comment or field means `book/src/cli.md` needs regenerating (see `crates/kg/AGENTS.md`). Check: `cli_reference_matches_help`.
