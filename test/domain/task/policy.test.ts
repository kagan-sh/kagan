import { describe, expect, test } from "bun:test"
import {
  approveDenyReason,
  canRestartHelper,
  columnMoveDenyReason,
  countInProgressForMove,
  getRefinedPrompt,
  helper,
  helperRestartPatch,
  helperRetries,
  inProgressCap,
  intakeReady,
  isSupervisedSession,
  needsHuman,
  nextGenerationPatch,
  pendingFindingCount,
  pendingRequiredIntakeDecisions,
  squashMerge,
} from "../../../src/domain/task/policy"
import {
  commandInTaskScope,
  commandMatchesChangedFile,
  commandPlan,
  configuredScopes,
  sanitizeTaskScope,
} from "../../../src/domain/task/commands"
import { isSubstantive, resolveIntakeDecision, sanitizeIntakeDecisions } from "../../../src/domain/task/intake"
import {
  isResolvedFinding,
  resolveFinding,
  sortFindingsByConfidence,
  verifyFindingCitations,
} from "../../../src/domain/task/findings"
import { kagan, validMode } from "../../../src/domain/task/metadata"

const substantiveNote = "Refactored the callsite to guard against null before the high-risk operation is invoked."

describe("inProgressCap", () => {
  test("defaults to 2 without options", () => {
    expect(inProgressCap()).toBe(2)
    expect(inProgressCap({})).toBe(2)
  })

  test("reads an integer inProgressLimit >= 1", () => {
    expect(inProgressCap({ inProgressLimit: 5 })).toBe(5)
    expect(inProgressCap({ inProgressLimit: 1 })).toBe(1)
  })

  test("rejects non-integer, zero, and negative limits", () => {
    expect(inProgressCap({ inProgressLimit: 0 })).toBe(2)
    expect(inProgressCap({ inProgressLimit: -3 })).toBe(2)
    expect(inProgressCap({ inProgressLimit: 2.5 })).toBe(2)
    expect(inProgressCap({ inProgressLimit: "3" })).toBe(2)
  })
})

describe("kagan().approved / needsHuman / kagan().boardTask", () => {
  test("kagan().approved reads the approved flag", () => {
    expect(kagan({ kagan: { approved: true } }).approved).toBe(true)
    expect(kagan({ kagan: {} }).approved).toBeUndefined()
    expect(kagan(undefined).approved).toBeUndefined()
  })

  test("needsHuman is true only in review while unapproved", () => {
    expect(needsHuman("review", { kagan: {} })).toBe(true)
    expect(needsHuman("review", { kagan: { approved: true } })).toBe(false)
    expect(needsHuman("in_progress", { kagan: {} })).toBe(false)
  })

  test("needsHuman is true in any column while a permission is waiting", () => {
    expect(
      needsHuman("in_progress", {
        kagan: { awaitingPermissions: [{ id: "p1", title: "Run rm -rf?", sessionID: "s1" }] },
      }),
    ).toBe(true)
    expect(needsHuman("backlog", { kagan: { awaitingPermissions: [{ id: "p1", title: "x", sessionID: "s1" }] } })).toBe(
      true,
    )
  })

  test("kagan().boardTask reads the boardTask flag", () => {
    expect(kagan({ kagan: { boardTask: true } }).boardTask).toBe(true)
    expect(kagan({ kagan: {} }).boardTask).toBeUndefined()
    expect(kagan(undefined).boardTask).toBeUndefined()
  })
})

describe("kagan() metadata view", () => {
  test.each([
    ["description", "Add rate limiting to the API.", "Add rate limiting to the API.", "   "],
    ["baseBranch", "main", "main", "   "],
    ["worktree", "/tmp/wt", "/tmp/wt", "   "],
    ["taskNumber", 4, 4, 0],
    ["report", "Refactored the parser.", "Refactored the parser.", "  "],
    [
      "model",
      { providerID: "anthropic", modelID: "claude" },
      { providerID: "anthropic", modelID: "claude" },
      { providerID: "anthropic" },
    ],
    ["activeIteration", "s1", "s1", ""],
    ["startedAt", 42, 42, "42"],
    ["role", "intake", "intake", "bogus"],
    ["workerParent", "root1", "root1", 42],
  ] as const)(
    "%s accepts a valid value and salvages a malformed one to undefined",
    (field, valid, expected, invalid) => {
      expect(kagan({ kagan: { [field]: valid } })[field]).toEqual(expected)
      expect(kagan({ kagan: { [field]: invalid } })[field]).toBeUndefined()
      expect(kagan({ kagan: {} })[field]).toBeUndefined()
    },
  )

  test("generation defaults to 1 and accepts an explicit value", () => {
    expect(kagan({ kagan: { generation: 3 } }).generation).toBe(3)
    expect(kagan({ kagan: {} }).generation).toBe(1)
    expect(kagan(undefined).generation).toBe(1)
  })

  test("scope returns sanitized task scope metadata", () => {
    expect(kagan({ kagan: { scope: { values: ["project-alpha", "project-alpha"], custom: " docs " } } }).scope).toEqual(
      {
        values: ["project-alpha"],
        custom: "docs",
      },
    )
    expect(kagan({ kagan: { scope: { values: [] } } }).scope).toBeUndefined()
    expect(kagan({ kagan: { scope: null } }).scope).toBeUndefined()
  })
})

