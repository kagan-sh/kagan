import "../preload/git-isolation.ts"
import { describe, expect, test } from "bun:test"
import type { PluginInput } from "@opencode-ai/plugin"
import { gitShellForRepo, initTestRepo } from "../fixtures/git-shell"

// Runs in its own `bun test` invocation (see package.json "test") so git/runner is the real module,
// not the stub the TUI unit tests register globally. This is the only guard on the worktree-isolation
// fix: the v1 session.create must route `directory` through the query, never the body — otherwise the
// session lands in the routed main worktree and isolation (R2) silently breaks. createTaskWorktree's
// git behavior and the tool's ask-gating are covered by test/git/runner.test.ts and
// test/server/tools.test.ts respectively.
const { runCreateTasks } = await import("../../src/server/create-tasks")

type CreateCall = {
  query?: { directory?: string }
  body?: { title?: string; directory?: string; metadata?: { kagan?: Record<string, unknown> } }
}

function mockInput(repo: string): { input: PluginInput; createCalls: CreateCall[] } {
  const createCalls: CreateCall[] = []
  const input = {
    worktree: repo,
    $: gitShellForRepo(repo),
    client: {
      session: {
        get: async () => ({ data: { metadata: {} } }),
        list: async () => ({ data: [] }),
        create: async (options: CreateCall) => {
          createCalls.push(options)
          return { data: { id: `task-${createCalls.length}` } }
        },
      },
    },
  } as unknown as PluginInput
  return { input, createCalls }
}

describe("runCreateTasks", () => {
  test("routes the worktree through the create query, never the body", async () => {
    const repo = initTestRepo()
    const { input, createCalls } = mockInput(repo)
    await runCreateTasks(input, {}, [{ title: "Isolated", description: "Runs in its own worktree." }])
    expect(createCalls).toHaveLength(1)
    const call = createCalls[0]!
    expect(call.query?.directory).toBeDefined()
    expect(call.query!.directory).not.toBe(repo)
    expect(call.body?.directory).toBeUndefined()
    const patch = call.body?.metadata?.kagan ?? {}
    expect(patch.boardTask).toBe(true)
    expect(patch.worktree).toBe(call.query!.directory)
    expect(patch.status).toBe("backlog")
  })
})
