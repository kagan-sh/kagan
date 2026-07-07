# Isolation

## One worktree per task

Every board task runs in its own git worktree, created before the session exists:

- **Branch:** `kagan/<slug>`, branched from the base branch you selected in the create dialog.
- **Location:** `~/.kagan/worktrees/<repo-hash>/<slug>` — outside your checkout entirely.

The agent's edits, commits, and test runs happen there. Your working copy never changes until you
explicitly merge from the Done dialog. Because worktrees share the repository's git identity,
task sessions stay part of the same OpenCode project and appear on the board normally.

Kagan writes `.opencode/opencode.json` into each task worktree so the plugin also loads inside that
worktree-hosted session.

Kagan never deletes a worktree. To clean up after a merged task:

```bash
git worktree remove ~/.kagan/worktrees/<repo-hash>/<slug>
git branch -d kagan/<slug>
```

## No remote pushes from task sessions

Supervised sessions cannot push their `kagan/<slug>` branch to a remote. If the agent tries to run
`git push` — in any common shape, including forced pushes, directory flags, chained commands, or an
explicit remote and ref — Kagan denies the call and tells the agent that merging happens through
the board's Done dialog after review. Plain OpenCode chat sessions are unaffected.

## Board tasks vs. plain sessions

Only sessions created through the board's `n` dialog are supervised. A regular OpenCode chat —
including the session OpenCode opens on boot — gets no intake, no gates, no card. The two worlds
don't mix; to promote an idea from a chat into a supervised task, create a task for it.

## Helper sessions

Each task can own three kinds of child sessions, shown under its card:

| Child         | Role                                                        |
| ------------- | ----------------------------------------------------------- |
| `task prep`   | Read-only intake analysis (once per task).                  |
| `review`      | Read-only diff evaluation for the first review round.       |
| `review #N`   | Read-only diff evaluation for later review rounds.          |
| `iteration N` | A send-back work session continuing in the task's worktree. |

Intake and review helpers are read-only (no edit, write, or bash). Iteration children are work
sessions. All children are exempt from root-task lifecycle handlers so they cannot trigger their own
intake or review loops.