describe("isSupervisedSession", () => {
  test("true for a root board task", () => {
    expect(isSupervisedSession({ kagan: { boardTask: true } })).toBe(true)
  })

  test("true for a helper session identified by role", () => {
    expect(isSupervisedSession({ kagan: { role: "intake" } })).toBe(true)
    expect(isSupervisedSession({ kagan: { role: "validator" } })).toBe(true)
    expect(isSupervisedSession({ kagan: { role: "worker" } })).toBe(true)
  })

  test("true for a session carrying a kagan parent back-pointer even without role", () => {
    expect(isSupervisedSession({ kagan: { intakeParent: "root1" } })).toBe(true)
    expect(isSupervisedSession({ kagan: { validatorParent: "root1" } })).toBe(true)
    expect(isSupervisedSession({ kagan: { workerParent: "root1" } })).toBe(true)
  })

  test("false for a generic OpenCode session", () => {
    expect(isSupervisedSession({ kagan: {} })).toBe(false)
    expect(isSupervisedSession(undefined)).toBe(false)
    expect(isSupervisedSession({})).toBe(false)
  })
})

describe("kagan().awaitingPermissions", () => {
  test("returns a well-formed list", () => {
    expect(
      kagan({ kagan: { awaitingPermissions: [{ id: "p1", title: "Run rm -rf?", sessionID: "s1" }] } })
        .awaitingPermissions,
    ).toEqual([{ id: "p1", title: "Run rm -rf?", sessionID: "s1" }])
  })

  test("rejects malformed shapes", () => {
    expect(kagan({ kagan: { awaitingPermissions: [{ id: "p1", title: "x" }] } }).awaitingPermissions).toBeUndefined()
    expect(kagan({ kagan: { awaitingPermissions: { id: "p1" } } }).awaitingPermissions).toBeUndefined()
    expect(kagan({ kagan: { awaitingPermissions: null } }).awaitingPermissions).toBeUndefined()
    expect(kagan({ kagan: {} }).awaitingPermissions).toBeUndefined()
    expect(kagan(undefined).awaitingPermissions).toBeUndefined()
  })
})

describe("getRefinedPrompt", () => {
  test("returns the refined prompt only when substantive", () => {
    const good = "Implement the exponential backoff retry policy described in the ticket with jitter."
    expect(getRefinedPrompt({ kagan: { intake: { understanding: "x", decisions: [], refinedPrompt: good } } })).toBe(
      good,
    )
    expect(
      getRefinedPrompt({ kagan: { intake: { understanding: "x", decisions: [], refinedPrompt: "tbd" } } }),
    ).toBeUndefined()
    expect(getRefinedPrompt({ kagan: { intake: { understanding: "x", decisions: [] } } })).toBeUndefined()
    expect(getRefinedPrompt(undefined)).toBeUndefined()
  })
})

describe("isSubstantive", () => {
  test("rejects blanks, placeholders, and too-short text", () => {
    expect(isSubstantive("")).toBe(false)
    expect(isSubstantive("lgtm")).toBe(false)
    expect(isSubstantive("N/A")).toBe(false)
    expect(isSubstantive("Fixed the null check")).toBe(false)
  })

  test("accepts genuine multi-word explanations", () => {
    expect(isSubstantive(substantiveNote)).toBe(true)
  })
})

describe("countInProgressForMove", () => {
  const sessions = [
    { id: "a", status: "in_progress" as const },
    { id: "b", status: "in_progress" as const },
    { id: "c", status: "backlog" as const },
    { id: "child", parentID: "a", status: "in_progress" as const },
  ]

  test("counts root in-progress sessions, excluding children", () => {
    expect(countInProgressForMove(sessions, "c", "backlog")).toBe(2)
  })

  test("excludes the moving session when leaving another column", () => {
    expect(countInProgressForMove(sessions, "a", "backlog")).toBe(1)
  })

  test("includes the session when moving within in_progress", () => {
    expect(countInProgressForMove(sessions, "a", "in_progress")).toBe(2)
  })
})

