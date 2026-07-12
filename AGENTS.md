# AGENTS.md

Kagan is an OpenCode plugin that supervises coding tasks on a kanban board, with one git worktree
per task. Before changing lifecycle behavior, read [`.specs/README.md`](.specs/README.md) for the
spec authority and read order. `src/domain/task/metadata.ts` is the authoritative metadata schema.

## Commands

- Use Bun 1.3 or newer; CI pins 1.3.14. Run `bun install`, then `bun run setup` once after cloning to
  enable the `.githooks/pre-commit` hook.
- `bun run verify` is the agent-facing curated gate. It runs the declared source complexity, comment,
  and circular-dependency checks, auto-formats with oxfmt, plus the local `test` script.
- `bun run check` is the full check-only gate used by pre-commit and CI. It runs `verifyx all --check`,
  including its automatic test step, then package validation. CI checkout fetches full history so the
  new-comment check compares against the PR merge base.
- Run one built-in check with `bunx verifyx lint`, `bunx verifyx format`, `bunx verifyx check-types`, or
  `bunx verifyx duplicate-code`. Use `bun run package` for package checks.
- `verify:complexity` is two-tier: pure logic (`src/{domain,server,git,checks}`, `src/server.ts`,
  `src/task/`) must clear maintainability index 52; the TUI surface (`src/tui/`, `src/tui.tsx`) clears
  50 because its JSX render functions are inherently lower-scoring. Raise a score by splitting genuine
  responsibilities into cohesive units — never by deleting comments, joining lines, or fragmenting a
  coherent function. The exact command is pinned by `test/guards/validation.test.ts`.
- Run the full suite with `bun run test`, not bare `bun test`. The script supplies
  `--conditions browser`; `bunfig.toml` supplies the Solid preload. Bun positional test filters can
  also match a local gitignored `references/` checkout because its exclude applies only to test
  discovery.
- Focus a test with an anchored path and the browser condition, for example
  `bun test ./test/domain/task/policy.test.ts --conditions browser`.
- `bun run plugin:install` installs a raw development snapshot globally. Re-run it after source
  edits and restart OpenCode. `bun run plugin:install:prod` installs the packed production artifact;
  `bun run plugin:reset` removes Kagan from global OpenCode config.

## Build And Package

- `bun run build` recreates gitignored `dist/` from every `src/**/*.ts` and `src/**/*.tsx` file,
  preserving directories, compiling Solid JSX, changing relative imports to `.js`, and mapping
  host OpenTUI/Solid imports to runtime module IDs. Do not edit `dist/`.
- The published package contains compiled `dist/`, not `src/`. `bun run package` rebuilds,
  checks compiled Solid output and the exact tarball file list, installs the tarball in a clean
  consumer, rejects bundled OpenTUI/Solid copies, and imports both public exports.
- The development installer deliberately copies raw `src/` and the full local `node_modules` into
  `~/.kagan/plugin/kagan-pinned`; do not optimize that snapshot as if it were the published package.

## Code Map

- OpenCode entrypoints: `src/server.ts` for events, tools, and push protection; `src/tui.tsx` for the
  board and settings routes.
- `src/domain/` owns metadata parsing, policy, options, prompts, findings, and pure task logic.
- `src/task/` owns the shared worktree-first board task creation orchestrator.
- `src/server/` owns the v1 plugin lifecycle, intake/validator helpers, conversational task creation, and serialized server-side
  metadata patches.
- `src/tui/` owns the v2 TUI client, board store/components, dialogs, routes, and user actions.
- `src/git/` owns worktrees, canonical diffs, merges, and the git runner abstraction.
- `src/checks/runner.ts` runs configured setup and review commands.
- `test/guards/` enforces source boundaries, host-reactivity rules, and local dependency pins.

## Architecture Constraints

