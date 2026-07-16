# Developing software with AI agents

AI tooling will keep changing. The durable part is not a model, IDE, or coding CLI. It is the
control loop around delegated work: decide what can be delegated, make the intent clear, contain
the change, verify it, judge it, and integrate it deliberately.

This playbook is extracted from the rules Kagan enforces. Kagan is one implementation, not a
requirement. Apply the same practices with a checklist, issue tracker, pull request template, git
worktrees, CI, hooks, or a harness built on any coding agent.

## 1. Delegate only work you can verify

Use the highest level of autonomy whose failure you can catch with a check you trust more than the
agent. A passing test does not make a change safe by itself; consider the cost of a miss, whether
you need to understand the result, how clear the task is, and whether the work follows a known
pattern.

Use assisted or manual work when verification is weak, the blast radius is high, or discovery is
part of the task. Do not treat a tool's ability to make a change as evidence that it should make
the change.

**Enforce it:** choose a work mode in the task description; require a stated verification method
before delegation; have a harness recommend a lower-autonomy mode when no credible check exists.

## 2. Define a reviewable change before editing

An agent needs more than a prompt. Give it an agreed contract: the intended outcome, constraints,
what is out of scope, the definition of done, and decisions that could change the implementation.

Inspect the codebase before committing to a plan. Turn unknowns into explicit questions. A person
must answer or consciously accept the assumptions that matter before edits start. This is cheaper
than discovering a disagreement after the code is written.

One delegated task should produce one coherent, reviewable change. If the agent must discover the
product decision, redesign several subsystems, or keep more context than a reviewer can hold, split
the work first.

Split along independently useful outcomes, not arbitrary file counts. A good boundary lets one
agent finish, one reviewer understand the diff, and one person decide whether to integrate it.

**Enforce it:** use an issue template with a done-condition, scope, and open decisions; run a
read-only planning pass; require decisions to be resolved before write access; create separate tasks
for discovery and each implementation that follows.

## 3. Contain the change and minimise authority

Do not let delegated work modify the checkout, branch, credentials, or remote state you are trying
to protect. Give every change an isolated workspace and a clear integration boundary.

Give each agent only the tools it needs. A planning or review agent normally needs read access, not
shell access or write access. An implementation agent may need more, but it should not need the
ability to deploy, push, or merge its own work.

**Enforce it:** use a branch, worktree, container, or disposable clone per task; use least-privilege
tool permissions; block direct remote pushes and deployments from delegated sessions.

## 4. Limit work by your capacity to judge it

Agents create candidate changes faster than people can review them. The right concurrency limit is
the number of changes you can genuinely inspect and decide on, not the number of agents your
machine or subscription can run.

Treat a review queue as a queue of decisions owed by a person. Lower the limit when reviews wait
or become shallow. Raise it only when verification remains prompt and careful.

**Enforce it:** set a personal work-in-progress limit; limit active task branches; have a harness
deny new work when the active-task cap is reached.

## 5. Keep an honest record of state and evidence

Chat messages are not reliable workflow state. Record the task contract, current phase, decisions,
changed files, commands run, outcomes, and unresolved questions where the next person or agent can
read them.

Record failures, timeouts, skipped checks, and partial results as honestly as passes. A missing or
failed check is information about confidence, not a reason to pretend it ran. Keep enough evidence
to reproduce the decision without keeping unbounded logs.

**Enforce it:** update the issue or pull request with results; make CI publish all relevant check
outcomes; store task state durably in the harness instead of deriving it from the latest chat turn.

## 6. Review the diff and preserve the judgment

Judge the actual change, not the agent's completion message. Compare the diff to the agreed task
and decisions. Look for work that was not requested, missing work, defects, and things the diff
cannot prove.

Use a reviewer who approaches the work independently when practical. Require every claim to name
the relevant file and changed line, or label it as uncertainty. Treat deterministic checks as
corroborating evidence, not as permission to skip review.

Every review finding needs a decision: fix it, explain why it is intended, or record why it does
not apply. Empty approvals and one-word dismissals do not preserve judgment.

When a change returns for another iteration, carry forward the original contract, prior report,
changed files, findings to address, and rulings that must not be reopened. The next agent continues
from the current state instead of starting a new argument or a new implementation.

**Enforce it:** review a base-branch diff; require findings to distinguish bugs, misalignment, and
uncertainty; require a substantive reason for each dismissal; include rulings in the follow-up task;
make a harness block approval until every finding has a valid disposition.

## 7. Make integration deliberate and recovery safe

Finishing a task and integrating it are separate decisions. Only a person or an explicit protected
workflow should move reviewed work into a shared branch, publish it, or deploy it. Choosing not to
integrate is valid when the work was exploratory.

Automation also fails. Design the loop so a failed helper, timed-out command, duplicate event, or
interrupted run can be retried without duplicating work, losing a completed step, or leaving the
task permanently stuck. Preserve the last known state before starting the next side effect.

**Enforce it:** protect integration branches; require explicit merge approval; make task operations
idempotent; save per-step outcomes and offer safe retry or manual handoff paths.

## Choose an enforcement level

Start with the lightest mechanism that makes skipping a practice difficult. Move up when the cost
of a miss, the number of contributors, or the amount of delegation increases.

| Level               | Suitable mechanisms                                                                                                                      |
| ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Personal practice   | A task note, a focused branch, a pre-merge diff review, and a written check result.                                                      |
| Repository practice | Issue and pull request templates, contributor rules, protected branches, required CI, and agent instruction files.                       |
| Harness practice    | Isolated task environments, permission boundaries, durable task state, work-in-progress limits, review gates, and retry-safe operations. |

The implementation can change with every generation of agent tooling. The control loop should stay
the same: keep delegation within your ability to verify, and integration within your ability to
judge.
