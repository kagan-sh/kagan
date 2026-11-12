# kagan: rules for every change

If AGENTS.local.md exists, read it too: a maintainer's local rules.

## Order of truth

When two sources disagree, the one higher on this list is right:

1. **The code and its tests.** What an `Op` does, the crate graph, and what the window shows. A check runs against these.
2. **The rules in this file.** A rule stays only if a check enforces it, or it names the review that enforces it (see Enforced).
3. **The docs in `docs/`.** They explain the code and why it is shaped that way. A doc that disagrees with the code is a doc bug, unless the code is the bug.

A change to what an `Op` does updates `docs/behaviour.md`. A non-obvious design choice gets an entry in `docs/decisions.md`; an entry records why, and the code holds the rule. Every sentence you add there says what the code does now, not what it was meant to do. Check: review.

At every step, make the best decision that will elevate all three: the user experience (UX), developer experience (DX), and agent experience (AX), without breaking or degrading anything.

## Folder rules

Read a crate's own `AGENTS.md` before you change it. It adds to the rules here:

- `crates/store/AGENTS.md`: schema migrations.
- `crates/domain/AGENTS.md`: no GPUI, and the shared types and `Op`.
- `crates/ui/AGENTS.md`: GPUI rules, theme tokens, the smoke test.
- `crates/plugins/AGENTS.md`: powers and approval.
- `crates/kg/AGENTS.md`: the CLI, the `cli.md` regeneration, and an isolated app.

Keep `CLAUDE.md` and `CLAUDE.local.md` out of this repo: Claude Code reads `AGENTS.md` only when neither exists. opencode finds the crate files through `opencode.json`. Codex and Gemini CLI see a crate file only when started in that crate, so this list is their pointer.

## Build and test

- Rust 1.99 is pinned. macOS links with `ld64.lld`, so run `brew install lld` first; Linux and Windows prerequisites are in `book/src/start.md`.
- `cargo t` is the alias for the pinned nextest (`cargo bin cargo-nextest run`; install `cargo-run-bin` once). Run `cargo t --workspace --locked`; one crate or test with `cargo t -p kg-core <filter>`. Plain `cargo test` is not the gate: it runs a crate's tests in one process, so a `static` set by one test leaks into the next. A test that only fails under `cargo test` is not a bug to fix.
- Before a pull request, run: `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo t --workspace --locked`.
- CI denies every warning, so a pedantic lint, `unreachable_pub`, or `too_many_lines` (over 100 lines, `clippy.toml`) fails the build. Split the function; an `#[allow]` or `#[expect]` needs a `reason` and counts against `allow_expect` in the size budget.
- jscpd (copy detection) runs in CI, not in the hooks: `npx --yes jscpd@5.4.0 --config .config/jscpd.json`.

## Windows

- `cfg(windows)` code builds only in the Windows CI jobs (`windows-clippy`, decision 253, and `windows-tests`). A clean macOS run proves nothing about it; wait for those jobs before you call a change done.
- `#[cfg(...)]` covers the one item below it. When you delete a gated line, check what the attribute now covers: a leftover gate on a `use` passes on macOS and Linux and fails Windows with an unresolved name.
- A test that starts a shell is `#[cfg(unix)]`. Keep the rest of the logic in a test without a gate, so Windows runs it.
- A Windows-only failure has a cause. Find it; do not change behaviour just to make the test pass.

## Crates

- `kg` is the binary (CLI, app start, socket, MCP); `kg-ui` the GPUI window; `kg-plugins` the plugin host; `kg-core` the `Workspace`, entities, and thread lifecycle; `kg-store` SQLite; `kg-domain` ids, records, and `Op`; `kg-terminal` the terminal; `kg-process` the process tree.
- Every user-visible feature is an `Op` dispatched by `Workspace::dispatch`, and a changed `Op` updates `docs/behaviour.md`. Check: review.
- `scripts/crate-boundaries.sh` enforces the graph in its `EXPECTED` table. The diagram in `docs/architecture.md` is a picture of that table, and review checks that they match.

