# Mental Model — What Kagan Is and Why

Companion to [requirements.md](./requirements.md) and [design.md](./design.md). Those documents
say what Kagan does; this one says what Kagan is _for_, so future changes can be judged against
the intent instead of accreting features.

## The problem Kagan solves

AI agents generate change faster than a human can validate it. Left unstructured, the gap fills
with unverified merges: the developer feels fast while self-perception of speed runs far ahead of
measured delivery, understanding of the codebase decays, and quality problems surface downstream
where they are expensive — generation accelerates while integration stalls.

**Kagan's goal: nothing reaches your branch faster than your judgment can keep up — without
requiring you to sit and watch agents work.**

## The model

You are a tech lead with a small number of AI engineers. Kagan is the process that makes the
delegation honest.

**Columns are trust states, not progress states.**

| Column      | Trust state                                      |
| ----------- | ------------------------------------------------ |
| Backlog     | We have not yet agreed what to do                |
| In Progress | Agreed and delegated — don't watch               |
| Review      | Claims to be done — an unverified claim          |
| Done        | Independently reviewed, judged by me, integrated |

**The board is a queue of decisions awaiting you**, not a task tracker. Cards needing you sort
first; everything else is deliberately quiet.

**Human attention is the scarce resource.** Kagan spends it at exactly three points — the ones
with the highest leverage per minute — and protects it everywhere else:

1. **Intake** (before any code): confirm or override the agent's stated assumptions. The cheapest
   possible correction point; a one-line answer here replaces a rework cycle later.
2. **Triage** (when work claims done): an independent reviewer on a different model has already
   read the diff; you rule on each disputed finding, and every ruling requires a real reason.
3. **Merge** (the trust boundary): nothing crosses into a shared branch without explicit approval.

Between these points the design protects attention rather than demanding it: worktree isolation
means agents cannot touch anything you care about, and the WIP cap bounds review load to what one
person can genuinely judge — supervision quality collapses as concurrency rises.

## How to use it

- **Shape tasks to fit one context window.** Intake quality determines everything downstream; a
  vague task produces a confident agent doing the wrong thing.
- **Answer intake decisions honestly** rather than approving in bulk — this is where intent
  enters the system.
- **Once a card is In Progress, walk away.** Watching defeats the design; the point is moving you
  from the inner loop (supervising tokens) to the outer loop (brief → delegate → judge → integrate).
- **Treat triage as the job, not an interruption.** Slow triage is correct triage; optimizing
  approval latency is how code ends up in production effectively unreviewed.
- **Fix via send-back, not by hand-editing the worktree**, so every change stays inside the
  reviewed loop.

## Anti-goals

Kagan does **not** maximize throughput, does **not** minimize decision latency, and does **not**
pursue autonomy. The WIP cap, the gates, and the friction at triage are the product, not overhead
to be optimized away.

**Feature filter:** if a proposed change makes _approving easier_ without making _judging
better_, it moves in the wrong direction and should be rejected.

## Principles

The anti-goals and feature filter derive from six operating principles. Each has an evidence
anchor and a kagan mechanism that embodies it.

1. **Verify with something you trust more than the model, deterministically.** Mechanism:
   `checkCommand` + citation verification. Evidence: Kamoi 2024.
2. **Spend human attention only at the highest-leverage points.** Mechanism: three gates + WIP cap.
3. **Keep the human generating, not just approving.** Mechanism: fix via send-back instead of
   hand-editing task worktrees. Evidence: Anthropic skill-formation.
4. **Make trust legible and portable.** Mechanism: trust packets. Evidence: Conway; Herbsleb–Mockus.
5. **Budget by cost-per-verified-success, human-centric.** Mechanism: prefer configured checks over
   LLM-judge calls. Evidence: Wharton CoT; LiteLLM; OWASP LLM10.
6. **Right-size autonomy to enforceable determinism.** Mechanism: mode advisor.

## Evaluated and rejected

Kept here so the same ideas are not re-litigated from scratch. Each was rejected on adversarial
review:

- **Acceptance-criteria field threaded intake → validator.** The validator already receives the
  refined prompt, understanding, and every human-resolved decision. An explicit criteria list
  would be authored by the same intake agent with the same blind spots — AI checking AI against
  AI-written criteria — and predictably degrades into criteria-theater ("tests pass"). The
  human-resolved decisions are the real spec anchor and already flow through.
- **Review-queue back-pressure (block new In Progress pulls while Review backlog is high).**
  An org-scale pipeline remedy applied to a solo tool with a WIP cap of 2; the Review column is
  visible in one glance and needs-you sorting already exists. Policy machinery for a problem not
  yet hit.
- **Cross-generation iteration journal (append-only log of all reports).** Tasks rarely survive
  enough send-backs for report history to matter; the narrower real defect it exposed — human
  triage rulings being discarded on send-back — is fixed directly instead.
