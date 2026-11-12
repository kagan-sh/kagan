# kg: rules for changes here

Read the root `AGENTS.md` first. These rules add to it.

- Most CLI commands are `Op`s, flattened into clap as `Command::Op`. Two others exist: `kg mcp` (`Command::Mcp`) and plugin commands (`Command::Plugin`). A new command that is not an `Op` needs a reason in review. Check: review.
- After changing the CLI, run `KG_UPDATE_CLI=1 cargo t -p kg cli_reference_matches_help` to regenerate `book/src/cli.md`. The test fails when the file is stale. Check: `cli_reference_matches_help`.
- `cargo run -p kg --release` builds and opens the app only when none is running. Otherwise `kg` sends the command to the running app over a socket, so quit it before you test a new UI build. Check: review.
- For an isolated app, set `KG_DATA_DIR` to a temp folder and unset `KG_SOCKET` (or set it to a name only you use). `socket::location` reads `KG_SOCKET` first, and every thread kagan starts has it set to the person's app, so `KG_DATA_DIR` alone sends your commands there. Add `--offscreen` for UI checks without a window, and `KG_NO_START=1` when a command must not start an app. Check: review.
- `crates/kg/tests/first_run.rs` runs the first-run commands in `book/src/start.md`, so an edit there can fail it. Check: `first_run.rs`.