The GPUI Kit coding and design skills are in the `skills/` folder of [gpui-kit](https://github.com/aorumbayev/gpui-kit). Where a skill disagrees with this file, this file wins. Known override: no tokio in kagan's own code. Async runs on GPUI's executors (`cx.spawn`, `cx.background_spawn`); see decision 90. `.agents/skills/kagan-fork-sync` merges upstream into the forks `Cargo.toml` pins by rev (gpui-kit, gpui-fast, wry).

## Laconic

- Write the smallest code that does the ticket. No speculative options, no trait with one implementor, no builder for three fields, no wrapper that only forwards.
- Before you write a helper, a check, or a start path, search for the one that exists and call it. Review sends back a second copy, even a short one.
- Delete what you replace. No shims, no dead code, no `#[allow(dead_code)]`.
- No comments except where the code would surprise a reader. No doc comments on private items.
- Names come from `docs/CONTEXT.md`. One word per concept. `scripts/glossary-check.sh` rejects the banned synonyms: Session/Chat → Thread, Backend → Provider, Sandbox → Environment, Scratch → Personal project, Add-on → Plugin.
- Before adding to the host or UI core, ask: Does kagan itself act on it, beyond how it looks? Can only the host see it? Can a plugin not do it correctly even with care? If all three answers are no, it belongs in a plugin, even if that means some repetition.

## Idiomatic Rust

- Errors: `Result` and `?`; `thiserror` in library crates, `anyhow` only in the `kg` binary. No `unwrap`/`expect` outside tests unless the invariant is stated in the message.
- States are enums, matched exhaustively. Parse at the boundary, pass typed values inside.
- Borrow before you clone; iterators before index loops; `impl Trait` in argument position for single-use generics.
- Visibility is the smallest that compiles: `pub(crate)` by default. The workspace lint `unreachable_pub` (in the root `Cargo.toml`) warns on a `pub` item no other crate can reach.

## Fixes and tests

Each rule came from a real failure. A rule with a script names it; the others are checked in review.

- A fix names its cause. A change that only makes the symptom go away is sent back. If you cannot reproduce a bug, say so and change no code for it. Check: review.
- A `fix:` commit carries a regression test that fails on the code before the fix. Prove it: undo the fix by hand, see the test fail, then restore the file. Check: review.
- A test must be able to fail when its behaviour breaks. Delete a test that repeats the implementation or cannot fail. Check: review, with the surviving mutants from `scripts/mutants-diff.sh` as evidence.
- A test reads a `KG_*` or `KAGAN_*` variable only when it sets that variable itself: a child process it starts, or a value it saves and restores. Check: `scripts/test-env-check.sh` finds a direct `var`/`var_os` read in test code. A read through clap `env =` or a product helper (`socket::location`, `data_dir_from_env`) is checked in review.
- A test matches an error by its typed kind, never by another crate's message text. Check: review.

## Decisions and the size budget

- `docs/decisions.md` records why. A replaced entry starts with "Superseded by N". Entry numbers only go up, with no gaps except the retired ones named in the script, and every "decision N" reference names a real entry. Check: `scripts/decisions-check.sh` checks the numbers and that each "Superseded by N" names a real entry; review checks that the words sit at the top.
- When `main` moves, your new entry's number can now belong to another entry, and `decisions-check.sh` still passes. After each merge, renumber your entries and every reference to them (`docs/`, code comments, the Cost sentence).
- Product Rust is the `.rs` files under `crates`, except `tests.rs`, `*_test.rs`, and anything in a `tests/` folder, counted with `wc -l`. `.config/size-budget.txt` holds `target`, `ceiling`, `allow_expect` (the count of `#[allow(` and `#[expect(` markers), and a ceiling for each file already over 600 lines. `scripts/size-budget.sh` fails above any ceiling, or on an unlisted file over 600 lines. The end target is 35,000 lines.
- A PR under the ceilings does not edit that file. A PR over it raises `ceiling` to its count and adds one sentence to its decision: `Cost: product Rust went from main's N to M (+K).` After each merge with `main`, run the script again and correct N, M and the ceiling. Lowering a ceiling is a deliberate ratchet.

## Attribution

Commit messages and pull requests carry the change, not the tooling. Never add a `Co-authored-by`, `Generated with`, `Assisted-by`, `Created-by`, or `Made-with` line that names an AI tool, and never bypass `.githooks/commit-msg`. The hook is only a backstop.

A fresh clone turns it on with `git config core.hooksPath .githooks`.
`.githooks/pre-commit` runs only `cargo fmt --check`, so a commit takes about a second. `.githooks/pre-push-test` fails if it gains any other cargo command.
`.githooks/pre-push` runs checks that do not compile: attribution, typos, the glossary, decision numbers, test env reads, the size budget, crate boundaries, view colors, and `cargo fmt --check`. A diff that `scripts/ci-changes.sh` calls docs-only skips the last four. It refuses a dirty tree or a HEAD other than the one pushed. Fetch `origin/main` first. Install the pinned `typos` (`scripts/typos-version`).

## Agent loop

1. Commit each working step, and push. Add commits; do not amend or force-push a pushed branch, because the merge squashes them.
2. Read the CI run: `gh run list --branch <branch>`, then `gh run view <id> --log-failed`.
3. Fix what failed, and push again.

Push with the hooks on, so `.githooks/pre-push` runs. Mutation testing (`scripts/mutants-diff.sh`) is slow: it runs only when CI is started by hand with `mutants`.

## Enforced

- `.github/workflows/ci.yml` runs on every pull request and every push to `main` (decision 30): the hook checks, jscpd, cargo-deny, cargo-shear; `cargo clippy --workspace --all-targets -- -D warnings` on Linux, macOS and Windows (decision 253); and the tests on Linux, macOS and Windows. A diff with no Rust input skips the Rust jobs; `scripts/ci-changes.sh` lists the paths that skip. Some docs that tests or checks read do not skip, and neither does any `crates/*/AGENTS.md`.
- Merges into `main` are squashed, and the squash commit takes the PR title, so `.github/workflows/pr-title.yml` fails any title that is not a Conventional Commit (`type(scope)?: summary`).
- Workspace lints in the root `Cargo.toml` (`[workspace.lints]`) deny `unwrap_used`, `expect_used` (tests excepted), `dbg_macro`, `todo`, and `print_stdout` in every crate (`kg` prints through `out.rs`), and warn on `clippy::pedantic`.
- Every pull request into `main` is reviewed against this file and the folder files, and anything that breaks them is sent back.
