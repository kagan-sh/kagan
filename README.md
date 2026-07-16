<h3 align="center">
  <img src="https://raw.githubusercontent.com/kagan-sh/kagan/main/docs/public/favicon.svg" width="80" alt="Kagan" /><br />
  kagan
</h3>

<p align="center">Supervised kanban for AI coding agents inside OpenCode.</p>

<p align="center">
  <a href="https://docs.kagan.sh/">Docs</a> · <a href="https://docs.kagan.sh/quickstart/">Quickstart</a>
</p>

---

Kagan is an OpenCode plugin that turns agent work into supervised tasks on a kanban board. Each task is an OpenCode session running in an isolated git worktree, moving through **Backlog → In Progress → Review → Done** with a gate at every transition — intake before the agent runs, review before you approve, merge only on your say-so.

The agent never touches your checkout. It works on a `kagan/<slug>` branch in its own worktree, a reviewer agent files ranked findings against the original task, and nothing reaches Done until you've triaged every finding and chosen where — or whether — to merge.

## Install

You need [OpenCode](https://opencode.ai/) **1.17.13** or newer (see `engines.opencode` in
`package.json`).

Install globally so Kagan is available in every project:

```bash
opencode plugin -g @kagan-sh/kagan
```

Or add a local clone to both OpenCode config files:

```json
{
  "plugin": ["/path/to/kagan"]
}
```

Open the board with `/kagan` from the command palette, the `kagan` palette command, or `<leader>k` (the leader key defaults to `ctrl+x`).

From any regular OpenCode session, run `/kagan-task` to create board tasks conversationally — useful when planning several tickets at once without opening the board create dialog for each one.

For global npm installs, Kagan checks npm for newer stable releases and shows an available version in
the board footer. Press `u`, run `/kagan-update`, or use the command palette to review and confirm the
update; Kagan then installs the exact release and asks you to restart OpenCode. Local, file, and
development installs are never updated automatically. See
[Updating](https://docs.kagan.sh/quickstart/#updating).

Pass options by using the array-of-array form, or open `/kagan-settings` from the project — see the [configuration reference](https://docs.kagan.sh/reference/configuration/).

## Docs

Documentation is available in [`docs/`](https://docs.kagan.sh/).

## License

[MIT](LICENSE)
