# Design — Kagan Supervision Board

Technical design backing [requirements.md](./requirements.md). Traces each subsystem to the
requirements it satisfies (R1–R18).

## Overview

Kagan ships as two OpenCode plugin surfaces from one package:

- **Server plugin** (`src/server.ts`, default export `{ id, server }`) — runs inside each directory
  instance. Owns the task lifecycle: intake spawn, column-move gating, auto-start, review entry,
  the `kagan_intake` / `kagan_findings` tools, and cross-task reference resolution. Holds no durable
  state of its own; everything authoritative lives in session metadata.
- **TUI plugin** (`src/tui.tsx`, default export `{ id, tui }`) — renders the board route, the
  create-task dialog, and the triage/merge dialogs; owns user-initiated actions (create, move,
  triage, approve, send-back) via a Solid store and owns automatic update checking, preparation,
  promotion, and feedback.

Both receive the plugin `options` object (OpenCode config). The option reference in
[`docs/reference/configuration.md`](../../docs/reference/configuration.md) is canonical.

## Architecture constraints (verified against @opencode-ai/{plugin,sdk} 1.17.18)

These constraints shape the design and must hold for it to be correct:

- **Session directory is immutable.** It is set at creation from the routed instance. To run a
  session in a worktree the worktree must exist first and be passed as the create `directory`.
  → the TUI create flow builds the worktree before `session.create` (R1.6, R2).
- **Git worktrees resolve to the same OpenCode project** (identity = remote / common-dir /
  root-commit). Sessions inside them are project-scoped, so the board lists with `scope: "project"`;
  without it, worktree-hosted sessions are invisible (R3).
- **Server plugins load per directory instance**; the `event` hook only receives events whose
  `location.directory` equals that instance's directory. A task session in worktree W is served by
  the instance rooted at W. Therefore all task state lives in session metadata (global DB): the
  column-gating baseline is `lastGatedStatus` in `kagan` metadata, not an in-memory map, so a plugin
  restart never loses it — the gate re-derives `prev` from metadata on the first event it sees for a
  task, the same as any other run (R15.2).
- **Event payloads:** `session.created|updated|deleted` carry `properties.info` (read id off
  `info.id`); `session.idle` carries `properties.sessionID`.
- **`promptAsync`** starts an agent turn and returns immediately; used for every agent-starting
  prompt (intake, validator, auto-start, send-back).
- **Toasts are invisible while the board route is active**, so task and board-action feedback uses
  the board's own `Notice` overlay. Automatic update feedback is the deliberate exception: one host
  toast on home/session routes plus persistent board footer state, never the Notice queue (R3.8,
  R18.5).
- **Plugin activation ends at restart.** `api.plugins.add` can resolve, compatibility-check, import,
  and validate an exact package during the current TUI process, but server hooks already loaded by
  OpenCode cannot be replaced safely. Compatible npm updates therefore promote only during
  `api.lifecycle.onDispose`; restart loads the promoted wrapper (R18).
- **The host dialog stack already centers each dialog element** in a full-screen overlay, so the
  create-task dialog renders bare content and calls `dialog.setSize`, rather than wrapping itself in
  another overlay.

## Canonical diff source (R9.2, R11.2)

Board-task diffs come from git, not session summaries. `worktreeDiffs(runner, worktree, baseBranch)`
diffs the worktree (tracked changes and commits against the merge-base with the base branch, plus
untracked files) and returns a deterministically ordered list. The same function runs server-side
(over the Bun shell) and TUI-side (over `Bun.spawn`), so both derive identical results across
iterations. Session-summary diffs are not used: they cannot span iterations and the TUI's live diff
lacks patch text.

## Data model — `session.metadata.kagan`

`src/domain/task/metadata.ts` is the authoritative schema and parsed read view. It validates the `kagan` metadata
blob into `kagan(metadata).field`, salvages malformed optional fields where safe, and exposes the
derived gate functions. The main stored groups are: root task identity (`boardTask`, `taskNumber`,
`status`), task setup (`description`, `baseBranch`, `worktree`, `model`, `scope`, `setup`), lifecycle state
(`startedAt`, `activeIteration`, `generation`, `report`, `approved`, `lastGatedStatus`), child
session state (`role`, parent back-pointers, helper session ids/outcomes/attempts/errors), intake,
findings/prior triage, check evidence, and permission wait markers (`awaitingInput`).

Writes go only through `patchKagan` (server, v1 client) / `tuiPatchKagan` (TUI, v2 client). They do
a fresh read inside a per-session lock, merge into the existing `kagan` object, and serialize
same-session writes so concurrent event handlers cannot clobber each other.

## Components

