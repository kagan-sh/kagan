# Kagan

[Docs](https://docs.kagan.sh/) · [Quickstart](https://docs.kagan.sh/quickstart/)

---

Kagan is an OpenCode plugin that turns agent work into supervised tasks on a kanban board. Each task is an OpenCode session running in an isolated git worktree, moving through **Backlog → In Progress → Review → Done** with a gate at every transition — intake before the agent runs, review before you approve, merge only on your say-so.

The agent never touches your checkout. It works on a `kagan/<slug>` branch in its own worktree, a reviewer agent files ranked findings against the original task, and nothing reaches Done until you've triaged every finding and chosen where — or whether — to merge.

## Install

You need [OpenCode](https://opencode.ai/) installed.

From npm:

```bash
opencode plugin @kagan-sh/kagan
```

Or add a local clone to both OpenCode config files:

```json
{
  "plugin": ["/path/to/kagan"]
}
```

Open the board with `/kagan` from the command palette, the `kagan` palette command, or `<leader>k` (the leader key defaults to `ctrl+x`).

Pass options by using the array-of-array form, or open `/kagan-settings` from the project — see the [configuration reference](https://docs.kagan.sh/reference/configuration/).

## Docs

Full documentation lives in [`docs/`](https://docs.kagan.sh/).

## License

[MIT](LICENSE)

---
