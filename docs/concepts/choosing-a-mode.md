# Choosing a mode

Not every task should be delegated. Use the mode whose failure you can catch with a check you trust.

**Kagan is optimized for exactly one of these modes: autonomous.** The intake gate, the sandboxed
worktree, the auto-started agent, and the independent review all exist to make _delegation_ honest.
Kagan's reach is the frontier of _safe_ autonomy — the autonomous↔assisted boundary. When a task
falls on the far side of that line, kagan says so at intake and expects you to drive it yourself; it
does not pretend the boundary is elsewhere. The detail is in [Where kagan sits](#where-kagan-sits).

## The three modes

| Mode           | What you do                                                          | Right when                                                                                        |
| -------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| **Autonomous** | Delegate the task, walk away, judge the result.                      | There is a cheap, trusted automatic check and the blast radius of a miss is low.                  |
| **Assisted**   | Work in the editor with AI help; you own every line before it lands. | The task matters for security, maintenance, or regulation, or the spec needs discovery first.     |
| **Manual**     | Write the code yourself; AI only answers questions and finds docs.   | The work is novel, irreversible, or requires deep comprehension that full delegation would erode. |

## The five factors

| Factor                 | Question                                                                                                     | Pushes toward autonomous                         | Pushes toward manual                             |
| ---------------------- | ------------------------------------------------------------------------------------------------------------ | ------------------------------------------------ | ------------------------------------------------ |
| **Oracle strength**    | Is there a cheap, deterministic check that catches a wrong result — and do you trust it more than the model? | Strong tests, types, or build (`commands.check`) | Only human eyes can tell                         |
| **Blast radius**       | What does a bad merge cost?                                                                                  | Local, reversible, cheap to revert               | Prod data, money, safety, security, auth         |
| **Comprehension need** | Must _you_ understand this line-by-line afterward?                                                           | Throwaway, boilerplate, scaffolding              | Core domain, regulated, or maintained by you     |
| **Specifiability**     | Can it be pinned to one context window with a clear done-condition?                                          | Crisp spec, bounded                              | Fuzzy, exploratory, "I'll know it when I see it" |
| **Distribution**       | Well-trodden pattern or novel edge case?                                                                     | CRUD, framework glue, documented API             | New algorithm, sparse docs, no training signal   |

## Decide in four steps

1. **Do you trust an oracle more than the model?** If no, autonomous is out. In kagan terms: if you cannot write a configured check that fails when the work is wrong, the only verifier is you.
2. **If yes, what's the blast radius on a miss?** High or irreversible → drop to assisted, even with an oracle. An oracle reduces the chance of being wrong; it never reduces it to zero.
3. **Do you need durable line-by-line comprehension?** Yes → assisted. Engagement builds comprehension; reading generated code does not.
4. **Is it specifiable in one sitting and in-distribution?** Yes → autonomous. Fuzzy or novel → start in assisted or manual to discover the spec, then hand the now-crisp, oracle-backed pieces back to autonomous.

## The one-line rule

Use the highest-autonomy mode whose failure is caught by a check you trust more than the model.

"Trust" is operational: the probability that a wrong output survives your verification multiplied by the blast radius if it does. Anchor on the oracle, not the feeling.

## Worked examples

| Task                                           | Mode                       | Why                                                                                               |
| ---------------------------------------------- | -------------------------- | ------------------------------------------------------------------------------------------------- |
| Add a field to a tested CRUD endpoint          | Autonomous                 | Existing tests are a strong oracle; low blast radius; no comprehension debt.                      |
| Refactor the auth token-refresh path           | Assisted                   | Security blast radius; you must understand it.                                                    |
| Design a novel rate-limiter for a payment path | Manual → autonomous pieces | Start manual to invent and understand, then delegate the specified, test-backed mechanical parts. |
| Rename a symbol repo-wide                      | Autonomous                 | The compiler is a near-perfect oracle.                                                            |

## Where kagan sits

Kagan enforces the **autonomous** mode and the **autonomous↔assisted** boundary. Its reach is the frontier of safe autonomy: tasks with configured checks you trust, plus a validator whose citations are diff-verified.

If a task cannot be given such a check, kagan is structurally telling you to drop to assisted. That is why check evidence is never a gate: kagan's job is to inform your judgment about where the boundary is, not to pretend it is elsewhere.

When you select a Backlog card, the intake's mode rationale appears on the card. The same rationale
is shown in the findings-review header. If no check is configured, the rationale appends
"(no automatic check configured - lean assisted)" as a reminder that without a deterministic
oracle you are the verifier.
