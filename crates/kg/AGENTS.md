# kg: rules for changes here

Read the root `AGENTS.md` first. These rules add to it.

- The CLI is derived from `Op` with clap. Do not add a command that is not an `Op`. Check: review.
- After changing the CLI, run `KG_UPDATE_CLI=1 cargo t -p kg cli_reference_matches_help` to regenerate `book/src/cli.md`. The test fails when the file is stale. Check: `cli_reference_matches_help`.
- `cargo run -p kg --release` builds and opens the app only when none is running. Otherwise `kg` sends the command to the running app over a socket, so quit it before you test a new UI build. Check: review.
- Use `KG_DATA_DIR=<temp>` for an isolated headless app in CLI tests. Check: review.
