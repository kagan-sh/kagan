# Requirements — Kagan Supervision Board

Kagan is an OpenCode plugin that supervises AI-assisted software work as a four-column kanban
board (Backlog, In Progress, Review, Done) where each task is an OpenCode session running in an
isolated git worktree. The intent and usage model behind these requirements is in
[mental-model.md](./mental-model.md).

Requirements use EARS (Easy Approach to Requirements Syntax). The system name is **Kagan**;
component subsystems (the board, the create-task dialog, the intake session, the validator
session) are named where the behavior is theirs. Keywords: ubiquitous (`SHALL`), `WHILE` (state),
`WHEN` (event), `WHERE` (optional feature), `IF … THEN` (unwanted behavior).

---

## Requirement 1 — Task creation

**User Story:** As a developer, I want to create a task from a board dialog by entering a title
and description, choosing a model, and selecting a base branch, so that a supervised session
starts from a known point with the context I intend.

#### Acceptance Criteria

1. WHEN the user triggers task creation on the board THEN the create-task dialog SHALL present a
   title field, a description field, a scope selector, a model selector, and a base-branch selector.
2. THE create-task dialog SHALL be a custom OpenTUI component rendered inside the host dialog stack.
3. WHERE the project exposes more than one model, the model selector SHALL open a filterable
   dropdown listing every provider/model pair plus an "Auto (session default)" entry.
4. WHERE the repository has local branches, the base-branch selector SHALL open a filterable
   dropdown listing them and SHALL default to the currently checked-out branch.
5. IF the user submits the dialog with an empty title THEN Kagan SHALL reject the submission and
   SHALL inform the user that a title is required.
6. WHEN the user submits a valid dialog THEN Kagan SHALL create the task's isolated worktree before
   creating the session, and SHALL create the session with its directory set to that worktree.
7. WHEN a task session is created THEN Kagan SHALL assign it a task number equal to one greater than
   the highest existing task number, or 1 when no tasks exist.
8. WHEN a task session is created THEN Kagan SHALL record on the session the status `backlog`, the
   board-task marker, the task number, the base branch, the worktree path, the description when
   non-blank, and the chosen model when one was selected.
9. WHERE configured command scopes exist, the scope selector SHALL allow choosing one or more
   configured `cwd` values; WHERE no configured scopes exist, it SHALL still allow a custom
   free-form scope value.
10. WHEN a task is created with a custom free-form scope THEN Kagan SHALL persist it as task context
    for intake, but SHALL NOT use it to run shell commands unless it exactly matches a configured
    command `cwd`.

---

## Requirement 2 — Worktree isolation

**User Story:** As a developer, I want every board-initiated session to run in its own git
worktree branched from my chosen base, so that concurrent agents never contaminate each other or
my working tree.

#### Acceptance Criteria

1. WHEN Kagan creates a task worktree THEN Kagan SHALL create it on a new branch named `kagan/<slug>`
   branched from the selected base branch, under a per-repository worktree root.
2. IF `git worktree add` fails THEN Kagan SHALL abort task creation and SHALL surface the git error
   to the user.
3. IF a task in Backlog has no recorded worktree THEN Kagan SHALL deny moving it to In Progress and
   SHALL explain that board tasks must run sandboxed.
4. THE validator, intake, and iteration child sessions of a task SHALL run against that task's
   worktree.

---

## Requirement 3 — Board display

**User Story:** As a developer, I want the board to show only my supervised tasks with clear
status cues, so that I can see at a glance what needs my attention.

#### Acceptance Criteria

1. THE board SHALL render exactly four columns in order: Backlog, In Progress, Review, Done.
2. THE board SHALL render a card only for root sessions that carry the board-task marker.
3. IF a session is a generic OpenCode session without the board-task marker THEN the board SHALL NOT
   render it as a card.
4. WHERE a task has child sessions, the board SHALL attach them under their parent task's card.
5. WHERE a task has a task number, its card SHALL display that number as a `#N` prefix on the title.
6. WHILE a Backlog task is intake-ready, its card SHALL display a distinct border color and an
   `intake ok` badge indicating it is eligible to move to In Progress.