- The two plugin surfaces use different SDK call shapes. Server code uses the v1 client
  (`client.session.x({ path, body, query, throwOnError })`); TUI code uses the v2 client
  (`api.client.session.x(params, { throwOnError })`). For v1 fields missing from generated types,
  follow the existing `as Parameters<typeof ...>[0]` cast pattern.
- Domain code must not import `src/server/` or `src/tui/`; server and TUI implementations must not
  import each other. `test/guards/architecture.test.ts` enforces these boundaries.
- Durable task state lives in `session.metadata.kagan`. Read it through `kagan(metadata)`. When
  updating an existing session, use `patchKagan`, `claimHelperSpawn`, or `tuiPatchKagan`; they
  fresh-read, merge, and serialize writes so concurrent handlers do not replace sibling fields.
- Task creation order is fixed: create the worktree, register the plugin in that worktree, run
  eligible setup commands, then create the session with that directory. Session directories are
  immutable. Session listings must use `scope: "project"` or worktree sessions disappear.
- Root board cards have `boardTask: true`. Helper and worker children are supervised through `role`
  and parent back-pointers. Root transition handlers skip children, but helper error/idle handlers
  intentionally process them; use `isSupervisedSession` for the full rule.
- OpenCode session event payloads differ: `session.created` and `session.updated` use
  `properties.info`; `session.error` and `session.idle` use `properties.sessionID`.
- Use `worktreeDiffs` for review input, citations, changed-file checks, and send-back context. It
  includes committed and tracked changes from the merge base plus untracked files; session-summary
  diffs are not canonical.
- Configured command plans must continue after one command fails. Record skipped, failed, timed-out,
  or unspawnable steps and truncate output before writing it to metadata.
- Start every agent turn with `promptAsync`, never blocking `prompt`.
- Board feedback uses `store.notify`, not `api.ui.toast`, because host toasts do not render on plugin
  routes. Content passed to `api.ui.dialog.replace` must be bare; wrapping it in `api.ui.Dialog`
  creates a second overlay.
- Any source file importing `solid-js` or `@opentui/*` must use `.tsx`, even without JSX. The host's
  raw-source transform ignores `.ts`, which creates a separate Solid instance in development and a
  board that does not repaint. The architecture guard enforces this.
- Use `TuiPluginApi` for renderer dimensions, keyboard input, and keymap layers; do not import
  OpenTUI/Solid context hooks for those surfaces.
- Automatic updates are TUI-only. `src/tui/updates/check.ts` checks npm `latest` for bare/`@latest`
  installs, prepares compatible exact releases through `api.plugins.add`, promotes only on
  `api.lifecycle.onDispose`, and toasts only on home/session routes. Never add a server update hook
  or touch exact pins and file installs. `src/tui/updates/manager.ts` treats cache paths as hostile:
  it accepts only non-symlinked `@kagan-sh/kagan` wrappers and removes only its own marker and single
  backup; never broaden that deletion.

## External APIs

- Treat `package.json`, installed `@opencode-ai/*`, and the OpenCode support range in
  `engines.opencode` as current truth. Verify SDK/TUI behavior against those versions, not memory.
- When `references/opencode/` exists, it is a gitignored, read-only OpenCode source checkout for
  verifying host behavior. Never import from it or run its tests.

## Specs And Releases

- A behavior change must update `.specs/kagan-supervision-board/requirements.md`, `design.md`, user
  docs, and README in the same change, citing the requirement numbers changed.
- Pushes to `main` release through semantic-release. During `0.x`, `fix:` makes a patch and `feat:`
  makes a minor release, including breaking changes. `docs:`, `test:`, `chore:`, `ci:`, `build:`,
  `style:`, and `refactor:` do not release; reserve `BREAKING CHANGE:` for the move to 1.0.
- For explicit self-align, audit, or health-check requests, follow
  [`.claude/skills/self-align/SKILL.md`](.claude/skills/self-align/SKILL.md).
- Keep uncommitted plans and scratch notes in the gitignored `.plans/` directory.
