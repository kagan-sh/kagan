<p align="center">
  <a href="https://opensource.org/license/MIT"><img src="https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge" alt="License: MIT"></a>
</p>

<h3 align="center">
  <a href="docs/index.md">Docs</a> ·
  <a href="docs/quickstart.md">Quickstart</a> ·
  <a href="docs/concepts/task-lifecycle.md">Task lifecycle</a> ·
  <a href="docs/reference/configuration.md">Configuration</a>
</h3>

---

Kagan is an OpenCode plugin that turns agent work into supervised tasks on a kanban board. Each task is an OpenCode session running in an isolated git worktree, moving through **Backlog → In Progress → Review → Done** with a gate at every transition — intake before the agent runs, review before you approve, merge only on your say-so.

The agent never touches your checkout. It works on a `kagan/<slug>` branch in its own worktree, a reviewer agent files ranked findings against the original task, and nothing reaches Done until you've triaged every finding and chosen where — or whether — to merge.

## Install

You need [OpenCode](https://opencode.ai/) installed. Add the plugin to your project's `opencode.json`:

```json
{
  "plugin": ["/path/to/kagan"]
}
```

Then open the board from the OpenCode command palette by typing `/kagan`.

## Docs

Full documentation lives in [`docs/`](docs/index.md) — a [VitePress](https://vitepress.dev) site:

```bash
bun run docs:dev
```

The authoritative behavior specs live in [`.specs/`](.specs/README.md) — behavior changes must land with matching spec updates.

## License

MIT
