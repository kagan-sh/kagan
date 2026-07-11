import { describe, expect, test } from "bun:test"
import { buildTaskMetadata, nextTaskNumber } from "../../../src/domain/task/metadata"

describe("nextTaskNumber", () => {
  test("returns 1 when no sessions exist", () => {
    expect(nextTaskNumber([])).toBe(1)
  })

  test("returns one greater than the highest task number", () => {
    expect(
      nextTaskNumber([{ metadata: { kagan: { taskNumber: 2 } } }, { metadata: { kagan: { taskNumber: 5 } } }]),
    ).toBe(6)
  })
})

describe("buildTaskMetadata", () => {
  test("builds the canonical backlog patch shape", () => {
    expect(
      buildTaskMetadata({
        taskNumber: 3,
        baseBranch: "main",
        worktree: "/wt",
        description: "Fix login",
        model: { providerID: "anthropic", modelID: "claude" },
        scope: { values: ["project-alpha"] },
        setup: { command: "setup: npm ci", exitCode: 0, output: "ok" },
      }),
    ).toEqual({
      status: "backlog",
      boardTask: true,
      taskNumber: 3,
      baseBranch: "main",
      worktree: "/wt",
      description: "Fix login",
      model: { providerID: "anthropic", modelID: "claude" },
      scope: { values: ["project-alpha"] },
      setup: { command: "setup: npm ci", exitCode: 0, output: "ok" },
    })
  })

  test("omits blank description and optional keys", () => {
    const patch = buildTaskMetadata({ taskNumber: 1, baseBranch: "main", worktree: "/wt", description: "   " })
    expect(patch).toEqual({
      status: "backlog",
      boardTask: true,
      taskNumber: 1,
      baseBranch: "main",
      worktree: "/wt",
    })
  })
})