7. THE board footer SHALL display the plugin name and version.
8. WHILE a task is in Review and not yet approved, the board SHALL sort its card ahead of other
   cards in that column.
9. WHILE a Backlog task's intake outcome is `failed`, the card SHALL display a distinct failed
   badge rather than the ready badge, so a failed helper never displays as a success.
10. WHEN the user reorders the selected root card within its column, the board SHALL swap it with
    its adjacent neighbor in that column's persisted order and SHALL re-render the column in the
    new order; a reorder command issued with a subtask selected SHALL be a no-op.
11. WHILE an In Progress task's active agent is busy, its card SHALL display a live `working`
    indicator; WHILE it is retrying after an error, its card SHALL display a live `retrying`
    indicator instead; an In Progress card with neither indicator SHALL be treated as stalled.
    THE indicator SHALL NOT appear on Backlog, Review, or Done cards.
12. WHERE the user filters cards with a query of the form `#N`, the board SHALL match only the
    card whose task number equals `N` exactly, in addition to the existing title/slug substring
    match.

---

## Requirement 4 — Intake gate

**User Story:** As a developer, I want an automatic read-only analysis of a new task before any
code is written, so that the agent's understanding and my own are tested and aligned first.

#### Acceptance Criteria

1. WHEN a board task is created in Backlog with no prior intake outcome THEN Kagan SHALL spawn a
   read-only intake child session.
2. THE intake session SHALL be granted read access and SHALL be denied edit, write, and bash access.
3. WHEN the intake session runs THEN Kagan SHALL instruct it to return an understanding of the task,
   a list of clarifying decisions, and a refined final instruction prompt for the implementing
   agent.
4. WHEN the intake session records its assessment THEN Kagan SHALL persist the understanding, the
   sanitized decisions, and the refined prompt on the parent task and SHALL set the intake outcome
   to `ran`.
5. IF the intake session cannot be spawned THEN Kagan SHALL set the intake outcome to `failed`.
6. WHILE a task's intake outcome is `failed`, Kagan SHALL treat the task as intake-ready so that a
   failed helper never strands the task.
7. IF the intake session's turn fails to run (a provider/model error after spawn) or finishes idle
   without calling its recording tool THEN Kagan SHALL detect the failure and record it as an
   intake spawn attempt failure.
8. WHILE a detected intake failure's attempt count is at or below the configured `helperRetries`,
   Kagan SHALL clear the intake session and automatically respawn it without user action; WHEN
   retries are exhausted, Kagan SHALL set the intake outcome to `failed` and SHALL record the
   failure's message.
9. WHILE a Backlog task's intake has started but not succeeded (failed, or spawned without an
   outcome), Kagan SHALL offer a manual retry that clears the recorded intake session, outcome, and
   attempt count so the state-based spawn respawns it — without affecting the task's worktree — so
   recovery does not depend on a failure having been auto-detected first.
10. WHEN duplicate or concurrent session lifecycle events are delivered for a task THEN Kagan SHALL
    spawn at most one intake session for it (a new spawn SHALL occur only after the recorded intake
    state is cleared).
11. WHEN the intake session runs THEN Kagan SHALL instruct it to assess whether the task fits a
    single focused implementation session and, WHEN it does not, to include a required decision
    proposing how to split it.
12. WHEN the intake session records its assessment THEN Kagan SHALL persist an advisory mode
    recommendation (autonomous, assisted, or manual) with a one-line rationale. This recommendation
    SHALL be informational only and SHALL NOT appear in move-gating or approval-gating logic.

---

## Requirement 5 — Intake decision resolution

**User Story:** As a developer, I want to confirm or override each clarifying assumption before
work starts, so that the agent proceeds on decisions I have actually validated.

#### Acceptance Criteria

1. WHILE a Backlog task has unresolved required intake decisions, Kagan SHALL deny moving it to
   In Progress.
2. WHEN the user advances a Backlog task with unresolved required decisions THEN the board SHALL
   prompt the user to resolve each decision in turn.
