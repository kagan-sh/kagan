# Troubleshooting

**A card says "Intake is still being prepared."**
The `task prep` child hasn't reported yet. Select the card and press `o` on the subtask to watch
it. If spawning failed outright, the task is still allowed to start, but the card shows
`intake failed` so the failure is visible.

**A card shows `intake failed` or `review failed`.**
The `task prep` or `review` helper exited with an error before it could finish. Kagan retries
automatically up to [`helperRetries`](/reference/configuration) times; after that the badge stays
until you press `r` on the card to retry the helper manually.

**A card shows `△ needs you`.**
The task is waiting for a permission input inside its session — for example, a command the agent
wants to run. Open the session with `o` and answer the prompt; the badge clears once the session
is no longer awaiting input.

**Moving to In Progress is denied with "Task has no isolated worktree."**
Only tasks created through the board's `n` dialog get a worktree and can be started. Plain
sessions can't be promoted — create a task instead.

**"In Progress WIP limit of N reached."**
The concurrency cap. Finish or send a task onward, or raise
[`inProgressLimit`](/reference/configuration).

**Approve says "Review hasn't finished" or "N finding(s) need triage."**
Approval requires the `review` child to have reported and every finding to have a disposition.
Press `a` again to walk the remaining findings.

**The merge failed.**
Kagan aborts the merge and nothing moves to Done. Resolve it manually: the task's branch is
`kagan/<slug>` and its worktree is under `~/.kagan/worktrees/` — merge it yourself, or send the
task back with instructions to rebase onto the current base.

**The agent's `git push` failed.**
Supervised sessions cannot push to a remote. Use the board's approve/merge flow, or take over the
worktree yourself if you need a different git flow.

**The footer says an update is ready.**
Kagan has prepared that release without changing the running plugin. Restart OpenCode once to
apply it.

**The footer says to update OpenCode for a Kagan release.**
That Kagan release does not support the running OpenCode version. The footer names the required
range; the current Kagan release remains active until OpenCode is compatible.

**The footer says updates unavailable.**
Automatic update cleanup or preparation failed — usually stale cache state after an interrupted
restart. Restart OpenCode once more; if the footer persists, remove the Kagan plugin cache under
OpenCode's cache directory and reinstall.

**My existing sessions don't appear on the board.**
By design — the board shows only tasks created through it. Chat sessions stay in OpenCode's
native session list.

**A `#N` reference rendered "(#N not found)."**
No board task currently carries that number. Check the card's `#N` prefix.