describe("columnMoveDenyReason", () => {
  const startable = { kagan: { worktree: "/tmp/wt", intakeOutcome: "ran" } }

  test("denies moving into in_progress at the WIP cap from a non-in_progress source", () => {
    expect(columnMoveDenyReason("in_progress", startable, { inProgressCount: 2, source: "backlog", cap: 2 })).toContain(
      "WIP",
    )
  })

  test("does not gate re-entry into in_progress from a non-backlog source", () => {
    expect(
      columnMoveDenyReason("in_progress", startable, { inProgressCount: 2, source: "in_progress", cap: 2 }),
    ).toBeUndefined()
    expect(
      columnMoveDenyReason("in_progress", startable, { inProgressCount: 0, source: "review", cap: 2 }),
    ).toBeUndefined()
  })

  test("denies backlog → in_progress without a worktree", () => {
    expect(
      columnMoveDenyReason(
        "in_progress",
        { kagan: { intakeOutcome: "ran" } },
        { inProgressCount: 0, source: "backlog" },
      ),
    ).toContain("worktree")
  })

  test("denies backlog → in_progress when intake is not ready", () => {
    expect(
      columnMoveDenyReason(
        "in_progress",
        { kagan: { worktree: "/tmp/wt" } },
        { inProgressCount: 0, source: "backlog" },
      ),
    ).toBeDefined()
    expect(
      columnMoveDenyReason(
        "in_progress",
        {
          kagan: {
            worktree: "/tmp/wt",
            intakeOutcome: "ran",
            intake: { understanding: "x", decisions: [{ id: "d1", question: "q", assumption: "a", required: true }] },
          },
        },
        { inProgressCount: 0, source: "backlog" },
      ),
    ).toBe("1 intake decision(s) need your answer before starting")
  })

  test("allows backlog → in_progress when worktree present and intake ready", () => {
    expect(columnMoveDenyReason("in_progress", startable, { inProgressCount: 0, source: "backlog" })).toBeUndefined()
  })

  test("soft-degrades when intake spawn failed", () => {
    expect(
      columnMoveDenyReason(
        "in_progress",
        { kagan: { worktree: "/tmp/wt", intakeOutcome: "failed" } },
        { inProgressCount: 0, source: "backlog" },
      ),
    ).toBeUndefined()
  })

  test("gates done on approval", () => {
    expect(columnMoveDenyReason("done", { kagan: {} })).toContain("approved")
    expect(columnMoveDenyReason("done", { kagan: { approved: true } })).toBeUndefined()
  })

  test("denies in_progress → backlog once the agent has started", () => {
    expect(
      columnMoveDenyReason("backlog", { kagan: { startedAt: 1 } }, { inProgressCount: 0, source: "in_progress" }),
    ).toContain("already started")
  })

  test("allows in_progress → backlog before the agent has started", () => {
    expect(
      columnMoveDenyReason("backlog", { kagan: {} }, { inProgressCount: 0, source: "in_progress" }),
    ).toBeUndefined()
  })

  test("denies moving a done task to any other column", () => {
    expect(columnMoveDenyReason("review", { kagan: {} }, { inProgressCount: 0, source: "done" })).toContain("Done")
    expect(columnMoveDenyReason("backlog", { kagan: {} }, { inProgressCount: 0, source: "done" })).toContain("Done")
  })

  test("allows review → in_progress (send-back) unaffected by the new backward-move rules", () => {
    expect(columnMoveDenyReason("in_progress", { kagan: {} }, { inProgressCount: 0, source: "review" })).toBeUndefined()
  })
})

describe("kagan().lastGatedStatus", () => {
  test("reads a valid column value", () => {
    expect(kagan({ kagan: { lastGatedStatus: "in_progress" } }).lastGatedStatus).toBe("in_progress")
  })

  test("rejects invalid or missing values", () => {
    expect(kagan({ kagan: { lastGatedStatus: "bogus" } }).lastGatedStatus).toBeUndefined()
    expect(kagan({ kagan: {} }).lastGatedStatus).toBeUndefined()
    expect(kagan(undefined).lastGatedStatus).toBeUndefined()
  })
})

describe("kagan().status", () => {
  test("reads a valid column value", () => {
    expect(kagan({ kagan: { status: "review" } }).status).toBe("review")
  })

  test("rejects invalid or missing values", () => {
    expect(kagan({ kagan: { status: "bogus" } }).status).toBeUndefined()
    expect(kagan({ kagan: {} }).status).toBeUndefined()
    expect(kagan(undefined).status).toBeUndefined()
  })
})

describe("approveDenyReason ladder", () => {
  test("rejects non-board tasks first", () => {
    expect(approveDenyReason({ kagan: { validatorOutcome: "ran" } })).toContain("board")
  })

  test("blocks until a validator outcome exists", () => {
    expect(approveDenyReason({ kagan: { boardTask: true } })).toContain("validator")
  })

  test("failed validator is approvable (no deadlock)", () => {
    expect(approveDenyReason({ kagan: { boardTask: true, validatorOutcome: "failed" } })).toBeUndefined()
  })

  test("counts pending findings after validator ran", () => {
    expect(
      approveDenyReason({
        kagan: {
          boardTask: true,
          validatorOutcome: "ran",
          findings: [
            { id: "f1", summary: "a" },
            { id: "f2", summary: "b", resolution: "intended" },
          ],
        },
      }),
    ).toBe("1 finding(s) need triage")
  })

  test("approvable once all findings are triaged", () => {
    expect(
      approveDenyReason({
        kagan: {
          boardTask: true,
          validatorOutcome: "ran",
          findings: [{ id: "f1", summary: "a", resolution: "ignored", note: substantiveNote }],
        },
      }),
    ).toBeUndefined()
  })
})

