import { describe, expect, test } from "bun:test"
import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import { resolveValidatorModel, spawnValidator } from "../src/validator"
import type { Intake } from "../src/task"
import type { CheckResult } from "../src/check"
import { mockSpawnInput } from "./fixtures/api"

const diffs: Array<SnapshotFileDiff> = [
  { file: "a.ts", patch: "+x", additions: 1, deletions: 0, status: "added" as const },
]

function mockInput(
  onPrompt: (options: unknown) => void = () => {},
  createResult: { id?: string } = { id: "child-1" },
  onCreate: (options: unknown) => void = () => {},
) {
  return mockSpawnInput({ onPrompt, createResult, onCreate }).input
}

function promptText(options: unknown): string | undefined {
  const typed = options as { body?: { parts?: Array<{ type: string; text?: string }> } }
  return typed.body?.parts?.find((part) => part.type === "text")?.text
}

describe("resolveValidatorModel", () => {
  test("is undefined when no option is set, so the child inherits the session default", () => {
    expect(resolveValidatorModel()).toBeUndefined()
  })

  test("rotates a configured validatorModels list by generation", () => {
    const models = [
      { providerID: "openai", modelID: "gpt-5" },
      { providerID: "anthropic", modelID: "claude-4" },
    ]
    expect(resolveValidatorModel({ validatorModels: models }, 1)).toEqual(models[0])
    expect(resolveValidatorModel({ validatorModels: models }, 2)).toEqual(models[1])
    expect(resolveValidatorModel({ validatorModels: models }, 3)).toEqual(models[0])
  })

  test("prefers validatorModels from a different provider than the builder", () => {
    const models = [
      { providerID: "openai", modelID: "gpt-5" },
      { providerID: "openai", modelID: "gpt-4o" },
      { providerID: "anthropic", modelID: "claude-4" },
    ]
    expect(resolveValidatorModel({ validatorModels: models }, 1, { providerID: "openai", modelID: "o1" })).toEqual({
      providerID: "anthropic",
      modelID: "claude-4",
    })
  })

  test("rotates the different-provider pool when multiple entries qualify", () => {
    const models = [
      { providerID: "openai", modelID: "gpt-5" },
      { providerID: "anthropic", modelID: "claude-4" },
      { providerID: "anthropic", modelID: "claude-3-7" },
    ]
    expect(resolveValidatorModel({ validatorModels: models }, 1, { providerID: "openai", modelID: "o1" })).toEqual(
      models[1],
    )
    expect(resolveValidatorModel({ validatorModels: models }, 2, { providerID: "openai", modelID: "o1" })).toEqual(
      models[2],
    )
    expect(resolveValidatorModel({ validatorModels: models }, 3, { providerID: "openai", modelID: "o1" })).toEqual(
      models[1],
    )
  })

  test("falls back to the full validatorModels list when none differ from the builder provider", () => {
    const models = [
      { providerID: "openai", modelID: "gpt-5" },
      { providerID: "openai", modelID: "gpt-4o" },
    ]
    expect(resolveValidatorModel({ validatorModels: models }, 1, { providerID: "openai", modelID: "o1" })).toEqual(
      models[0],
    )
    expect(resolveValidatorModel({ validatorModels: models }, 2, { providerID: "openai", modelID: "o1" })).toEqual(
      models[1],
    )
  })

  test("treats a missing generation as generation 1", () => {
    const models = [{ providerID: "openai", modelID: "gpt-5" }]
    expect(resolveValidatorModel({ validatorModels: models })).toEqual(models[0])
  })

  test("ignores malformed entries in validatorModels", () => {
    const models = [{ providerID: "openai", modelID: "gpt-5" }, { providerID: "anthropic" }, null]
    expect(resolveValidatorModel({ validatorModels: models }, 1)).toEqual({ providerID: "openai", modelID: "gpt-5" })
  })
})

