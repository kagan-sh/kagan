# ui: rules for changes here

Read the root `AGENTS.md` first. These rules add to it.

- Colors and spacing come from the Workbench theme tokens in `docs/look.md`, never literals in a view. Check: `scripts/view-colors.sh`, outside `src/theme.rs`.
- Views implement `Render`; stateless pieces implement `RenderOnce`. No I/O and no heavy work in `render`. Check: review.
- Shared UI state lives in GPUI entities (`Entity<T>`, `cx.notify()`, events), never `Arc<Mutex<T>>`. Check: `disallowed-types` in `crates/ui/clippy.toml`, which replaces the root `clippy.toml` for this crate, so a change to one changes both.
- Retained state (inputs, lists, subscriptions, tasks) is owned by an entity and kept alive by a field. Drop a `Task` or `Subscription` only to cancel it. Check: review.
- `ElementId`s stay the same across renders. Check: review.
- Every user-facing command is a GPUI action with a keybinding, and also an `Op`. Check: review.
- UI behavior is tested through GPUI's test context and the view's interface. Check: review.
- Tests wait with `wait_until` or `wait_for_shell` in `src/tests/kagan/support.rs`; never write a second wait loop. Check: review.
- Before a pull request for a UI change, run `sh scripts/smoke.sh --keep` on macOS and attach the snapshot and screenshot paths. Check: review.