3. WHEN the user approves a decision's assumption THEN Kagan SHALL mark that decision resolved.
4. WHEN the user overrides a decision THEN Kagan SHALL require a substantive answer and SHALL record
   it as the resolution.
5. WHILE every required decision is resolved and the intake outcome is `ran`, Kagan SHALL treat the
   task as intake-ready.

---

## Requirement 6 — Auto-start on In Progress

**User Story:** As a developer, I want moving a task to In Progress to start the agent with the
refined instruction, so that the work begins from the aligned understanding without manual
prompting.

#### Acceptance Criteria

1. WHEN a board task first enters In Progress THEN Kagan SHALL record its start time.
2. WHEN a board task first enters In Progress THEN Kagan SHALL start its session with a prompt
   composed of the refined instruction (or the description, or the title, in that order of
   preference) plus the resolved intake decisions and understanding.
3. WHERE the task has a chosen model, Kagan SHALL start the session with that model.
4. WHERE the task's description references other tasks, Kagan SHALL append the resolved reference
   context to the start prompt.
5. IF a task has already been started THEN Kagan SHALL NOT start it again on subsequent entries to
   In Progress.
6. WHERE an intake-ready task's recommended mode is not autonomous, WHEN the user advances it to
   In Progress THEN the board SHALL first surface the recommendation and its rationale as a
   dismissible advisory prompt; confirming SHALL proceed with the normal auto-start, and dismissing
   SHALL leave the task in Backlog. This prompt SHALL NOT deny the move or appear in move-gating
   logic.
7. WHILE a started In Progress task's agent may still be running, Kagan SHALL deny moving it back
   to Backlog and SHALL report that the user should send it back from Review or delete the task
   instead.

---

## Requirement 7 — Parallelism limit

**User Story:** As a developer, I want a configurable cap on concurrently active tasks, so that I
can supervise a bounded number of agents at once.

#### Acceptance Criteria

1. THE In Progress limit SHALL default to 2.
2. WHERE the plugin options set a valid `inProgressLimit` (an integer of at least 1), Kagan SHALL
   use it as the In Progress limit.
3. IF moving a task into In Progress would exceed the In Progress limit THEN Kagan SHALL deny the
   move and SHALL report the limit.
4. THE In Progress column header SHALL display the current count against the limit.
5. THE In Progress limit SHALL count only root board tasks, excluding child sessions.

---

## Requirement 8 — Completion and review entry

**User Story:** As a developer, I want a task to move to Review automatically when its agent
finishes, so that completed work is queued for evaluation without my intervention.

#### Acceptance Criteria

1. WHEN the active iteration of an in-progress board task goes idle THEN Kagan SHALL move the task
   to Review.
2. WHEN a task moves to Review on completion THEN Kagan SHALL capture the finishing iteration's
   final report as the task's report.
3. IF the idle session is not the task's active iteration THEN Kagan SHALL NOT move the task to
   Review.
4. IF the idle session is an intake or validator helper session THEN Kagan SHALL take no
   review-entry action for it.

---

## Requirement 9 — Review subtask

**User Story:** As a developer, I want an automatic review of the implemented work against the
original intake and description, so that misalignments, bugs, and uncertainties are surfaced and
ranked before I decide.

#### Acceptance Criteria

1. WHEN a board task enters Review THEN Kagan SHALL spawn a read-only validator child session,
   unless one has already been spawned for the current review.
2. WHEN the validator runs THEN Kagan SHALL provide it the worktree diff computed against the base
   branch and the original task context: title, description, intake understanding, resolved
   decisions, and refined prompt.
3. WHEN the validator runs THEN Kagan SHALL instruct it to classify each finding as `misalignment`,
   `bug`, or `uncertainty` and to score each finding's confidence from 0 to 10.
4. WHEN the validator records findings THEN Kagan SHALL persist them on the task and SHALL set the
   validator outcome to `ran`.
5. IF the validator cannot be spawned THEN Kagan SHALL set the validator outcome to `failed`.
6. WHEN findings are presented to the user THEN the board SHALL order them by descending confidence.
7. WHEN the validator runs THEN Kagan SHALL provide prior-triage findings from earlier generations
   and SHALL instruct it not to re-report them or close variations of them.
