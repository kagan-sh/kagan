# kagan: rules for every change

If AGENTS.local.md exists, read it too: a maintainer's local rules.

The design docs in `docs/` are the spec and win over this file: `CONTEXT.md` (terms), `architecture.md` (crate graph and runtime), `behaviour.md` (what each `Op` must do), `plugin-api.md`, and `look.md`. A change to what an `Op` does updates `docs/behaviour.md`; a non-obvious design decision goes in `docs/decisions.md`.

At every step, make the best decision that will elevate all three: the user experience (UX), developer experience (DX), and agent experience (AX), without breaking or degrading anything.

## Build and test

- Rust 1.98 is pinned. macOS links with `ld64.lld`, so run `brew install lld` first; Linux and Windows prerequisites are in `book/src/start.md`.
- `cargo run -p kg --release` builds and opens the app only when none is running; otherwise `kg` sends the command to the running app over a socket, so quit it before testing a new UI build. `KG_DATA_DIR=<temp>` gives an isolated headless app for CLI tests.
- `cargo t` is the alias for the pinned nextest (`cargo bin cargo-nextest run`; install `cargo-run-bin` once). Run `cargo t --workspace --locked`; one crate or test with `cargo t -p kg-core <filter>`.
- Before a pull request, run: `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo t --workspace --locked`.
- For a UI change, run `sh scripts/smoke.sh --keep` on macOS before a pull request, and attach its snapshot and screenshot paths to the report.
- `book/src/cli.md` is generated from clap help. After changing the CLI, run `KG_UPDATE_CLI=1 cargo t -p kg cli_reference_matches_help`; the test `cli_reference_matches_help` fails otherwise.

## Crates

- `kg` is the binary (CLI, app start, socket, MCP); `kg-ui` the GPUI window; `kg-plugins` the plugin host; `kg-core` the `Workspace`, entities, and thread lifecycle; `kg-store` SQLite; `kg-domain` ids, records, and `Op`; `kg-terminal` the terminal; `kg-process` the process tree.
- Only `kg-core` may have a normal dependency on `kg-store`; `kg-domain` must not reach GPUI. `scripts/crate-boundaries.sh` enforces the exact graph in `docs/architecture.md`.
- Every user-visible feature is an `Op` dispatched by `Workspace::dispatch`; the CLI is derived from `Op`.
- SQLite migrations are the `MIGRATIONS` array in `crates/store/src/store.rs`: append a new entry and never edit an applied one; `user_version` is the count applied.

## Skills

Agent skills on these topics help, and are optional for contributors:

- GPUI Kit coding guides: state ownership, entities, public API, naming and tests.
- GPUI Kit design guides: for any UI.
- Idiomatic Rust: ownership, errors, traits and concurrency.