describe("isResolvedFinding matrix", () => {
  test("no resolution is unresolved", () => {
    expect(isResolvedFinding({ id: "f", summary: "s" })).toBe(false)
  })

  test("ignored requires a substantive note", () => {
    expect(isResolvedFinding({ id: "f", summary: "s", resolution: "ignored" })).toBe(false)
    expect(isResolvedFinding({ id: "f", summary: "s", resolution: "ignored", note: "ok" })).toBe(false)
    expect(isResolvedFinding({ id: "f", summary: "s", resolution: "ignored", note: substantiveNote })).toBe(true)
  })

  test("clarified requires a substantive note", () => {
    expect(isResolvedFinding({ id: "f", summary: "s", resolution: "clarified" })).toBe(false)
    expect(isResolvedFinding({ id: "f", summary: "s", resolution: "clarified", note: substantiveNote })).toBe(true)
  })

  test("intended is resolved without a note unless severity is high", () => {
    expect(isResolvedFinding({ id: "f", summary: "s", resolution: "intended" })).toBe(true)
    expect(isResolvedFinding({ id: "f", summary: "s", resolution: "intended", severity: "low" })).toBe(true)
    expect(isResolvedFinding({ id: "f", summary: "s", resolution: "intended", severity: "high" })).toBe(false)
    expect(
      isResolvedFinding({ id: "f", summary: "s", resolution: "intended", severity: "high", note: substantiveNote }),
    ).toBe(true)
  })
})

describe("pendingFindingCount", () => {
  test("returns 0 without findings", () => {
    expect(pendingFindingCount(undefined)).toBe(0)
    expect(pendingFindingCount({ kagan: {} })).toBe(0)
  })

  test("counts findings lacking a valid resolution", () => {
    expect(
      pendingFindingCount({
        kagan: {
          findings: [
            { id: "f1", summary: "a" },
            { id: "f2", summary: "b", resolution: "intended" },
            { id: "f3", summary: "c", resolution: "ignored" },
          ],
        },
      }),
    ).toBe(2)
  })
})

describe("sortFindingsByConfidence", () => {
  test("sorts descending, ranking unscored findings last", () => {
    const sorted = sortFindingsByConfidence([
      { id: "a", summary: "a", confidence: 3 },
      { id: "b", summary: "b" },
      { id: "c", summary: "c", confidence: 7 },
    ])
    expect(sorted.map((f) => f.id)).toEqual(["c", "a", "b"])
  })

  test("does not mutate the input", () => {
    const input = [
      { id: "a", summary: "a", confidence: 1 },
      { id: "b", summary: "b", confidence: 9 },
    ]
    sortFindingsByConfidence(input)
    expect(input.map((f) => f.id)).toEqual(["a", "b"])
  })
})

describe("resolveFinding", () => {
  test("updates the matching finding and preserves others", () => {
    const findings = [
      { id: "f1", summary: "one" },
      { id: "f2", summary: "two" },
    ]
    const result = resolveFinding(findings, "f2", "clarified", substantiveNote)
    expect(result[1]).toEqual({ id: "f2", summary: "two", resolution: "clarified", note: substantiveNote })
    expect(result[0]).toEqual(findings[0])
  })

  test("omits note when not provided", () => {
    const result = resolveFinding([{ id: "f1", summary: "one" }], "f1", "intended")
    expect(result[0]?.note).toBeUndefined()
    expect(result[0]?.resolution).toBe("intended")
  })
})

describe("nextGenerationPatch", () => {
  test("bumps generation and purges the review-scoped fields", () => {
    const patch = nextGenerationPatch({ kagan: { generation: 2 } })
    expect(patch).toEqual({
      generation: 3,
      priorTriage: undefined,
      findings: undefined,
      check: undefined,
      validatorSessionID: undefined,
      validatorOutcome: undefined,
      validatorAttempts: undefined,
      helperError: undefined,
      approved: undefined,
    })
  })

  test("starts from generation 1 by default", () => {
    expect(nextGenerationPatch(undefined).generation).toBe(2)
  })

  test("carries intended and ignored findings into prior triage", () => {
    const patch = nextGenerationPatch({
      kagan: {
        generation: 1,
        findings: [
          {
            id: "intended-1",
            summary: "Verbose logs are deliberate",
            category: "misalignment",
            severity: "high",
            confidence: 9,
            resolution: "intended",
            note: substantiveNote,
          },
          {
            id: "ignored-1",
            summary: "Naming nit",
            category: "uncertainty",
            severity: "low",
            confidence: 2,
            resolution: "ignored",
            note: "This is acceptable because the local naming follows generated API conventions.",
          },
          { id: "open-1", summary: "Still broken", resolution: undefined },
          { id: "clarified-1", summary: "Needs worker action", resolution: "clarified", note: substantiveNote },
        ],
      },
    })
    expect(patch.priorTriage).toEqual([
      {
        id: "intended-1",
        summary: "Verbose logs are deliberate",
        category: "misalignment",
        severity: "high",
        confidence: 9,
        resolution: "intended",
        note: substantiveNote,
      },
      {
        id: "ignored-1",
        summary: "Naming nit",
        category: "uncertainty",
        severity: "low",
        confidence: 2,
        resolution: "ignored",
        note: "This is acceptable because the local naming follows generated API conventions.",
      },
    ])
  })

  test("accumulates prior triage across generations", () => {
    const first = { id: "f1", summary: "Keep sync logging", resolution: "intended" as const }
    const second = {
      id: "f2",
      summary: "Known flaky external check",
      resolution: "ignored" as const,
      note: substantiveNote,
    }
    const patch = nextGenerationPatch({ kagan: { generation: 2, priorTriage: [first], findings: [second] } })
    expect(patch.priorTriage).toEqual([first, second])
  })
})

