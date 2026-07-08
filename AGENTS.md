# AGENTS.md

Kagan: an OpenCode plugin that supervises AI coding tasks as a kanban board, one isolated git
worktree per task. Before changing lifecycle behavior, read [`.specs/README.md`](.specs/README.md)
— it indexes the behavior specs, their authority order, and the newcomer read order.
`src/task.ts` is the authoritative metadata schema.

## Commands

- `bun run check` — the merge gate (prettier + oxlint + tsc + tests). CI runs exactly this.
- `bun run test` — full suite (~1s; there is rarely a reason to run less).
- NEVER run bare `bun test` or `bun test <file>`: positional args are substring path filters that
  also match the vendored `references/` tree (bunfig's exclude only applies to discovery mode), and
  the suite needs `--conditions browser` (Solid/OpenTUI resolve their browser builds; tests fail
  confusingly without it). If you must focus, anchor the path and pass the flag:
  `bun test ./test/task.test.ts --conditions browser`.
- `bun run format:fix` before finishing any edit batch.
- `bun run setup` — one-time after cloning: points git at `.githooks` (pre-commit runs
  format/lint/typecheck). Not a `prepare`/`postinstall` lifecycle script on purpose: those make
  npm/pacote treat the repo as a git dep needing a build step, which breaks
  `opencode plugin https://github.com/kagan-sh/kagan`.

## Release conventions

Releases are automated with `semantic-release` on every push to `main`. Commits must follow
conventional commits. During the `0.x` phase:

- `fix:` — patch release (`0.1.0 → 0.1.1`)
- `feat:` — minor release (`0.1.0 → 0.2.0`), may include breaking changes
- `BREAKING CHANGE:` — major release (`0.2.0 → 1.0.0`); only use when ready to leave `0.x`

Commits that do **not** trigger a release: `docs:`, `test:`, `chore:`, `ci:`, `build:`, `style:`,
`refactor:`.

While in `0.x`, express breaking changes as `feat:` so they stay in the `0.x` line. Do not customize
`@semantic-release/commit-analyzer`. Reserve `BREAKING CHANGE:` for the `1.0.0` transition.

This rule will be updated after 1.0.0 release for now assume we are in 0.x alpha stages.

## Ground truth for external APIs

- `references/opencode/` is a vendored, gitignored, read-only checkout of the OpenCode source.
  Verify any claim about SDK/TUI/plugin behavior there and against the installed
  `@opencode-ai/*@1.17.13` in node_modules — never from memory. Never import from `references/`
  and never run its tests.
- Dependency pins are exact (plugin 1.17.13, opentui 0.4.2, solid 1.9.13). `.opencode/package.json`
  must keep the same plugin version as the root manifest.

## Map

- Entry points loaded by OpenCode: `src/server.ts` (events, gates, tools, push guard), `src/tui.tsx`
  (board route).
- UI: `board.tsx`, `column.tsx`, `card.tsx`, `commands.tsx`, `create-task.tsx`,
  `findings-review.tsx`, `trust-packet.tsx`, `onboarding.tsx`, `store.ts`, `format.ts`.
- Helper spawns: `intake.ts`, `validator.ts`.
- Domain: `task.ts` (metadata schema, parsed view, gates), `handoff.ts` (prompts), `types.ts`.
- IO: `session-api.ts` (session CRUD, serialized metadata patching, merge/send-back), `git.ts`
  (worktrees, diffs, merge), `check.ts` (setup/check commands).

## Architecture facts that are easy to get wrong

- Two plugin surfaces, two SDK dialects: `src/server.ts` uses the v1 client
  (`client.session.x({ path, body, query, throwOnError })`); the TUI files use the v2 client
  (`api.client.session.x(params, { throwOnError })`). Don't mix the call shapes. The server accepts
  more body/query fields than the generated v1 types declare — follow the existing
  `as Parameters<typeof …>[0]` cast idiom.
- The server plugin loads once per directory instance and only sees events for its own directory;
  task sessions live in their worktrees, so a different plugin instance handles them. All task
  state therefore lives in `session.metadata.kagan` (schema in `design.md`), read via `task.ts`'s
  single parsed view — `kagan(metadata).field` — and written only through `patchKagan` /
  `tuiPatchKagan` (they merge; never replace the whole `kagan` object).
- Order matters at creation: the worktree must exist before `session.create` (session directories
  are immutable). Board listing must pass `scope: "project"` or worktree-hosted sessions vanish.
- Only sessions with the `boardTask` marker are supervised or shown on the board; generic OpenCode
  sessions must remain untouched. Helper/iteration children carry `role` + a `*Parent` back-pointer
  — every lifecycle handler must skip role/parented sessions first (recursion guard).
- Board UI feedback goes through the store's `Notice` overlay (`store.notify`), never
  `api.ui.toast` (toasts don't render while the board route is active). Elements passed to
  `api.ui.dialog.replace` render bare content — the host stack already wraps them in a centered
  overlay; adding `api.ui.Dialog` around them double-wraps and breaks layout.
- All agent-starting prompts use `promptAsync`, never the blocking `prompt`.
- Board reactivity depends on file extension. The host's OpenTUI Solid transform (registered in
  `references/opencode/packages/opencode/src/plugin/tui/runtime.ts`) only rewires imports for
  `.jsx`/`.tsx` files — its Bun `onLoad` filter excludes plain `.ts`. Any file importing `solid-js`
  or `@opentui/*` MUST be `.tsx`, even when it contains no JSX: a `.ts` file resolves those to the
  snapshot's own bundled Solid instance instead of the host's, so its signals update but never
  repaint the mounted board (remounting the route re-reads state once, which masks it). `store.tsx`
  holds the board's signals and is `.tsx` for exactly this reason — a `test/` gate fails if any
  `src/*.ts` imports those modules. Because every board file is bridged, the board is plain reactive
  JSX with no imperative repaint workarounds.
- `plugin:install` copies the full dev `node_modules` and never strips it. The bundled
  `solid-js`/`@opentui/*` are inert — every `.tsx` import rewrites to the host's live instances
  before resolution — but they must be present or the plugin fails to load. `@opencode-ai` must stay
  too: `server.ts` value-imports `@opencode-ai/plugin/tool`, which the host does not bridge.
- The `.tsx` board is only reactive after the Solid transform runs; Bun's plain automatic JSX
  runtime evaluates each JSX expression once, so signals never repaint mounted content. The
  opencode host applies that transform itself when it imports raw plugin `.tsx` (it registers
  `@opentui/solid`'s runtime plugin support and rewires `solid-js`/`@opentui/solid` imports to its
  own live instances — `references/opencode/packages/opencode/src/plugin/tui/runtime.ts`). Ship the
  plugin as raw source; never pre-transform it. Pre-transformed output imports bare
  `@opentui/solid`/`solid-js`, which resolve to the snapshot's own node_modules and, under Bun's
  default conditions, to solid's inert server build — a board that renders once and ignores every
  refresh. Tests still need bunfig's `preload = ["@opentui/solid/preload"]` because `bun test` runs
  without the host.
- In plugin source, use `TuiPluginApi` for renderer dimensions, keyboard input, and keymap layers; never import OpenTUI/Solid context hooks for those surfaces.
- Behavior changes land with matching `.specs/`, `docs/`, and README updates in the same change.

## Style and discipline

- Strict YAGNI: build only what the task needs — no speculative options, abstraction layers, or
  fallback paths; when replacing behavior, delete the old path and its tests (no compat shims).
- Compactness without cleverness: no branch that re-checks what an earlier guard or schema already
  guarantees; no near-duplicate messages for indistinguishable cases.
- Mirror the established idioms instead of inventing new shapes: `task.ts`'s single `kagan()`
  parsed view for reads (never re-derive a field with ad hoc checks), chained
  `DialogSelect`/`DialogPrompt` flows in `commands.tsx`, the mock `PluginInput` / mock api styles
  already in `test/`.
- Prettier is config-as-law: no semicolons, printWidth 120. Comments only where the code alone
  would mislead (external constraint, deliberate deviation) — never narration.
- Test depth matches siblings: pure logic gets direct tests; thin glue is covered through its
  caller, not with dedicated suites.
- Uncommitted working notes go in `.plans/` (gitignored), nowhere else.