8. IF the validator session's turn fails to run (a provider/model error after spawn) or finishes
   idle without calling its recording tool THEN Kagan SHALL detect the failure and record it as a
   validator spawn attempt failure.
9. WHILE a detected validator failure's attempt count is at or below the configured
   `helperRetries`, Kagan SHALL clear the validator session and automatically respawn it without
   user action; WHEN retries are exhausted, Kagan SHALL set the validator outcome to `failed` and
   SHALL record the failure's message.
10. WHILE a Review task's validator has started but not succeeded (failed, or spawned without an
    outcome), Kagan SHALL offer a manual retry that clears the recorded validator session, outcome,
    and attempt count so the state-based spawn respawns it — without affecting the task's worktree —
    so recovery does not depend on a failure having been auto-detected first.
11. WHERE the plugin options set a non-empty legacy `checkCommand`, Kagan SHALL run that command once in the task worktree when the task enters Review, SHALL record the command, its exit code, and the last 4000 characters of combined stdout+stderr on the task, and SHALL include that evidence in the validator prompt.
12. WHERE the plugin options set `commands.check`, Kagan SHALL run each configured check whose `cwd` contains at least one changed file, or whose optional repo-relative `scope` regex matches at least one changed file, when the task enters Review.
13. WHEN scoped checks are evaluated THEN Kagan SHALL record every configured check as `ran` or `skipped`; a failed, timed-out, or unspawnable ran check SHALL be recorded honestly and SHALL NOT block review entry or validator spawn.
14. WHERE check evidence is recorded, the board card SHALL display `check ok` when every configured check ran and passed, `check failed` when any ran check failed, `check skipped` when every configured check skipped, and `check partial` when some checks ran and passed while others skipped.
15. THE check evidence SHALL NOT be considered by column-move gating or approval gating, and SHALL NOT appear in `columnMoveDenyReason` or `approveDenyReason` logic.
16. WHEN duplicate or concurrent session lifecycle events are delivered for a task THEN Kagan SHALL
    spawn at most one validator session for it (a new spawn SHALL occur only after the recorded
    validator state is cleared).
17. WHEN the validator records findings THEN Kagan SHALL verify each finding's `location` citation
    against the worktree diff computed the same way the review entry diff is computed.
18. IF a finding's citation does not resolve to a file in the diff, or resolves to a file but its
    line falls outside every new-side hunk range in that file's patch, THEN Kagan SHALL cap the
    finding's confidence at 2 and SHALL mark it `outOfDiff` rather than discarding it.
19. IF the worktree diff cannot be computed when findings are recorded (for example, a missing
    worktree) THEN Kagan SHALL persist the findings unmodified.
20. WHEN the validator runs THEN Kagan SHALL instruct it to: audit the diff for changes the refined
    prompt and resolved decisions did not ask for and report each as a misalignment finding; check
    that every added or changed test can fail when the logic it covers breaks and report one that
    cannot as a bug finding; not report findings resting on unverified speculation and cap claims
    with no concrete failure mode at confidence 2; and report anything the diff alone cannot prove
    as an uncertainty finding.
21. IF findings are recorded by a validator session that is no longer the task's recorded validator
    THEN Kagan SHALL NOT persist them, so a stale write cannot attach a superseded generation's
    findings to the current one.

---

## Requirement 10 — Finding triage

**User Story:** As a developer, I want to ignore a finding, mark it as intended behavior, or
answer it, so that I decide the disposition of each issue the review raises.

#### Acceptance Criteria

1. WHEN the user triages a finding THEN Kagan SHALL offer to ignore it, mark it as intended
   behavior, or answer and clarify it.
2. IF the user ignores or clarifies a finding THEN Kagan SHALL require a substantive note.
3. IF the user marks a high-severity finding as intended THEN Kagan SHALL require a substantive note.
4. WHILE any finding lacks a valid disposition, Kagan SHALL treat the task as not approvable.

---

## Requirement 11 — Send back for another iteration

