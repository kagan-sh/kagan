import { describe, expect, test } from "bun:test"
import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import { isTrustPacket, serializeTrustPacket } from "../src/trust-packet"
import type { Finding, Intake } from "../src/task"

describe("serializeTrustPacket", () => {
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
    const packet = serializeTrustPacket(metadata, diffs)
    expect(packet.version).toBe(1)
    expect(packet.taskNumber).toBe(7)
    expect(packet.report).toBe("Add cache")
    expect(packet.description).toBe("Speed up resolver.")
    expect(packet.baseBranch).toBe("main")
    expect(packet.generation).toBe(3)
    expect(packet.approved).toBe(true)
    expect(packet.findings).toEqual([finding])
    expect(packet.intake).toEqual(intake)
    expect(packet.check).toEqual({ command: "bun test", exitCode: 0, output: "ok" })
    expect(packet.diffStats).toEqual([{ file: "src/cache.ts", additions: 10, deletions: 2, status: "modified" }])
  })

  test("defaults missing fields safely", () => {
    const packet = serializeTrustPacket({}, [])
    expect(packet.generation).toBe(1)
    expect(packet.approved).toBe(false)
    expect(packet.findings).toEqual([])
    expect(packet.priorTriage).toEqual([])
    expect(packet.diffStats).toEqual([])
  })
})

describe("isTrustPacket", () => {
  test("accepts a valid packet", () => {
    expect(
      isTrustPacket({
        version: 1,
        exportedAt: "2026-07-03T00:00:00.000Z",
        generation: 1,
        approved: false,
        findings: [],
        priorTriage: [],
        diffStats: [],
      }),
    ).toBe(true)
  })

  test("rejects non-objects, wrong version, and missing arrays", () => {
    expect(isTrustPacket(null)).toBe(false)
    expect(
      isTrustPacket({
        version: 2,
        exportedAt: "x",
        generation: 1,
        approved: false,
        findings: [],
        priorTriage: [],
        diffStats: [],
      }),
    ).toBe(false)
    expect(
      isTrustPacket({
        version: 1,
        exportedAt: "x",
        generation: 1,
        approved: false,
        findings: "nope",
        priorTriage: [],
        diffStats: [],
      }),
    ).toBe(false)
    expect(
      isTrustPacket({
        version: 1,
        exportedAt: "x",
        generation: 1,
        approved: false,
        findings: [],
        priorTriage: [],
        diffStats: "nope",
      }),
    ).toBe(false)
  })
})
