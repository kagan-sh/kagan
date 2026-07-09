---
name: self-align
description: Adversarial self-audit of the Kagan repo through four lenses — code quality & YAGNI, security, spec alignment, docs/AGENTS.md freshness — each finding self-assessed and reported by severity and confidence for the developer to action. Use only when explicitly asked to self-align, audit, or health-check the codebase; it spawns multiple agents and is not for routine questions.
disable-model-invocation: true
allowed-tools: Read, Grep, Glob, Bash, Agent
argument-hint: "[quality, security, spec, docs]"
---

# Self-Align Audit

Audit this repo through four adversarial lenses, then self-assess every finding before reporting.
Written for any agent (Claude Code, OpenCode, Codex): tool names below are capabilities, not
specific tools — map them to your environment.

## Lenses

| Lens     | Criteria file                        | Targets                                                |
| -------- | ------------------------------------ | ------------------------------------------------------ |
| quality  | [lens-quality.md](lens-quality.md)   | `src/`, `test/`, `scripts/` — quality, YAGNI, idioms   |
| security | [lens-security.md](lens-security.md) | `src/`, `scripts/` — input→sink paths, destructive ops |
| spec     | [lens-spec.md](lens-spec.md)         | `.specs/` vs `src/` behavior                           |
| docs     | [lens-docs.md](lens-docs.md)         | `README.md`, `docs/`, `CONTRIBUTING.md`, `AGENTS.md`   |

Do not load the criteria files yourself — each lens agent reads its own. Load one only in the
sequential fallback, one at a time.

## Ground rules (all phases, all agents)

- Never read `references/` (vendored, out of scope), `node_modules/`, `dist/`, `.plans/`, or `bun.lock`.
- Never run linters, type checks, or tests — `bun run check` owns those; re-deriving them wastes tokens.
- Read-only audit: no writes, no network, no state-changing commands.
- Every finding must cite evidence actually read, not assumed.

Each invocation is fresh: re-read the repo state and discard findings, verdicts, and reports from
any earlier run in this session — the code may have changed since.

## Phase 1: Scope

Arguments (text after the skill name, e.g. `/self-align security docs`) name the lenses to run.
Empty or unrecognized → run all four. Done when the active lens list is fixed.

## Phase 2: Dispatch lens agents

Launch one subagent per active lens, all in parallel (Claude Code: Agent tool calls in a single
message; OpenCode: task tool). Each agent's prompt is this template with `{LENS}` and
`{LENS_FILE}` filled in:

```
You are one adversarial lens in a self-align audit of the Kagan repo at {REPO_ROOT}.
Read {LENS_FILE} and audit exactly that lens — nothing outside it. Read AGENTS.md first for orientation.

Rules: never read references/, node_modules/, dist/, .plans/, or bun.lock. Do not run linters,
type checks, tests, or any write/network command. Be adversarial — hunt for what is wrong, stale,
or unjustified — but cite only evidence you actually read.

Severity anchors:
- critical: exploitable vulnerability, or loss/corruption of user sessions, worktrees, or uncommitted work
- high: contradicts a numbered spec requirement; a documented instruction fails as written; realistic unsafe input path
- medium: spec/docs drift, YAGNI violation, dead code, stale or unproven claim
- low: polish, minor inconsistency

Return at most 8 findings, most severe first, in exactly this format and nothing else:

## {LENS} findings
- severity: <critical|high|medium|low>
  location: <path:line or doc heading>
  issue: <one sentence>
  evidence: <the exact line(s) or fact proving it>
  fix: <one sentence>

If the lens is clean, return: ## {LENS} findings — none.
```

**Sequential fallback** (no subagent mechanism, e.g. Codex): run the lenses one at a time in order
security → spec → quality → docs. For each, read its criteria file, audit, and write the findings
block in the same format before moving on. Phase 3 still runs as a separate pass afterwards.

Done when every active lens has returned its findings block (or "none").

## Phase 3: Self-assessment

Done when every finding carries a confidence score or a recorded refutation. For every finding,
in this order:

1. **Dedupe** — same root cause reported by two lenses: merge, keep the higher severity, note both lenses.
2. **Refute against the counter-source** — a finding survives only after checking the source most
   likely to disprove it, not just the cited location:
   - spec findings: the full text of the cited requirement — it may say the opposite of the paraphrase
   - dead-code and duplication findings: grep for callers, including same-file usage
   - docs findings: the exact code or manifest line the doc allegedly contradicts
   - all findings: tests. A passing test that asserts the flagged behavior means the behavior is
     intentional — re-file as a spec/docs question at reduced severity, not a code bug.

   Then ask: would I file this myself from this evidence? If not, or the evidence doesn't hold,
   discard it — but record it (Phase 4).

3. **Score confidence** for survivors. A finding whose counter-source was not checked is not
   scored — verify it or discard it.
   - **high** — counter-source checked, defect held, stated concretely and reproducibly
   - **medium** — counter-source checked and the defect holds, but severity or the right fix is debatable
   - **low** — judgment call; reasonable maintainers could disagree
4. **Re-score severity** against the anchors above; downgrade anything inflated. A code comment at
   the flagged site documenting a deliberate tradeoff caps severity at medium — report it as a
   tradeoff to revisit, not a defect.

## Phase 4: Report

```markdown
# Self-Align Report

**Scope:** {lenses run} · **Findings:** {N} ({counts per severity}) · **Discarded:** {M}

## Action items

Sorted by severity (critical → low), then confidence (high → low).

### 1. [{SEVERITY} · {confidence} confidence · {lens}] {one-line title}

- **Where:** {path:line or doc heading}
- **Issue:** {what is wrong}
- **Fix:** {what to do}

## Discarded in self-assessment

- {original claim} — {one-line refutation}

## Clean

{lenses or areas with nothing to report}
```

Omit empty sections. The report is the final output — no preamble or process narration around it.

## Keywords

self-align, codebase audit, health check, spec drift, docs freshness, stale docs, yagni, over-engineering, adversarial review, alignment audit