| File                                              | Responsibility                                                                                                                                                     | Requirements                                         |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------- |
| `domain/task/`, `domain/options.ts`               | Metadata schema, parsed view, option readers, scoped command parsing/matching, gate functions, finding/intake rules, citation verification, and generation patches | R1.9–10, R2.3, R5, R7, R9.6, R9.17–19, R10, R12, R16 |
| `git/`                                            | Runner abstraction; worktree create/config; diff assembly; hunk-range parser; `git push` matcher; merge implementation                                             | R1.6, R2, R9.2, R9.16–17, R11.2, R12.3–10, R16       |
| `domain/handoff.ts`                               | Prompt composition and task-reference formatting                                                                                                                   | R6.2, R11.2, R13                                     |
| `server.ts`                                       | Event lifecycle; helper failure funnel; tools; permission-wait tracking; reference resolution; report capture; remote-push guard                                   | R4, R6, R7.3, R8, R9, R13, R15–17                    |
| `server/intake.ts`                                | `spawnIntake` read-only child                                                                                                                                      | R4                                                   |
| `server/validator/`                               | `spawnValidator` read-only child, diff+context prompt, validator model rotation                                                                                    | R9                                                   |
| `checks/runner.ts`                                | Setup/check command runner with timeout, skipped/ran step evidence, and output tail                                                                                | R9.11–15, R17.2–3                                    |
| `tui.tsx`                                         | TUI composition, routes, subscriptions, automatic-update orchestration, and route-aware update toast                                                               | R3, R18.3–5, R18.9                                   |
| `tui/session/`, `tui/tasks/`                      | TUI data ops: list/create, serialized metadata patching, send-back, merge, triage, approval, retry                                                                 | R1, R3.2, R11, R12, R17.8                            |
| `tui/dialogs/create-task.tsx`                     | Custom OpenTUI create dialog, including configured/custom task scope selection                                                                                     | R1.1–1.4, R1.9–10                                    |
| `tui/board/store.tsx`                             | Solid board store: grouping, ordering, selection, move gating, refresh, notices, and update status                                                                 | R3, R7, R17.4–5, R18                                 |
| `tui/board/commands.tsx`                          | Key bindings and dialog flows: create, move, triage, approve/merge, send-back, retry, task details view                                                            | R5.2, R10, R11, R12, R17.3, R17.7                    |
| `tui/board/board.tsx` / `column.tsx` / `card.tsx` | Board layout, column headers with cap, cards with task number and badges; board footer shows version and persistent update status                                  | R3, R3.7–8, R7.4, R18                                |
| `tui/updates.ts`                                  | one-hour npm latest/manifest cache and `engines.opencode` classification                                                                                           | R3.8, R18.1, R18.4, R18.6–7                          |
| `tui/update-manager.ts`                           | exact-release preparation, hostile-path validation, disposal promotion/restore, and successful-load cleanup                                                        | R18.2–3, R18.6–9                                     |
| `tui/format.ts`                                   | Card badges, age/diff/subtask formatting                                                                                                                           | R3.6                                                 |
| `tui/dialogs/task-details.tsx`                    | Read-only task details view from live session metadata and diff stats                                                                                              | R17.7                                                |
| `tui/dialogs/onboarding.tsx`                      | First-run board tour and opt-out persistence                                                                                                                       | R17.6                                                |
| `tui/routes/settings.tsx`                         | Settings route for editing plugin options and saving `opencode.json`                                                                                               | R17.10–13                                            |

## Key flows

**Creation (R1, R2, R17.2–3).** `createTask` lists project sessions to compute the next task number, creates
the worktree (`git worktree add -b kagan/<slug> <dir> <base>`), then creates the session with
`directory = worktree` and the initial `kagan` metadata. The dialog persists field state in the
opener's closure so opening a filterable dropdown and returning preserves entries. The task scope is
stored as configured `cwd` values plus optional custom text. Configured setup commands run only when
their `cwd` is included in the selected scope; custom text is passed to intake as context but does
not trigger shell commands unless it exactly matches a configured `cwd`.

