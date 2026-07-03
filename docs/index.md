# What is Kagan?

Kagan is an OpenCode plugin that turns agent work into supervised tasks on a kanban board. Each
task is an OpenCode session running in its own git worktree, and it moves through four columns —
**Backlog → In Progress → Review → Done** — with a gate at every transition.

The point is structural supervision, not vibes:

- **Before work starts**, a read-only intake agent analyzes the task and your codebase, then asks
  you to confirm or override its assumptions. The agent's understanding and yours are aligned
  before a single line changes.
- **While work runs**, it runs sandboxed in a worktree branched from the base you chose. Your
  checkout is never touched. A configurable cap bounds how many agents run at once.
- **After work finishes**, a reviewer agent evaluates the diff against the original task and
  intake, classifying findings as misalignment, bug, or uncertainty, ranked by confidence. You
  triage every finding before the task can be approved.
- **Merging is your call** — into the current branch, another branch, or not at all.

Kagan is optimized for **autonomous** work — tasks you can delegate and verify with a check you
trust more than the model. When a task is too risky, too novel, or too tangled to delegate safely,
the intake agent flags it and recommends you drive it yourself instead. See
[Choosing a mode](/concepts/choosing-a-mode).

Plain OpenCode chat sessions are untouched: no board card, no intake, no gating.

Start with the [Quickstart](/quickstart). The lifecycle in full detail:
[Task lifecycle](/concepts/task-lifecycle).
