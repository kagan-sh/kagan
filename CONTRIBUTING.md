# Contributing

Clone the repository and [build from source](book/src/start.md#build-from-source).

Before a pull request:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo t --workspace --locked
```

On macOS, a UI change also runs `sh scripts/smoke.sh --keep`. Attach its snapshot and screenshot paths.

The [design documents](https://github.com/kagan-sh/kagan/tree/main/docs) are the spec. A change to what the app does updates `docs/behaviour.md`.

`AGENTS.md` is the rules file for a change in this repository, for a person or an agent. The spec wins where they disagree.
