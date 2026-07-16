# Troubleshooting

**A card says "Intake is still being prepared."**
The `task prep` child hasn't reported yet. Select the card and press `o` on the subtask to watch
it. If spawning failed outright, the task is still allowed to start, but the card shows
`intake failed` so the failure is visible.

**A card shows `intake failed` or `review failed`.**
The `task prep` or `review` helper exited with an error before it could finish. Kagan retries
automatically up to [`helperRetries`](/reference/configuration) times; after that the badge stays
until you press `r` on the card to restart the helper manually.

**You want to rerun intake or review after it already succeeded.**
Press `r` on the card (Backlog for intake, Review for review). Kagan clears the helper state first,
then aborts any live helper session, and respawns through the normal state-based spawn path — that
ordering is deliberate so the abort cannot trigger a second, duplicate helper. Restarting review also
clears findings, check evidence, and any approval stamp without bumping the task generation.

**A card shows `△ needs you`.**
The task is waiting for a permission input inside its session — for example, a command the agent
wants to run. Open the session with `o` and answer the prompt; the badge clears once the session
is no longer awaiting input. This only affects task and worker sessions: the read-only intake and
review helpers auto-approve their own permission requests, so a permission prompt can no longer
strand them mid-run.

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

**The footer says a version is available.**
A newer stable Kagan release exists. Press `u`, run `/kagan-update`, or use the command palette to
review and confirm it. Nothing changes until you confirm.

**The footer says a version is installed — restart OpenCode.**
Kagan installed the update but the running process still uses the old version. Restart OpenCode once
to load it.

**The update failed.**
Kagan keeps running the current version and leaves the update available to retry. This happens when
OpenCode cannot download the release or its `plugin --global --force` install exits with an error;
the notice shows what OpenCode reported. Automatic updates only apply to global npm installs — a
local or file install is never updated.

**My existing sessions don't appear on the board.**
By design — the board shows only tasks created through it. Chat sessions stay in OpenCode's
native session list.

**A `#N` reference rendered "(#N not found)."**
No board task currently carries that number. Check the card's `#N` prefix.
