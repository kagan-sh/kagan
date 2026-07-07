# Quickstart

## Install

You need [OpenCode](https://opencode.ai/) installed.

From GitHub (until the package is on npm):

```bash
opencode plugin https://github.com/kagan-sh/kagan
```

Or add a local clone to your project's `opencode.json`:

```json
{
  "plugin": ["/path/to/kagan"]
}
```

Open the board with `/kagan`, the `kagan` palette command, or `<leader>k` (leader defaults to
`ctrl+x`). Run `/kagan-tutorial` anytime to replay the guided tour.

To configure options such as `checkCommand` or `inProgressLimit`, use the array-of-array plugin
entry shown in the [configuration reference](/reference/configuration).

## Your first task

1. **Open the board** — use `/kagan`, the `kagan` palette command, or `<leader>k`.
2. **Create a task** — press `n`. Enter a title and an optional description, pick a model, pick
   the base branch to work from. On submit, Kagan creates a `kagan/<slug>` worktree and the task
   lands in Backlog. If you configure a `setupCommand`, it runs once in the fresh worktree and the
   card shows `setup ok` or `setup failed`.
3. **Answer the intake** — a read-only `task prep` agent analyzes the task and codebase. Once it
   finishes and all required intake decisions are resolved, the card turns green with an
   `intake ok` label.
4. **Start it** — press `m` to move the card to In Progress. You'll be asked to confirm or
   override each of the intake's assumptions first. If the intake recommends `assisted` or
   `manual` mode, Kagan shows the rationale and asks whether to start the agent anyway; the
   recommendation is advisory and does not block the move. Then the agent starts with the refined
   instruction the intake produced.
5. **Review** — when the agent finishes, the card moves to Review automatically and a `review`
   agent evaluates the diff against the original task. Findings appear sorted by confidence. If
   you configure a `checkCommand`, it runs when the task enters Review and its result is shown as
   `check ok` or `check failed` and included in the reviewer's prompt.
6. **Triage and approve** — press `a`. For each finding choose **Ignore**, **Intended behavior**,
   or **Answer & clarify**. Once everything is triaged, choose where to merge the work — current
   branch, another branch, or no action — and the card moves to Done. If the base branch has
   advanced since the task was created, the merge dialog warns you that the reviewed diff may be
   stale.

Not satisfied? Press `s` instead to **send the task back**: a fresh agent session continues in the
same worktree with the previous iteration's report, the changed files, and your clarifications.
After enough send-backs (configurable with `sendBackStopThreshold`) Kagan asks whether to keep
iterating, take over the session, or leave it in Review.

## Board tips

- **Watch the indicators** — an In Progress card whose agent is busy shows `● working`; one that hit
  an error and is retrying shows `↻ retrying`. A card with neither is stalled.
- **Retry a helper** — if `task prep` or `review` fails, the card shows `intake failed` or
  `review failed`. Press `r` to retry manually; Kagan also retries automatically up to
  `helperRetries` times.
- **Filter and reorder** — press `/` to filter cards by title or slug. Type `#3` to jump straight
  to task 3. Press `J` / `K` to reorder the selected root card within its column.
- **Export and archive** — open a card's action menu with `Enter` to **export a trust packet**
  (a JSON snapshot of the task's evidence and rulings) or, for Done tasks, **archive** them so
  they leave the board. Archiving is one-way; the session remains reachable through OpenCode's
  own session list.

## Referencing earlier tasks

Cards show a `#N` number. Write `#3` in a new task's description and the agent receives task 3's
title, status, intake understanding, and final report as context.