**Intake (R4).** A Backlog board task with `intakeOutcome === undefined` and `intakeSessionID ===
undefined` spawns the intake child (`role: "intake"`, read-only tools + `kagan_intake`), recording
`intakeSessionID` + `intakeOutcome: "pending"` before prompting. `session.created` only bootstraps
`lastGatedStatus` on a fresh board task; that patch itself induces a `session.updated`, and the
state-based check runs exclusively on `session.updated` — never directly on `session.created` — so
a cleared state, from a failed auto-retry or a manual retry, respawns intake through the same single
path rather than two independent triggers. The entry function reads the task's live session state
(re-fetched by id) rather than the metadata snapshot carried on the triggering event, so a guard
never passes against a stale snapshot that predates another in-flight spawn's claim. A synchronous
per-task-per-role single-flight claim (a module-level set keyed on `sessionID:role`, held for the
duration of the entry function and released in a `finally`) makes entry idempotent under duplicate
or concurrent event delivery, closing the window between two overlapping `session.updated`
deliveries before either has persisted its claim. A spawn-time throw routes to the helper-failure
funnel only after the claim is released: OpenCode notifies event listeners of a patch before the
patch call resolves, and the funnel's clear-state patch event is itself the respawn trigger — run
inside the claim it would be swallowed and the auto-retry lost. The child calls `kagan_intake`,
which patches the parent's understanding, sanitized decisions, refined prompt, sets outcome `ran`,
and clears any stale `helperError`.

**Start (R6).** On the gated transition into In Progress, the server records `startedAt`, composes
the start prompt (refined prompt with the human's original description appended under `## Original
task description` when both exist, else whichever of refined prompt/description/title is present,
plus resolved decisions and understanding), appends resolved reference context, and `promptAsync`es
the task session with the chosen model.

**Review (R8, R9).** When the active iteration goes idle, the server captures that iteration's last
assistant report into `report` and moves the task to Review. A Review task with
`validatorOutcome === undefined` and `validatorSessionID === undefined` spawns the validator against
`worktreeDiffs` with the full task context and any prior-triage rulings it must not re-report,
recording `validatorSessionID` + `validatorOutcome: "pending"` before prompting — the same
patch-before-prompt protocol as intake. This state-based check runs on every `session.updated` for
the task, so a cleared state respawns the same as intake, and it follows the same live-read and
single-flight protocol described under Intake above: the entry function re-fetches the task by id
instead of trusting the event's metadata snapshot (a stale snapshot is exactly what the
check-result patch would otherwise re-enter on), and the `sessionID:validator` claim in the shared
single-flight set covers both that induced event and any concurrent duplicate delivery. WHERE
`commands.check` is configured, the server evaluates every check command against the changed-file
list from `worktreeDiffs`: a command runs when a changed file is under its `cwd`, or when a changed
file matches one of its repo-relative `scope` regexes; otherwise the command is recorded as skipped.
All matching commands run, even after a previous command fails, and the validator receives every ran/skipped step as evidence; a failing,
timing-out, or unspawnable command is recorded honestly and does not block review entry. The
validator calls `kagan_findings`, which recomputes
the worktree diff the same way review entry did, runs each finding's `location` through
`verifyFindingCitations` (file/line must resolve inside a diff hunk range, via `domain/diff/ranges.ts`'s pure
`newSideHunkRanges` parser), caps confidence at 2 and marks `outOfDiff` on any citation that
doesn't verify — never dropping the finding — then persists findings, sets outcome `ran`, and
clears any stale `helperError`. If the diff can't be computed (e.g. no worktree), findings persist
unmodified; verification only saves triage attention, it never blocks. Because a send-back can
reset the review while that recomputation is in flight, `kagan_findings` re-reads the parent just
before writing and is a no-op when the calling session is no longer the recorded
`validatorSessionID` (R9.20) — the same stale-write guard the helper-failure detectors use.

**Helper failure and retry (R4.7–9, R9.7–9).** Two detectors feed one shared funnel,
`handleHelperEvent`: a `session.error` event for the helper's session (turn never ran — e.g. an
invalid model), and `session.idle` on the helper with its outcome still `"pending"` (turn ran but
never called its tool). Because both `spawnIntake` and `spawnValidator` record `"pending"` before
prompting, the funnel is identical for both roles: resolve the parent via `helperParentID`, confirm
the parent's recorded helper sessionID still matches this session (ignoring a stale event from a
helper already superseded by a retry), confirm the outcome is still `"pending"`, then call
`handleHelperFailure`. `spawnIntake`/`spawnValidator` let a `promptAsync` failure propagate directly
— the caller's `errorMessage()` already normalizes non-`Error` throws — so a synchronous prompt
error reaches the same handler. On a detected failure the handler reads
`intakeAttempts`/`validatorAttempts` (spawns so far): while at or below `helperRetries`, it clears
the session id and outcome (letting the state-based spawn above respawn it) and logs a line; once
exhausted, it sets the outcome to `failed` and records `helperError: { role, message }`. Failed
intake stays intake-ready (R4.6) and failed validator stays approvable (R12.7) by the pre-existing
semantics — the handler never invents a new stuck state. Same-session-id guards on both detectors
ignore a stale event from a helper already superseded by a retry.

**Send-back (R11).** `sendBack` reads the previous iteration's report and the changed-file list,
creates a worker child in the worktree, prompts it with `composeHandoffPrompt`, and applies one root
patch: `nextGenerationPatch` (bump generation, carry intended/ignored findings into `priorTriage`,
clear review state) + `status: in_progress` + `activeIteration`. It is gated by the same
`moveDenyReason("in_progress", …)` the manual move uses.

**Approve + merge (R12).** Approval is gated by `approveDenyReason` (validator ran, all findings
disposed). On approval the merge dialog offers current branch / chosen branch / no action, labeled
"Squash-merge into …" or "Merge into …" per the resolved `squashMerge` option; `mergeTaskBranch`
commits outstanding work and merges (directly when the target is checked out, else via a temporary
worktree). A failed merge aborts approval and the move.

By default the merge squashes the task branch into one commit on the target (`git merge --squash`
followed by `git commit`); `squashMerge: false` performs the prior plain `git merge` instead,
preserving the branch's individual commits. `git merge --squash` makes no commit itself, so a
"nothing to commit" result (no net change vs. the target) is treated as a successful no-op rather
than a failure. `git merge --abort` does not work after `--squash` (there's no `MERGE_HEAD`), so
conflict cleanup differs by which worktree is merging: in a temporary worktree, a conflict just
returns failure and the existing `finally` removes the worktree; in the main worktree, the squash
only proceeds if `git status --porcelain` is empty first — if it's dirty, Kagan refuses without
touching anything, and only when it was clean does a conflict get cleaned up with `git reset --hard
HEAD`, which is safe precisely because the pre-squash clean check already ran.

**References (R13).** `resolveTaskRefs` parses `#N`, lists project sessions once, maps by task
number, and formats each hit from the referenced session's own metadata (title, status, intake
understanding, `report`) — no per-reference API calls. Misses render `(#N not found)`.