describe("spawnValidator", () => {
  test("titles the review child by generation", async () => {
    let createBody: { title?: string } | undefined
    const capture = (options: unknown) => {
      createBody = (options as { body?: { title?: string } }).body
    }
    await spawnValidator(
      mockInput(() => {}, { id: "child-1" }, capture),
      "parent-1",
      diffs,
      {
        title: "Task",
        generation: 1,
      },
    )
    expect(createBody?.title).toBe("review")
    await spawnValidator(
      mockInput(() => {}, { id: "child-2" }, capture),
      "parent-1",
      diffs,
      {
        title: "Task",
        generation: 3,
      },
    )
    expect(createBody?.title).toBe("review #3")
  })

  test("returns undefined and never prompts when session.create yields no child id", async () => {
    let prompted = false
    const childID = await spawnValidator(
      mockInput(() => {
        prompted = true
      }, {}),
      "parent-1",
      diffs,
      { title: "Task", generation: 1 },
    )
    expect(childID).toBeUndefined()
    expect(prompted).toBe(false)
  })

  test("prompts read-only with kagan_findings, no model override, category guidance, and the diff", async () => {
    let options: unknown
    const childID = await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      diffs,
      { title: "Add a cache", generation: 1 },
    )
    expect(childID).toBe("child-1")
    const body = (options as { body?: Record<string, unknown> }).body
    expect(body?.tools).toEqual({ read: true, edit: false, write: false, bash: false, kagan_findings: true })
    expect(body?.model).toBeUndefined()
    const text = promptText(options)
    expect(text).toContain("Add a cache")
    expect(text).toContain("misalignment")
    expect(text).toContain("uncertainty")
    expect(text).toContain("kagan_findings")
    expect(text).toContain("--- a.ts")
    expect(text).toContain("detail")
    expect(text).toContain("location")
    expect(text).toContain("file:line")
  })

  test("instructs the review disciplines: scope audit, test integrity, evidence bar, human verification list", async () => {
    let options: unknown
    await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      diffs,
      { title: "Add a cache", generation: 1 },
    )
    const text = promptText(options)
    expect(text).toContain("beneficial, neutral, or drift")
    expect(text).toContain("can actually fail when the logic it covers breaks")
    expect(text).toContain("speculation about code you have not read")
    expect(text).toContain("directed checklist of what to verify by hand")
  })

  test("includes description, intake understanding, resolved decisions, and refined prompt", async () => {
    const intake: Intake = {
      understanding: "Cache the resolver output keyed by tenant.",
      decisions: [
        { id: "d1", question: "TTL?", assumption: "60s", required: true, resolution: "approved" },
        {
          id: "d2",
          question: "Scope?",
          assumption: "per-tenant",
          required: true,
          resolution: "overridden",
          answer: "global",
        },
        { id: "d3", question: "Ignored?", assumption: "n", required: false },
      ],
      refinedPrompt: "Add a per-tenant cache with a 60s TTL to the resolver.",
    }
    let options: unknown
    await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      diffs,
      {
        title: "Add a cache",
        description: "Resolver is too slow under load.",
        intake,
        generation: 1,
      },
    )
    const text = promptText(options)!
    expect(text).toContain("Resolver is too slow under load.")
    expect(text).toContain("Cache the resolver output keyed by tenant.")
    expect(text).toContain("TTL? → 60s")
    expect(text).toContain("Scope? → global")
    expect(text).not.toContain("Ignored?")
    expect(text).toContain("Add a per-tenant cache with a 60s TTL")
  })

  test("includes prior human rulings when prior triage is present", async () => {
    let options: unknown
    await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      diffs,
      {
        title: "Add a cache",
        generation: 2,
        priorTriage: [
          {
            id: "f1",
            summary: "Verbose retry logging",
            category: "bug",
            resolution: "intended",
            note: "The audit trail requires one line per retry attempt.",
          },
          { id: "f2", summary: "Generated name is awkward", category: "uncertainty", resolution: "ignored" },
        ],
      },
    )
    const text = promptText(options)!
    expect(text).toContain("The human has already reviewed these issues in earlier iterations")
    expect(text).toContain(
      "- [bug] Verbose retry logging — ruled intended: The audit trail requires one line per retry attempt.",
    )
    expect(text).toContain("- [uncertainty] Generated name is awkward — ruled ignored")
  })

  test("omits prior human rulings section when prior triage is empty or absent", async () => {
    let emptyOptions: unknown
    await spawnValidator(
      mockInput((o) => (emptyOptions = o)),
      "parent-1",
      diffs,
      { title: "Task", generation: 1, priorTriage: [] },
    )
    expect(promptText(emptyOptions)).not.toContain("The human has already reviewed these issues")

    let absentOptions: unknown
    await spawnValidator(
      mockInput((o) => (absentOptions = o)),
      "parent-1",
      diffs,
      { title: "Task", generation: 1 },
    )
    expect(promptText(absentOptions)).not.toContain("The human has already reviewed these issues")
  })

  test("renders (no diff) when the diff set is empty", async () => {
    let options: unknown
    await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      [],
      { title: "Task", generation: 1 },
    )
    expect(promptText(options)).toContain("(no diff)")
  })

  test("omits agent unless validatorAgent is configured in options", async () => {
    let options: unknown
    await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      diffs,
      { title: "Task", generation: 1 },
    )
    expect((options as { body?: Record<string, unknown> }).body?.agent).toBeUndefined()

    let overrideOptions: unknown
    await spawnValidator(
      mockInput((o) => (overrideOptions = o)),
      "parent-1",
      diffs,
      { title: "Task", generation: 1 },
      {
        validatorAgent: "reviewer",
      },
    )
    expect((overrideOptions as { body?: Record<string, unknown> }).body?.agent).toBe("reviewer")
  })

  test("rotates validatorModels by generation, preferring a different provider than the builder", async () => {
    let options: unknown
    await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      diffs,
      {
        title: "Task",
        generation: 1,
        builderModel: { providerID: "openai", modelID: "o1" },
      },
      {
        validatorModels: [
          { providerID: "openai", modelID: "gpt-5" },
          { providerID: "anthropic", modelID: "claude-4" },
        ],
      },
    )
    expect((options as { body?: Record<string, unknown> }).body?.model).toEqual({
      providerID: "anthropic",
      modelID: "claude-4",
    })
  })

  test("normalizes and rethrows when promptAsync fails", async () => {
    const input = mockInput(() => {
      throw new Error("ProviderModelNotFoundError: model not found")
    })
    await expect(spawnValidator(input, "parent-1", diffs, { title: "Task", generation: 1 })).rejects.toThrow(
      "ProviderModelNotFoundError: model not found",
    )
  })

  test("patches validatorSessionID and pending outcome before prompting", async () => {
    const order: string[] = []
    const { input } = mockSpawnInput({
      onUpdate: (options) => {
        order.push("update")
        const metadata = (options as { body?: { metadata?: Record<string, unknown> } }).body?.metadata
        expect((metadata?.kagan as Record<string, unknown> | undefined)?.validatorOutcome).toBe("pending")
        expect((metadata?.kagan as Record<string, unknown> | undefined)?.validatorSessionID).toBe("child-1")
      },
      onPrompt: () => order.push("prompt"),
    })
    await spawnValidator(input, "parent-1", diffs, { title: "Task", generation: 1 })
    expect(order).toEqual(["update", "prompt"])
  })

  test("includes deterministic check evidence when a check result is provided", async () => {
    const check: CheckResult = { command: "bun test", exitCode: 0, output: "1 passing" }
    let options: unknown
    await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      diffs,
      { title: "Task", generation: 1, check },
    )
    const text = promptText(options)!
    expect(text).toContain("Deterministic check evidence")
    expect(text).toContain("`bun test` exited 0")
    expect(text).toContain("1 passing")
    expect(text).toContain("corroborating evidence")
  })

  test("separates ran check steps from skipped check steps", async () => {
    const check: CheckResult = {
      command: "alpha: npm test && beta: npm test",
      exitCode: 0,
      output: "alpha ok",
      steps: [
        { name: "alpha", cwd: "project-alpha", command: "npm test", status: "ran", exitCode: 0, output: "ok\n" },
        {
          name: "beta",
          cwd: "project-beta",
          command: "npm test",
          status: "skipped",
          exitCode: null,
          output: "",
          reason: "no changed files in scope",
        },
      ],
    }
    let options: unknown
    await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      diffs,
      { title: "Task", generation: 1, check },
    )
    const text = promptText(options)!
    expect(text).toContain("Ran checks:")
    expect(text).toContain("- alpha (project-alpha) — `npm test` exited 0:\nok")
    expect(text).toContain("Skipped checks:\n- beta (project-beta): no changed files in scope")
    expect(text).not.toContain("beta (project-beta) — `npm test`")
  })

  test("renders an incomplete check without an exit code", async () => {
    const check: CheckResult = { command: "bun test", exitCode: null, output: "check timed out after 300s" }
    let options: unknown
    await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      diffs,
      { title: "Task", generation: 1, check },
    )
    const text = promptText(options)!
    expect(text).toContain("Deterministic check `bun test` did not complete: check timed out after 300s")
  })

  test("omits the lockfile patch, keeping only a stat line", async () => {
    const lockfileDiffs: Array<SnapshotFileDiff> = [
      { file: "bun.lock", patch: "+".repeat(200), additions: 120, deletions: 80, status: "modified" as const },
    ]
    let options: unknown
    await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      lockfileDiffs,
      { title: "Task", generation: 1 },
    )
    const text = promptText(options)!
    expect(text).toContain("--- bun.lock (lockfile — patch omitted, +120/-80)")
    expect(text).not.toContain("+".repeat(200))
  })

  test("truncates a patch that exceeds the per-file char limit", async () => {
    const bigPatch = "+".repeat(9000)
    const bigDiffs: Array<SnapshotFileDiff> = [
      { file: "big.ts", patch: bigPatch, additions: 9000, deletions: 0, status: "added" as const },
    ]
    let options: unknown
    await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      bigDiffs,
      { title: "Task", generation: 1 },
    )
    const text = promptText(options)!
    expect(text).toContain("+".repeat(8000))
    expect(text).not.toContain("+".repeat(8001))
    expect(text).toContain(
      "[patch truncated — showing 8000 of 9000 chars; read the file in the worktree for full context]",
    )
  })

  test("emits stat lines for files once the total diff budget is exhausted, keeping earlier patches intact", async () => {
    const fullFiles: Array<SnapshotFileDiff> = Array.from({ length: 7 }, (_, i) => ({
      file: `full-${i}.ts`,
      patch: "+".repeat(8000),
      additions: 8000,
      deletions: 0,
      status: "added" as const,
    }))
    const overBudget: SnapshotFileDiff = {
      file: "over.ts",
      patch: "+".repeat(4001),
      additions: 4001,
      deletions: 0,
      status: "added" as const,
    }
    const after: SnapshotFileDiff = {
      file: "after.ts",
      patch: "+".repeat(10),
      additions: 10,
      deletions: 0,
      status: "added" as const,
    }
    let options: unknown
    await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      [...fullFiles, overBudget, after],
      { title: "Task", generation: 1 },
    )
    const text = promptText(options)!
    for (const f of fullFiles) expect(text).toContain(`--- ${f.file}\n${f.patch}`)
    expect(text).toContain("--- over.ts (patch omitted — diff budget exhausted, +4001/-0)")
    expect(text).toContain("--- after.ts (patch omitted — diff budget exhausted, +10/-0)")
  })

  test("risk ordering keeps a later risky file and drops an earlier trivial file when the budget is exhausted", async () => {
    const risky: SnapshotFileDiff = {
      file: "z-risky.test.ts",
      patch: "+".repeat(8000),
      additions: 8000,
      deletions: 0,
      status: "added" as const,
    }
    const configs: Array<SnapshotFileDiff> = [
      { file: "tsconfig.json", patch: "+".repeat(8000), additions: 8000, deletions: 0, status: "added" as const },
      { file: "vite.config.ts", patch: "+".repeat(8000), additions: 8000, deletions: 0, status: "added" as const },
    ]
    const trivial: SnapshotFileDiff = {
      file: "a-trivial.ts",
      patch: "+".repeat(10),
      additions: 10,
      deletions: 0,
      status: "added" as const,
    }
    const fillers: Array<SnapshotFileDiff> = Array.from({ length: 6 }, (_, i) => ({
      file: `${String.fromCharCode(98 + i)}.ts`,
      patch: "+".repeat(8000),
      additions: 8000,
      deletions: 0,
      status: "added" as const,
    }))
    let options: unknown
    await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      [trivial, ...fillers, ...configs, risky],
      { title: "Task", generation: 1 },
    )
    const text = promptText(options)!
    expect(text).toContain(`--- z-risky.test.ts\n${"+".repeat(8000)}`)
    expect(text).toContain("--- a-trivial.ts (patch omitted — diff budget exhausted, +10/-0)")
  })

  test("omits check guidance when no check result is provided", async () => {
    let options: unknown
    await spawnValidator(
      mockInput((o) => (options = o)),
      "parent-1",
      diffs,
      { title: "Task", generation: 1 },
    )
    const text = promptText(options)!
    expect(text).not.toContain("Deterministic check")
    expect(text).not.toContain("corroborating evidence")
  })
})