describe.each(["check", "setup"] as const)("kagan().%s", (field) => {
  test("returns a well-formed result", () => {
    expect(kagan({ kagan: { [field]: { command: "bun test", exitCode: 0, output: "ok" } } })[field]).toEqual({
      command: "bun test",
      exitCode: 0,
      output: "ok",
    })
  })

  test("accepts a null exit code", () => {
    expect(kagan({ kagan: { [field]: { command: "x", exitCode: null, output: "timed out" } } })[field]).toEqual({
      command: "x",
      exitCode: null,
      output: "timed out",
    })
  })

  test("rejects malformed shapes", () => {
    expect(kagan({ kagan: { [field]: { command: "x", exitCode: "0", output: "ok" } } })[field]).toBeUndefined()
    expect(kagan({ kagan: { [field]: { command: 1, exitCode: 0, output: "ok" } } })[field]).toBeUndefined()
    expect(kagan({ kagan: { [field]: { command: "x", exitCode: 0 } } })[field]).toBeUndefined()
    expect(kagan({ kagan: {} })[field]).toBeUndefined()
  })
})

describe("helperRetries", () => {
  test("defaults to 1 without options", () => {
    expect(helperRetries()).toBe(1)
    expect(helperRetries({})).toBe(1)
  })

  test("reads an integer helperRetries >= 0", () => {
    expect(helperRetries({ helperRetries: 0 })).toBe(0)
    expect(helperRetries({ helperRetries: 3 })).toBe(3)
  })

  test("rejects non-integer and negative values", () => {
    expect(helperRetries({ helperRetries: -1 })).toBe(1)
    expect(helperRetries({ helperRetries: 1.5 })).toBe(1)
    expect(helperRetries({ helperRetries: "2" })).toBe(1)
  })
})

describe("squashMerge", () => {
  test("defaults to true without options", () => {
    expect(squashMerge()).toBe(true)
    expect(squashMerge({})).toBe(true)
  })

  test("reads an explicit true or false", () => {
    expect(squashMerge({ squashMerge: true })).toBe(true)
    expect(squashMerge({ squashMerge: false })).toBe(false)
  })

  test("defaults to true for non-boolean values", () => {
    expect(squashMerge({ squashMerge: "false" })).toBe(true)
    expect(squashMerge({ squashMerge: 0 })).toBe(true)
  })
})

describe("commandPlan", () => {
  test("normalizes configured setup and check commands", () => {
    expect(
      commandPlan(
        {
          commands: {
            setup: [{ name: "alpha deps", cwd: "project-alpha", command: "npm ci", scope: ["^package"] }],
            check: [{ name: "beta check", cwd: "project-beta", command: "npm run verify" }],
          },
        },
        "setup",
      ),
    ).toEqual([{ name: "alpha deps", cwd: "project-alpha", command: "npm ci", scope: ["^package"] }])
  })

  test("rejects unsafe cwd and invalid regex scopes", () => {
    const options = {
      commands: {
        check: [
          { name: "root", cwd: "/", command: "npm test" },
          { name: "abs", cwd: "/tmp", command: "npm test" },
          { name: "win", cwd: "C:\\tmp", command: "npm test" },
          { name: "parent", cwd: "../app", command: "npm test" },
          { name: "regex", cwd: "app", command: "npm test", scope: ["["] },
          { name: "ok", cwd: "app", command: "npm test", scope: ["^shared/"] },
        ],
      },
    }
    expect(commandPlan(options, "check")).toEqual([
      { name: "ok", cwd: "app", command: "npm test", scope: ["^shared/"] },
    ])
  })

  test("normalizes relative cwd spelling", () => {
    expect(
      commandPlan(
        {
          commands: { check: [{ name: "app", cwd: "./app/", command: "npm test" }] },
        },
        "check",
      ),
    ).toEqual([{ name: "app", cwd: "app", command: "npm test" }])
  })

  test("treats an empty scope array as no scope filter", () => {
    expect(
      commandPlan(
        {
          commands: {
            check: [{ name: "ok", cwd: "app", command: "npm test", scope: [] }],
          },
        },
        "check",
      ),
    ).toEqual([{ name: "ok", cwd: "app", command: "npm test" }])
  })

  test("extracts unique configured scopes from command cwd values", () => {
    expect(
      configuredScopes({
        commands: {
          setup: [{ name: "alpha deps", cwd: "project-alpha", command: "npm ci" }],
          check: [
            { name: "alpha check", cwd: "project-alpha", command: "npm test" },
            { name: "beta check", cwd: "project-beta", command: "npm test" },
            { name: "root", cwd: ".", command: "npm test" },
          ],
        },
      }),
    ).toEqual(["project-alpha", "project-beta"])
  })
})

