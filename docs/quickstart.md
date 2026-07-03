# Quickstart

## Install

You need [OpenCode](https://opencode.ai/) installed. Add the plugin to your project's
`opencode.json`:

```json
{
  "plugin": ["/path/to/kagan"]
}
```

## Your first task

1. **Open the board** — run `kagan` from the OpenCode command palette (or `/kagan`). Run
   `/kagan-tutorial` anytime to replay the guided tour.
2. **Create a task** — press `n`. Enter a title and an optional description, pick a model, pick
   the base branch to work from. On submit, Kagan creates a `kagan/<slug>` worktree and the task
   lands in Backlog.
3. **Answer the intake** — a read-only `task prep` agent analyzes the task and codebase. Once it
   finishes and all required intake decisions are resolved, the card turns green with an
   `intake ok` label.
4. **Start it** — press `m` to move the card to In Progress. You'll be asked to confirm or
   override each of the intake's assumptions first; then the agent starts with the refined
   instruction the intake produced.
5. **Review** — when the agent finishes, the card moves to Review automatically and a `review`
   agent evaluates the diff against the original task. Findings appear sorted by confidence.
6. **Triage and approve** — press `a`. For each finding choose **Ignore**, **Intended behavior**,
   or **Answer & clarify**. Once everything is triaged, choose where to merge the work — current
   branch, another branch, or no action — and the card moves to Done. If the base branch has
   advanced since the task was created, the merge dialog warns you that the reviewed diff may be
   stale.

Not satisfied? Press `s` instead to **send the task back**: a fresh agent session continues in the
same worktree with the previous iteration's report, the changed files, and your clarifications.

## Referencing earlier tasks

Cards show a `#N` number. Write `#3` in a new task's description and the agent receives task 3's
title, status, intake understanding, and final report as context.
