# Contributing

Thanks for helping improve Kagan — an OpenCode plugin that runs your AI coding tasks as a
kanban board, one git worktree per task.

## What you need

- [OpenCode](https://opencode.ai/) installed.
- [Bun](https://bun.sh/) installed.

## Get set up

```sh
bun install      # dependencies
bun run setup     # one-time: turns on the git hooks that check your work before each commit
```

## Try your changes locally

```sh
bun run plugin:install
```

This installs your local build into OpenCode. Open a project in OpenCode and launch the board to
see your changes. Local/file installs, including `plugin:install:prod`, are never updated
automatically; only a global npm install exercises the update path. Run `bun run plugin:reset` to
undo the install.

## Curated validation

```sh
bun run verify
```

This runs the declared source checks: complexity, comment policy, and circular dependencies, and
auto-formats with oxfmt. Verifyx also runs the local `test` script automatically.

The maintainability-index gate is two-tier, matching the architecture split: pure logic
(`src/**/*.ts` excluding `src/tui/**` and `src/tui.tsx`) must clear maintainability index 52; the TUI
surface (`src/tui/**/*.{ts,tsx}`, `src/tui.tsx`) clears 50 because its JSX render functions are
inherently lower-scoring. Simplify the failing function first; split only when responsibilities
genuinely diverge. Merging or unsplitting is valid when cohesion beats MI churn from artificial file
boundaries. Never game the metric by deleting comments, joining lines, or fragmenting a coherent
function. The exact command is pinned by `test/guards/validation.test.ts`.

Pre-commit and CI run `bun run check`. That full check-only gate runs every built-in `verifyx` check,
including its automatic test step, then validates the package. It does not rewrite files.

Use this helper when running tests directly:

- `bun run test` — just the tests (use this, not a bare `bun test`).

## Commits and pull requests

- Work on a branch, not `main`. Open a pull request when you're ready.
- Write [conventional commit](https://www.conventionalcommits.org/) messages — releases are cut
  automatically from them when a change lands on `main`:
  - `fix:` — a bug fix.
  - `feat:` — a new feature (may include a breaking change while we're pre-1.0).
  - `docs:`, `test:`, `chore:`, `ci:`, `build:`, `style:`, `refactor:` — no release.

## Learn the code

- [`AGENTS.md`](AGENTS.md) — how the plugin is built and the rules to follow when changing it.
- [`.specs/README.md`](.specs/README.md) — how a task moves across the board, read this before
  changing that behavior.
- [docs.kagan.sh](https://docs.kagan.sh/) — user-facing docs.

By contributing you agree your work is licensed under the project's [MIT license](LICENSE).