describe("task scope and command matching", () => {
  test("sanitizes task scope values and custom text", () => {
    expect(sanitizeTaskScope({ values: ["project-alpha", "project-alpha", ""], custom: " docs " })).toEqual({
      values: ["project-alpha"],
      custom: "docs",
    })
    expect(sanitizeTaskScope({ values: [] })).toBeUndefined()
  })

  test("runs setup commands only when task scope includes their cwd", () => {
    const command = { name: "alpha deps", cwd: "project-alpha", command: "npm ci" }
    expect(commandInTaskScope(command, { values: ["project-alpha"] })).toBe(true)
    expect(commandInTaskScope(command, { values: ["project-beta"], custom: "project-alpha" })).toBe(true)
    expect(commandInTaskScope(command, { values: ["project-beta"], custom: "docs" })).toBe(false)
    expect(commandInTaskScope({ name: "root", cwd: ".", command: "npm ci" }, undefined)).toBe(true)
  })

  test("runs check commands for changed files under cwd or repo-relative scope matches", () => {
    const command = { name: "alpha check", cwd: "project-alpha", command: "npm test", scope: ["^\\.github/"] }
    expect(commandMatchesChangedFile(command, ["project-alpha/app/index.tsx"])).toBe(true)
    expect(commandMatchesChangedFile(command, [".github/workflows/check.yml"])).toBe(true)
    expect(commandMatchesChangedFile(command, ["project-beta/app.tsx"])).toBe(false)
  })
})

describe("helper()", () => {
  test("reads a non-empty sessionID, per role", () => {
    expect(helper({ kagan: { intakeSessionID: "i1" } }, "intake").sessionID).toBe("i1")
    expect(helper({ kagan: {} }, "intake").sessionID).toBeUndefined()
    expect(helper({ kagan: { validatorSessionID: "v1" } }, "validator").sessionID).toBe("v1")
    expect(helper({ kagan: {} }, "validator").sessionID).toBeUndefined()
  })

  test("outcome accepts pending, failed, and ran, per role", () => {
    expect(helper({ kagan: { intakeOutcome: "pending" } }, "intake").outcome).toBe("pending")
    expect(helper({ kagan: { intakeOutcome: "bogus" } }, "intake").outcome).toBeUndefined()
    expect(helper({ kagan: { validatorOutcome: "pending" } }, "validator").outcome).toBe("pending")
    expect(helper({ kagan: { validatorOutcome: "ran" } }, "validator").outcome).toBe("ran")
    expect(helper({ kagan: { validatorOutcome: "failed" } }, "validator").outcome).toBe("failed")
    expect(helper(undefined, "validator").outcome).toBeUndefined()
  })

  test("attempts defaults to 0, per role", () => {
    expect(helper(undefined, "intake").attempts).toBe(0)
    expect(helper({ kagan: { intakeAttempts: 2 } }, "intake").attempts).toBe(2)
    expect(helper(undefined, "validator").attempts).toBe(0)
    expect(helper({ kagan: { validatorAttempts: 3 } }, "validator").attempts).toBe(3)
  })

  test("parent reads the role-specific back-pointer", () => {
    expect(helper({ kagan: { intakeParent: "root1" } }, "intake").parent).toBe("root1")
    expect(helper({ kagan: { validatorParent: "root1" } }, "validator").parent).toBe("root1")
    expect(helper({ kagan: {} }, "intake").parent).toBeUndefined()
  })

  test("kagan().helperError reads a well-formed error", () => {
    expect(kagan({ kagan: { helperError: { role: "validator", message: "boom" } } }).helperError).toEqual({
      role: "validator",
      message: "boom",
    })
  })

  test("kagan().helperError rejects malformed shapes", () => {
    expect(kagan({ kagan: { helperError: { role: "worker", message: "boom" } } }).helperError).toBeUndefined()
    expect(kagan({ kagan: { helperError: { role: "intake" } } }).helperError).toBeUndefined()
    expect(kagan({ kagan: {} }).helperError).toBeUndefined()
  })
})

describe("sanitizeIntakeDecisions", () => {
  test("keeps well-formed decisions, defaults required, dedupes, caps at 6", () => {
    expect(sanitizeIntakeDecisions([{ id: "d1", question: "q", assumption: "a" }])).toEqual([
      { id: "d1", question: "q", assumption: "a", required: true },
    ])
    expect(sanitizeIntakeDecisions([{ id: "d1", question: "q", assumption: "a", required: false }])[0]?.required).toBe(
      false,
    )
    expect(sanitizeIntakeDecisions([{ id: "d1", question: "q" }])).toEqual([])
    expect(
      sanitizeIntakeDecisions([
        { id: "d1", question: "q1", assumption: "a1" },
        { id: "d1", question: "q2", assumption: "a2" },
      ]),
    ).toHaveLength(1)
    expect(
      sanitizeIntakeDecisions(Array.from({ length: 10 }, (_, i) => ({ id: `d${i}`, question: "q", assumption: "a" }))),
    ).toHaveLength(6)
  })
})