**User Story:** As a developer, I want to send a reviewed task back for another iteration that
continues from where the last one stopped, so that a second pass builds on prior progress rather
than restarting.

#### Acceptance Criteria

1. WHEN the user sends a task back THEN Kagan SHALL create a fresh iteration child session in the
   task's existing worktree.
2. WHEN a send-back iteration starts THEN Kagan SHALL prompt it with a handoff comprising the task
   instruction, the previous iteration's report, the files already changed in the worktree, the
   findings to address, and the intended-behavior findings to leave unchanged.
3. WHEN a task is sent back THEN Kagan SHALL increment the task generation, clear the prior review
   state, carry findings ruled intended or ignored into the task's prior-triage record, return the
   task to In Progress, and record the new iteration as active.
4. IF sending back would exceed the In Progress limit THEN Kagan SHALL deny the send-back and SHALL
   report the limit without starting an iteration.
5. THE send-back action SHALL apply only to tasks in Review.

---

## Requirement 12 — Approval and merge to Done

**User Story:** As a developer, I want approval to unlock a merge choice when moving a task to
Done, so that I can integrate the work where I want or leave it in place for investigative work.

#### Acceptance Criteria

1. WHILE a task is not approved, Kagan SHALL deny moving it to Done.
2. IF the validator has not run for the current review, or any finding lacks a valid disposition,
   THEN Kagan SHALL treat the task as not approvable and SHALL report the reason.
3. WHEN the user approves an approvable task THEN Kagan SHALL offer to merge the task branch into the
   currently checked-out branch, merge into a user-chosen branch, or take no action.
4. WHEN the user chooses to merge into another branch THEN Kagan SHALL present the local branches
   excluding the task branch.
5. IF a merge fails THEN Kagan SHALL surface the error and SHALL abort the approval and the move to
   Done.
6. WHEN a merge succeeds, or the user chooses no action, THEN Kagan SHALL mark the task approved and
   SHALL move it to Done.
7. WHERE the validator spawn failed, Kagan SHALL still allow approval so that a helper failure never
   deadlocks the task.
8. WHEN the user chooses a merge target THEN Kagan SHALL squash the task branch into it as a single
   commit, by default.
9. WHERE the plugin option `squashMerge` is `false`, Kagan SHALL perform a standard merge that
   preserves the task branch's individual commits instead of squashing.
10. IF the squash cannot proceed because the merge target's checkout is dirty THEN Kagan SHALL refuse
    the merge and SHALL report it without modifying the checkout.
11. WHILE a task is in Done, Kagan SHALL deny moving it to any other column and SHALL report that a
    follow-up task should be created instead.

---

## Requirement 13 — Cross-task references

**User Story:** As a developer, I want to reference other tasks by `#N` in a description, so that
the agent receives a report of the referenced work as extra context.

#### Acceptance Criteria

1. WHERE a task description contains `#N` tokens, Kagan SHALL resolve each referenced task's number
   to its board task.
2. WHEN Kagan resolves a reference THEN Kagan SHALL include the referenced task's title, status,
   intake understanding, and report in the intake prompt and in the auto-start prompt.
3. IF a referenced number matches no task THEN Kagan SHALL render an explicit "not found" line for
   that reference.
4. WHEN a task's active iteration completes and the task enters Review THEN Kagan SHALL capture that
   iteration's final report as the structured report used by references.
5. IF reference resolution fails THEN Kagan SHALL proceed with intake and auto-start without the
   reference context.

---

## Requirement 14 — Generic session isolation

**User Story:** As a developer, I want plain OpenCode sessions to be untouched by the board, so
that I can chat normally without being pulled into the supervision workflow.

#### Acceptance Criteria

1. IF a session was not created as a board task THEN Kagan SHALL NOT spawn intake for it, SHALL NOT
   gate it, and SHALL NOT display it on the board.
2. IF a session created on boot is a generic session THEN Kagan SHALL leave it entirely to OpenCode's
   native session handling.

---

## Requirement 15 — Lifecycle recursion safety

**User Story:** As a developer, I want helper and iteration sessions to be exempt from the task
lifecycle handlers, so that intake and review cannot recursively trigger themselves.