**Remote push denial (R16).** `server.ts` registers a `tool.execute.before` hook alongside the
`event` handlers above. On every `bash` tool call it runs the pure matcher `isGitPushCommand`
(`git/runner.ts`) against `output.args.command`; if the command pushes to a remote, it re-fetches the
calling session and checks the pure predicate `isSupervisedSession` (`domain/task/policy.ts`) — true for a root
board task, a helper/iteration child identified by role, or a session carrying a kagan parent
back-pointer even if role is absent. When both are true it throws, which OpenCode surfaces to the
agent as a failed tool call carrying the thrown message; a generic OpenCode session (neither board
task, role, nor parent back-pointer) is left untouched, and non-`bash` tool calls and non-push bash
commands are ignored.

**Automatic update (R18).** Only TUI instances loaded from bare `@kagan-sh/kagan` or explicit
`@latest` resolve npm `latest`; exact pins and file installs return before network access. A newer
clean release is classified only after its manifest supplies a valid `engines.opencode` range.
Compatible latest is prepared exactly through `api.plugins.add`, which independently performs the
host compatibility check and imports the package without activating a duplicate `kagan` plugin id
(the host dedupes by module id).
The manager verifies that the current and prepared targets are valid
`opencode/packages/@kagan-sh/kagan@…/node_modules/@kagan-sh/kagan` wrappers before writing one
sibling marker. Its disposal callback renames current to one backup, promotes prepared to
`kagan@latest`, and restores current if promotion fails in-process. If promotion was interrupted by
process exit, the host re-downloads `latest` on the next launch (requiring network) and Kagan's
cleanup then removes the leftover backup, marker, and prepared directory. The next successful load
of the marker's version removes only that validated backup and marker. Ready/blocked/broken status is a dedicated store signal: home/session routes
receive one host toast for ready/blocked, while the board renders persistent footer text and never
consumes Notice capacity.

## Configuration

The canonical option list is [`docs/reference/configuration.md`](../../docs/reference/configuration.md).
`domain/task/policy.ts` owns option parsing helpers such as `inProgressCap`, `helperRetries`, `squashMerge`,
`commandPlan`, `configuredScopes`, and `sendBackStopThreshold`; `server/validator/spawn.ts` owns `validatorModels` rotation. The TUI settings route can
edit the plugin options and save them back only to project `opencode.json`; it does not hot-reload
the active plugin instance, so it reports that OpenCode must be restarted or the project reopened.

## Out of scope (removed subsystems)

Deliberately not part of this design: CI as an approval gate, risk tiers and `.kagan/repo.yaml`,
approve-time comprehension quizzes, two-stage approval, scope-drift tracking, review receipts, retro
lessons, approve nudges, and the outcome mirror. Permission waits are surfaced as attention markers,
not used as a gate. These boundaries keep the supervision loop focused on intake → work → review →
approve.

Further ideas already evaluated and rejected, with rationale, are in
[mental-model.md](./mental-model.md#evaluated-and-rejected).
