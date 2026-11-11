# Contributing

Clone the repository and [build from source](start.md).

Before a pull request, run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo t --workspace --locked
```

On macOS, a change to the window also runs `sh scripts/smoke.sh --keep`, which shows no window (offscreen). Add `--visible` to also check the window as the person sees it. Attach the snapshot path and the screenshot path.

The [design documents](https://github.com/kagan-sh/kagan/tree/main/docs) explain the code. When you change what kagan does, update `docs/behaviour.md`.

`AGENTS.md` is the rules file for a change in this repository. The file is for a person or an agent. When the rules and the code do not agree, the code and its tests are right, and the rule is the thing to fix.