#### Acceptance Criteria

1. IF a session carries a helper or iteration role, or has a parent session, THEN Kagan SHALL skip
   the intake, column-gating, and review-entry handling for it.
2. THE task state relied upon by lifecycle handlers SHALL live in session metadata so that handlers
   behave correctly regardless of which worktree instance receives the event.

---

## Requirement 16 — Remote push denial

**User Story:** As a developer, I want a task session's agent to be unable to push its own branch
to a remote, so that the board's merge dialog stays the only way work leaves the sandbox.

#### Acceptance Criteria

1. IF a supervised session (a board task, or a helper/iteration child identified by role or by a
   kagan parent back-pointer) attempts a bash tool call that runs `git push` THEN Kagan SHALL deny
   the tool call and SHALL explain that the board's merge dialog is the only way to merge reviewed
   work out of the sandbox.
2. THE push detection SHALL recognize common invocation shapes, including a bare `git push`, a
   forced push, a push preceded by a `-C` directory flag, a push chained after another command with
   `&&`/`;`/a newline, and a push with an explicit remote and ref.
3. IF a session was not created as a board task and carries no role or kagan parent back-pointer
   THEN Kagan SHALL NOT deny any of its bash tool calls, including `git push`.

---

## Requirement 17 — Support features

**User Story:** As a developer, I want the supervision loop to expose its supporting state clearly,
so that failures, handoffs, and portable review context are visible instead of hidden.

#### Acceptance Criteria

1. WHEN a task worktree is created THEN Kagan SHALL write an OpenCode plugin config into that
   worktree so the plugin loads for worktree-hosted task sessions.
2. WHERE the plugin options set a non-empty legacy `setupCommand`, Kagan SHALL run that command once
   in the fresh task worktree at creation, SHALL record the command, exit code, and last 4000
   characters of output, and SHALL show `setup ok` or `setup failed` on the card without blocking
   task creation.
3. WHERE the plugin options set `commands.setup`, Kagan SHALL run each setup command whose `cwd` is
   included in the task's selected scope, record skipped setup commands as skipped evidence, and use
   the same `setup ok` / `setup failed` / `setup skipped` / `setup partial` badge rules as checks.
4. WHERE a Review task is sent back at or above the configured `sendBackStopThreshold`, Kagan SHALL
   ask whether to iterate again, let the human take over, or leave the task in Review.
5. WHEN a board task or its active iteration waits on a permission reply THEN Kagan SHALL record an
   awaiting-input marker on the root task and surface a `needs you` badge until the permission is
   answered.
6. WHEN a helper failure is newly observed THEN Kagan SHALL surface a board notice once for that
   failure and keep the persistent failure badge until retry or success clears it.
7. WHEN the board opens for the first time in a run THEN Kagan SHALL offer the onboarding dialog,
   unless the user has opted out; the user SHALL be able to reopen the tour at any time via the
   `kagan.tutorial` palette command (`/kagan-tutorial`), regardless of the opt-out.
8. WHEN the user exports a trust packet THEN Kagan SHALL serialize the task's title, status, intake,
   findings, prior triage, reports, and diff stats as JSON; WHEN the user imports one THEN Kagan
   SHALL display it read-only without mutating local tasks.
9. WHEN concurrent handlers patch the same session's `kagan` metadata THEN Kagan SHALL serialize the
   read-modify-write operations per session so one patch cannot clobber another, and a failed patch
   SHALL NOT block later patches for that session.
10. WHEN the user archives a Done task THEN Kagan SHALL stamp the session's `time.archived` via the
    session API and remove it from the board on the next refresh; Kagan SHALL NOT offer an unarchive
    action, since the session remains reachable through OpenCode's own session tooling.
11. WHEN the user opens Kagan settings THEN Kagan SHALL show the configurable plugin options and a
    JSON preview; WHEN the user saves settings THEN Kagan SHALL update the Kagan plugin entry in the
    project's `opencode.json` only, preserving unrelated config entries, and SHALL tell the user to
    restart OpenCode or reopen the project before expecting the saved settings to apply.
