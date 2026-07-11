import { beforeEach, describe, expect, test } from "bun:test"
import type { PluginInput } from "@opencode-ai/plugin"

// Only the no-git tool paths live here: they never reach createTaskWorktree, so they are safe in the
// shared `bun test ./test` process where other files globally mock src/git/runner. The git-touching
// happy path (real worktree creation) is covered in real-git/create-tasks.test.ts, which runs in its
// own process. No mock.module here — it is process-global in Bun and would leak across files.
const { createServerTools } = await import("../../src/server/tools")

type Store = Map<string, { metadata?: Record<string, unknown>; parentID?: string }>

function mockInput(sessions: Store): { input: PluginInput; createCount: () => number } {
  let created = 0
  const input = {
    worktree: "/repo",
    $: {} as PluginInput["$"],
    client: {
      session: {
        get: async ({ path }: { path: { id: string } }) => ({ data: sessions.get(path.id) }),
        list: async () => ({ data: [...sessions.entries()].map(([id, session]) => ({ id, ...session })) }),
        create: async () => {
          created++
          return { data: { id: `task-${created}` } }
        },
      },
    },
  } as unknown as PluginInput
  return { input, createCount: () => created }
}

const toolCtx = (sessionID: string, ask: () => Promise<void> = async () => {}) => ({
  sessionID,
  messageID: "m1",
  agent: "build",
  directory: "/repo",
  worktree: "/repo",
  abort: new AbortController().signal,
  metadata: () => {},
  ask,
})

function toolOutput(result: string | { output: string }) {
  return typeof result === "string" ? result : result.output
}

let sessions: Store

beforeEach(() => {
  sessions = new Map([
    ["caller", { metadata: {} }],
    ["board-root", { metadata: { kagan: { boardTask: true, intakeSessionID: "intake-new", worktree: "/wt" } } }],
    ["intake-child", { metadata: { kagan: { role: "intake", intakeParent: "board-root" } } }],
  ])
})

describe("kagan_create_tasks gating", () => {
  test("denial fails the tool before any task is created", async () => {
    const { input, createCount } = mockInput(sessions)
    const tools = createServerTools(input)
    await expect(
      tools.kagan_create_tasks.execute(
        { tickets: [{ title: "Task", description: "Do the thing." }] },
        toolCtx("caller", async () => {
          throw new Error("denied")
        }),
      ),
    ).rejects.toThrow("denied")
    expect(createCount()).toBe(0)
  })

  test("rejects a supervised caller before asking", async () => {
    sessions.set("board-root", { metadata: { kagan: { boardTask: true } } })
    let asked = false
    const { input } = mockInput(sessions)
    const tools = createServerTools(input)
    await expect(
      tools.kagan_create_tasks.execute(
        { tickets: [{ title: "Task", description: "Do the thing." }] },
        toolCtx("board-root", async () => {
          asked = true
        }),
      ),
    ).rejects.toThrow("regular OpenCode sessions")
    expect(asked).toBe(false)
  })
})

describe("kagan_intake stale-write guard", () => {
  test("ignores intake from a superseded helper session", async () => {
    const { input } = mockInput(sessions)
    const tools = createServerTools(input)
    const result = await tools.kagan_intake.execute(
      { understanding: "Adds retry handling.", decisions: [], refinedPrompt: "Implement retry with backoff." },
      toolCtx("intake-child"),
    )
    expect(toolOutput(result)).toContain("superseded")
    expect((sessions.get("board-root")?.metadata?.kagan as { intake?: unknown })?.intake).toBeUndefined()
  })
})