describe("intake decision resolution", () => {
  test("kagan().intake validates shape", () => {
    expect(kagan({ kagan: { intake: { understanding: "x", decisions: [] } } }).intake).toEqual({
      understanding: "x",
      decisions: [],
    })
    expect(kagan({ kagan: { intake: { understanding: "x" } } }).intake).toBeUndefined()
  })

  test("pendingRequiredIntakeDecisions / intakeReady / resolveIntakeDecision", () => {
    const metadata = {
      kagan: {
        intakeOutcome: "ran",
        intake: { understanding: "x", decisions: [{ id: "d1", question: "q", assumption: "a", required: true }] },
      },
    }
    expect(pendingRequiredIntakeDecisions(metadata)).toHaveLength(1)
    expect(intakeReady(metadata)).toBe(false)
    const resolved = resolveIntakeDecision(metadata.kagan.intake.decisions, "d1", "approved")
    expect(intakeReady({ kagan: { intakeOutcome: "ran", intake: { understanding: "x", decisions: resolved } } })).toBe(
      true,
    )
  })
})

describe("kagan().intake?.mode", () => {
  test("returns a well-formed mode recommendation", () => {
    expect(
      kagan({
        kagan: {
          intake: {
            understanding: "x",
            decisions: [],
            mode: { recommended: "assisted", rationale: "No trusted check and the blast radius is high." },
          },
        },
      }).intake?.mode,
    ).toEqual({ recommended: "assisted", rationale: "No trusted check and the blast radius is high." })
  })

  test("rejects an invalid recommended value", () => {
    expect(
      kagan({
        kagan: {
          intake: {
            understanding: "x",
            decisions: [],
            mode: { recommended: "autopilot", rationale: "No trusted check and the blast radius is high." },
          },
        },
      }).intake?.mode,
    ).toBeUndefined()
  })

  test("rejects an insubstantial rationale", () => {
    expect(
      kagan({
        kagan: { intake: { understanding: "x", decisions: [], mode: { recommended: "manual", rationale: "ok" } } },
      }).intake?.mode,
    ).toBeUndefined()
  })

  test("rejects a malformed mode shape", () => {
    expect(
      kagan({ kagan: { intake: { understanding: "x", decisions: [], mode: "manual" } } }).intake?.mode,
    ).toBeUndefined()
    expect(kagan({ kagan: { intake: { understanding: "x", decisions: [] } } }).intake?.mode).toBeUndefined()
  })
})

describe("validMode", () => {
  test("keeps a valid mode object", () => {
    expect(validMode({ recommended: "autonomous", rationale: "A cheap check catches every wrong answer." })).toEqual({
      recommended: "autonomous",
      rationale: "A cheap check catches every wrong answer.",
    })
  })

  test("drops an invalid recommended value", () => {
    expect(
      validMode({ recommended: "full-auto", rationale: "A cheap check catches every wrong answer." }),
    ).toBeUndefined()
  })

  test("drops an insubstantial rationale", () => {
    expect(validMode({ recommended: "manual", rationale: "n/a" })).toBeUndefined()
  })

  test("drops a non-object", () => {
    expect(validMode("manual")).toBeUndefined()
    expect(validMode(undefined)).toBeUndefined()
  })
})

describe("kagan().findings / kagan().priorTriage", () => {
  test("findings filters malformed entries", () => {
    expect(kagan({ kagan: { findings: [{ id: "f1", summary: "s" }, { id: 2 }, null] } }).findings).toEqual([
      { id: "f1", summary: "s" },
    ])
    expect(kagan({ kagan: {} }).findings).toBeUndefined()
  })

  test("findings keeps well-formed detail and location", () => {
    expect(
      kagan({
        kagan: { findings: [{ id: "f1", summary: "s", detail: "full reasoning", location: "src/a.ts:10" }] },
      }).findings,
    ).toEqual([{ id: "f1", summary: "s", detail: "full reasoning", location: "src/a.ts:10" }])
  })

  test("findings drops malformed detail/location but keeps the finding", () => {
    expect(kagan({ kagan: { findings: [{ id: "f1", summary: "s", detail: 42, location: null }] } }).findings).toEqual([
      { id: "f1", summary: "s" },
    ])
  })

  test("priorTriage filters malformed entries", () => {
    expect(kagan({ kagan: { priorTriage: [{ id: "f1", summary: "s" }, { id: 2 }, null] } }).priorTriage).toEqual([
      { id: "f1", summary: "s" },
    ])
    expect(kagan({ kagan: {} }).priorTriage).toBeUndefined()
  })

  test("findings drops a spoofed outOfDiff instead of trusting the validator's own claim", () => {
    expect(kagan({ kagan: { findings: [{ id: "f1", summary: "s", outOfDiff: false }] } }).findings).toEqual([
      { id: "f1", summary: "s" },
    ])
    expect(kagan({ kagan: { findings: [{ id: "f1", summary: "s", outOfDiff: true }] } }).findings).toEqual([
      { id: "f1", summary: "s", outOfDiff: true },
    ])
  })
})

