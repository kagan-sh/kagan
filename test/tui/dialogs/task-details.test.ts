import { describe, expect, test } from "bun:test"
import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import { buildTaskDetails, dialogScrollMaxHeight } from "../../../src/tui/dialogs/task-details"
import type { Finding } from "../../../src/domain/task/findings"
import type { Intake } from "../../../src/domain/task/intake"

describe("buildTaskDetails", () => {
  test("captures metadata fields and diff stats", () => {
    const intake: Intake = {
      understanding: "Cache per tenant.",
      decisions: [{ id: "d1", question: "TTL?", assumption: "60s", required: true, resolution: "approved" }],
      refinedPrompt: "Add cache.",
    }
    const finding: Finding = {
      id: "f1",
      summary: "Missing test",
      category: "bug",
      severity: "high",
      confidence: 7,
      resolution: "clarified",
      note: "Add a test.",
    }
    const metadata = {
      kagan: {
        taskNumber: 7,
        status: "review",
        report: "Add cache",
        description: "Speed up resolver.",
        baseBranch: "main",
        generation: 3,
        approved: true,
        findings: [finding],
        priorTriage: [],
        intake,
        check: { command: "bun test", exitCode: 0, output: "ok" },
      },
    }
    const diffs: Array<SnapshotFileDiff> = [
      { file: "src/cache.ts", additions: 10, deletions: 2, status: "modified" as const, patch: "+x" },
    ]
    const details = buildTaskDetails(metadata, diffs, "Add tenant cache")
    expect(details.title).toBe("Add tenant cache")
    expect(details.status).toBe("review")
    expect(details.taskNumber).toBe(7)
    expect(details.report).toBe("Add cache")
    expect(details.description).toBe("Speed up resolver.")
    expect(details.baseBranch).toBe("main")
    expect(details.generation).toBe(3)
    expect(details.approved).toBe(true)
    expect(details.findings).toEqual([finding])
    expect(details.intake).toEqual(intake)
    expect(details.check).toEqual({ command: "bun test", exitCode: 0, output: "ok" })
    expect(details.diffStats).toEqual([{ file: "src/cache.ts", additions: 10, deletions: 2, status: "modified" }])
  })

  test("defaults missing fields safely", () => {
    const details = buildTaskDetails({}, [])
    expect(details.generation).toBe(1)
    expect(details.status).toBe("backlog")
    expect(details.approved).toBe(false)
    expect(details.findings).toEqual([])
    expect(details.priorTriage).toEqual([])
    expect(details.diffStats).toEqual([])
  })
})

describe("dialogScrollMaxHeight", () => {
  test("leaves room below the host dialog top pad", () => {
    expect(dialogScrollMaxHeight(40)).toBe(20)
    expect(dialogScrollMaxHeight(24)).toBe(8)
  })

  test("never collapses below six rows", () => {
    expect(dialogScrollMaxHeight(10)).toBe(6)
  })
})
