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
see your changes. Local/file installs, including `plugin:install:prod`, intentionally skip automatic
update checks; only a published bare/`@latest` npm install exercises that path. Run
`bun run plugin:reset` to undo the install.

## Curated validation

```sh
bun run verify
```

This runs the declared source checks: complexity, comment policy, and circular dependencies, and
auto-formats with oxfmt. Verifyx also runs the local `test` script automatically.

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