describe("verifyFindingCitations", () => {
  const diffs = [
    { file: "src/a.ts", patch: "@@ -1,2 +1,3 @@\n context\n-old\n+new\n+extra\n", additions: 2, deletions: 1 },
    { file: "src/b.ts", additions: 3, deletions: 0, status: "added" as const },
  ]

  test("leaves a finding untouched when its file:line falls inside a diff hunk", () => {
    const findings = [{ id: "f1", summary: "s", location: "src/a.ts:2", confidence: 8 }]
    expect(verifyFindingCitations(findings, diffs)).toEqual(findings)
  })

  test("leaves a finding untouched when only the file is cited and the file is in the diff", () => {
    const findings = [{ id: "f1", summary: "s", location: "src/b.ts", confidence: 9 }]
    expect(verifyFindingCitations(findings, diffs)).toEqual(findings)
  })

  test("leaves a finding with no location untouched", () => {
    const findings = [{ id: "f1", summary: "s", confidence: 9 }]
    expect(verifyFindingCitations(findings, diffs)).toEqual(findings)
  })

  test("caps confidence and marks outOfDiff when the file is not in the diff", () => {
    const findings = [{ id: "f1", summary: "s", location: "src/missing.ts:5", confidence: 9 }]
    expect(verifyFindingCitations(findings, diffs)).toEqual([
      { id: "f1", summary: "s", location: "src/missing.ts:5", confidence: 2, outOfDiff: true },
    ])
  })

  test("caps confidence and marks outOfDiff when the line falls outside every hunk range", () => {
    const findings = [{ id: "f1", summary: "s", location: "src/a.ts:50", confidence: 9 }]
    expect(verifyFindingCitations(findings, diffs)).toEqual([
      { id: "f1", summary: "s", location: "src/a.ts:50", confidence: 2, outOfDiff: true },
    ])
  })

  test("never raises confidence for an out-of-diff citation, only caps it", () => {
    const findings = [{ id: "f1", summary: "s", location: "src/missing.ts:5", confidence: 1 }]
    expect(verifyFindingCitations(findings, diffs)).toEqual([
      { id: "f1", summary: "s", location: "src/missing.ts:5", confidence: 1, outOfDiff: true },
    ])
  })

  test("defaults confidence to 2 for an out-of-diff citation with no prior confidence", () => {
    const findings = [{ id: "f1", summary: "s", location: "src/missing.ts:5" }]
    expect(verifyFindingCitations(findings, diffs)).toEqual([
      { id: "f1", summary: "s", location: "src/missing.ts:5", confidence: 2, outOfDiff: true },
    ])
  })
})

describe("canRestartHelper", () => {
  test("true in backlog when intake has ever run or spawned", () => {
    expect(canRestartHelper("backlog", { kagan: { intakeOutcome: "failed" } })).toBe(true)
    expect(canRestartHelper("backlog", { kagan: { intakeOutcome: "ran" } })).toBe(true)
    expect(canRestartHelper("backlog", { kagan: { intakeSessionID: "i1" } })).toBe(true)
  })

  test("true in review when validator has ever run or spawned", () => {
    expect(canRestartHelper("review", { kagan: { validatorOutcome: "failed" } })).toBe(true)
    expect(canRestartHelper("review", { kagan: { validatorOutcome: "ran" } })).toBe(true)
    expect(canRestartHelper("review", { kagan: { validatorSessionID: "v1" } })).toBe(true)
  })

  test("true when helperError is recorded for the role", () => {
    expect(canRestartHelper("review", { kagan: { helperError: { role: "validator", message: "boom" } } })).toBe(true)
  })

  test("false when nothing has started", () => {
    expect(canRestartHelper("backlog", { kagan: {} })).toBe(false)
    expect(canRestartHelper("review", undefined)).toBe(false)
  })

  test("false when the column and helper role do not match", () => {
    expect(canRestartHelper("backlog", { kagan: { validatorOutcome: "failed" } })).toBe(false)
    expect(canRestartHelper("review", { kagan: { intakeOutcome: "failed" } })).toBe(false)
    expect(canRestartHelper("review", { kagan: { helperError: { role: "intake", message: "boom" } } })).toBe(false)
  })
})

describe("helperRestartPatch", () => {
  test("clears intake helper state including the stale intake blob", () => {
    expect(helperRestartPatch("intake")).toEqual({
      intakeSessionID: undefined,
      intakeOutcome: undefined,
      intakeAttempts: 0,
      helperError: undefined,
      intake: undefined,
    })
  })

  test("clears validator state and review artifacts without a generation bump", () => {
    expect(helperRestartPatch("validator")).toEqual({
      validatorSessionID: undefined,
      validatorOutcome: undefined,
      validatorAttempts: 0,
      helperError: undefined,
      findings: undefined,
      check: undefined,
      approved: undefined,
    })
  })
})
