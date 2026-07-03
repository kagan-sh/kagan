# Task lifecycle

Every task moves through four columns. Each transition is gated — a move that fails a gate is
denied with the reason, on the board and server-side.

| Column          | What happens there                                                            |
| --------------- | ----------------------------------------------------------------------------- |
| **Backlog**     | Intake runs; you resolve its questions; the card turns `intake ok`.           |
| **In Progress** | The agent works in the task's worktree. Capped concurrency.                   |
| **Review**      | A reviewer agent files findings; you triage them; approval unlocks the merge. |
| **Done**        | Work merged where you chose — or deliberately left in its worktree.           |

## Intake (Backlog)

Creating a task spawns a read-only **task prep** child session. It reads the codebase at your
chosen base branch and returns three things: an understanding of the task, clarifying decisions
(each an assumption you must approve or override with an answer), and a refined final instruction
for the implementing agent. A task cannot enter In Progress until every required decision is
resolved. If the intake helper fails to spawn, the task is treated as intake-ready — a broken
helper never strands work.

## Start (In Progress)

On first entry the session is prompted automatically with the refined instruction, your resolved
decisions, the intake understanding, and any `#N` referenced-task context — using the model you
picked at creation. The move is denied if the In Progress cap is full, the task has no worktree,
or intake is unresolved. Once the agent has started, the task can't move back to Backlog — the
agent may still be working, so that would strand it with nothing to resume it.

## Review

When the active iteration goes idle, the card moves to Review automatically and its final report
is captured as the task's report. A read-only **review** child session evaluates the worktree
diff (against the base branch) against the original title, description, and intake, and files
findings via a structured tool — each classified as `misalignment`, `bug`, or `uncertainty` with a
0–10 confidence score. Findings are presented most-confident first.

## Triage, send-back, or approve

Each finding gets one of three dispositions: **Ignore** (with a substantive reason), **Intended
behavior** (reason required for high severity), or **Answer & clarify** (your clarification feeds
the next iteration).

Substantive means a real explanation, not placeholders like `ok`, `lgtm`, or `n/a`; weak notes are
rejected before they are saved.

- **Send back** (`s`, or `b` to move the card backward) starts iteration N+1: a fresh session in
  the same worktree, prompted with the previous iteration's report, the files already changed, the
  findings to address, and the intended-behavior findings to leave alone. Review state resets; the
  card returns to In Progress.
- **Approve** (`a`) is unlocked once the reviewer has run and every finding is triaged. It opens
  the merge dialog: merge the task branch into the checked-out branch, into another branch, or
  take no action (right for investigative tasks). If the base branch has advanced since the task
  was created, the dialog warns that the reviewed diff may be stale. A failed merge aborts the
  approval — nothing moves to Done on an error. Once a task reaches Done it stays there — moving it
  back out is denied; create a follow-up task instead.