The GPUI Kit coding and design skills are in the `skills/` folder of [gpui-kit](https://github.com/aorumbayev/gpui-kit).

`.agents/skills/kagan-fork-sync` merges upstream into the kagan-sh forks that `Cargo.toml` pins by rev (gpui-kit, gpui-fast, wry), re-pins kagan, and reports which fork changes upstream has taken.

Where a skill disagrees with the spec, the spec wins. Known overrides:

- No tokio in kagan's own code. Async runs on GPUI's executors (`cx.spawn`, `cx.background_spawn`); see decision 90.
- Shared UI state lives in GPUI entities (`Entity<T>`, `cx.notify()`, events), not `Arc<Mutex<T>>`.

## Laconic

- Write the smallest code that does the ticket. No speculative options, no trait with one implementor, no builder for three fields, no wrapper that only forwards.
- Delete what you replace. No shims, no dead code, no `#[allow(dead_code)]`.
- No comments except where the code would surprise a reader. No doc comments on private items.
- Names come from `docs/CONTEXT.md`. One word per concept. `scripts/glossary-check.sh` rejects the banned synonyms: Session/Chat → Thread, Backend → Provider, Sandbox → Environment, Scratch → Personal project, Add-on → Plugin.
- Before adding to the host or UI core, ask: Does kagan itself act on it, beyond how it looks? Can only the host see it? Can a plugin not do it correctly even with care? If all three answers are no, it belongs in a plugin, even if that means some repetition.

## Idiomatic Rust

- Errors: `Result` and `?`; `thiserror` in library crates, `anyhow` only in the `kg` binary. No `unwrap`/`expect` outside tests unless the invariant is stated in the message.
- States are enums, matched exhaustively. Parse at the boundary, pass typed values inside.
- Borrow before you clone; iterators before index loops; `impl Trait` in argument position for single-use generics.
- Visibility is the smallest that compiles: `pub(crate)` by default, `pub` only for the crate's interface in `docs/architecture.md`.

## GPUI

- Views implement `Render`; stateless pieces implement `RenderOnce`. No I/O and no heavy work in `render`.
- Retained state (inputs, lists, subscriptions, tasks) is owned by an entity and kept alive by a field; drop a `Task` or `Subscription` only to cancel it.
- Commands are GPUI actions with keybindings, and every user-facing one is also an `Op`.
- `ElementId`s are stable across renders.
- Colors and spacing come from the Workbench theme tokens (`docs/look.md`), never literals in views; `scripts/view-colors.sh` enforces it outside `crates/ui/src/theme.rs`.
- UI behavior is tested with GPUI's test context through the view's interface.

## Attribution

Commit messages and pull requests carry the change, not the tooling. Never add a `Co-authored-by`, `Generated with`, `Assisted-by`, `Created-by`, or `Made-with` line that names an AI tool, and never bypass `.githooks/commit-msg`. The hook is only a backstop.

A fresh clone turns it on with `git config core.hooksPath .githooks`.
`.githooks/pre-commit` runs only `cargo fmt --check`, so a commit takes about a second. `.githooks/pre-push-test` fails if it gains any other cargo command.
`.githooks/pre-push` runs checks that do not compile: attribution, typos, the glossary, the size budget, crate boundaries, view colors, and `cargo fmt --check`. It refuses a dirty tree or a HEAD other than the one pushed. Fetch `origin/main` first. Install the pinned `typos` (`scripts/typos-version`).

## Agent loop

1. Commit each working step, and push.
2. Read the CI run: `gh run list --branch <branch>`, then `gh run view <id> --log-failed`.
3. Fix what failed, and push again.

Push with the hooks on, so `.githooks/pre-push` runs. Mutation testing (`scripts/mutants-diff.sh`) is slow: it runs only when CI is started by hand with `mutants`.

## Enforced

- `.github/workflows/ci.yml` runs on every pull request and every push to `main` (`docs/decisions.md` 30): the hook checks, typos, the glossary, jscpd, the size budget, `cargo fmt --check`, cargo-deny, cargo-shear and crate boundaries; `cargo clippy --workspace --all-targets -- -D warnings` on Linux and macOS; and the tests on Linux, macOS and Windows. A diff with no Rust input skips the Rust jobs; `scripts/ci-changes.sh` lists the paths that skip, and any other path runs them.
- Merges into `main` are squashed, and the squash commit takes the PR title, so `.github/workflows/pr-title.yml` fails any title that is not a Conventional Commit (`type(scope)?: summary`).
- Workspace lints in the root `Cargo.toml` (`[workspace.lints]`) deny `unwrap_used`, `expect_used` (tests excepted), `dbg_macro`, `todo`, `print_stdout` outside `kg`, and warn on `clippy::pedantic`.
- Every pull request into `main` is reviewed against this file, and anything that breaks it is sent back.

Product Rust — `.rs` files under `crates`, excluding `tests.rs`, `*_test.rs`, and anything in a `tests/` directory — is measured in physical lines (`wc -l`). `.config/size-budget.txt` records `target`, `ceiling`, `allow_expect`, and a ceiling for every product file already over 600 lines. `scripts/size-budget.sh` (run by CI's `hygiene` job and the pre-push hook) fails when the total, the marker count, or a listed file is above its ceiling, and when an unlisted file is over 600 lines. It does not track exact counts, so a PR under the ceilings does not edit the file; lowering a ceiling is a deliberate ratchet. The end target is 35,000 lines.
