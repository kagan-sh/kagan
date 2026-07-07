import { describe, expect, test } from "bun:test"
import type { PluginInput } from "@opencode-ai/plugin"
import plugin from "../src/server"
import { approveDenyReason, helper, intakeReady } from "../src/task"

type SessionData = { title?: string; metadata?: Record<string, unknown>; parentID?: string | null }
type ListedSession = { id: string; title?: string; parentID?: string | null; metadata?: Record<string, unknown> }

type Captured = {
  updates: Array<{ id: string; kagan: Record<string, unknown> }>
  creates: Array<Record<string, unknown>>
  prompts: Array<{ id: string; body: Record<string, unknown> }>
  listCalls: Array<unknown>
}

function makeInput(
  config: {
    store?: Record<string, SessionData>
    sessions?: ListedSession[]
    messages?: Record<string, unknown[]>
    createId?: string | undefined | (() => string | undefined)
    shellStdout?: string
    gitStdout?: (args: string[]) => string
    promptError?: (id: string) => Error | undefined
    updateError?: (id: string, kagan: Record<string, unknown>) => Error | undefined
    onUpdate?: (id: string) => Promise<void> | void
  } = {},
): { input: PluginInput; captured: Captured } {
  const captured: Captured = { updates: [], creates: [], prompts: [], listCalls: [] }
  const store = config.store ?? {}
  const createId = "createId" in config ? config.createId : "child-1"
  const input = {
    client: {
      session: {
        get: (async (options: unknown) => {
          const id = (options as { path: { id: string } }).path.id
          return { data: store[id] ?? { metadata: {} } }
        }) as never,
        update: (async (options: unknown) => {
          const typed = options as { path: { id: string }; body: { metadata: Record<string, unknown> } }
          const kagan = typed.body.metadata.kagan as Record<string, unknown>
          const failure = config.updateError?.(typed.path.id, kagan)
          if (failure) throw failure
          captured.updates.push({ id: typed.path.id, kagan })
          const existing = store[typed.path.id] ?? { metadata: {} }
          const existingKagan = (existing.metadata?.kagan as Record<string, unknown>) ?? {}
          store[typed.path.id] = {
            ...existing,
            metadata: { ...existing.metadata, kagan: { ...existingKagan, ...kagan } },
          }
          await config.onUpdate?.(typed.path.id)
          return { data: undefined }
        }) as never,
        create: (async (options: unknown) => {
          captured.creates.push((options as { body: Record<string, unknown> }).body)
          const id = typeof createId === "function" ? createId() : createId
          return { data: id ? { id } : {} }
        }) as never,
        promptAsync: (async (options: unknown) => {
          const typed = options as { path: { id: string }; body: Record<string, unknown> }
          captured.prompts.push({ id: typed.path.id, body: typed.body })
          const failure = config.promptError?.(typed.path.id)
          if (failure) throw failure
          return { data: undefined }
        }) as never,
        list: (async (options: unknown) => {
          captured.listCalls.push(options)
          return { data: config.sessions ?? [] }
        }) as never,
        messages: (async (options: unknown) => {
          const id = (options as { path: { id: string } }).path.id
          return { data: (config.messages ?? {})[id] ?? [] }
        }) as never,
      },
    },
    $: ((_s: TemplateStringsArray, ..._e: unknown[]) => {
      const stdout = config.gitStdout ? config.gitStdout((_e[1] as string[]) ?? []) : (config.shellStdout ?? "")
      return {
        nothrow: () => ({
          quiet: async () => ({ stdout: Buffer.from(stdout), stderr: Buffer.from(""), exitCode: 0 }),
        }),
      }
    }) as PluginInput["$"],
    worktree: "/tmp/worktree",
  } as unknown as PluginInput
  return { input, captured }
}

function toolCtx(sessionID: string) {
  return {
    sessionID,
    messageID: "m1",
    agent: "test",
    directory: "/tmp",
    worktree: "/tmp",
    abort: new AbortController().signal,
    metadata: () => {},
    ask: async () => {},
  }
}

const boardReview = {
  kagan: { status: "review", boardTask: true, worktree: "/wt", description: "Do the thing" },
}

describe("kagan server — session.created", () => {
  test("bootstraps lastGatedStatus but does not spawn intake directly", async () => {
    const store: Record<string, SessionData> = {
      s1: { title: "Add retry", metadata: { kagan: { status: "backlog", boardTask: true } } },
    }
    const { input, captured } = makeInput({ store, createId: "intake-1" })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: {
        type: "session.created",
        properties: {
          info: { id: "s1", title: "Add retry", metadata: { kagan: { status: "backlog", boardTask: true } } },
        },
      },
    } as never)
    expect(captured.creates).toHaveLength(0)
    const patch = captured.updates.find((u) => u.id === "s1")
    expect(patch?.kagan.lastGatedStatus).toBe("backlog")
  })

  test("spawns intake once the induced session.updated is delivered", async () => {
    const store: Record<string, SessionData> = {
      s1: { title: "Add retry", metadata: { kagan: { status: "backlog", boardTask: true } } },
    }
    const { input, captured } = makeInput({ store, createId: "intake-1" })
    const hooks = await plugin.server(input, {})
    const info = { id: "s1", title: "Add retry", metadata: { kagan: { status: "backlog", boardTask: true } } }
    await hooks.event?.({ event: { type: "session.created", properties: { info } } } as never)
    await hooks.event?.({ event: { type: "session.updated", properties: { info } } } as never)
    expect((captured.creates[0]?.metadata as { kagan?: { intakeParent?: string } })?.kagan?.intakeParent).toBe("s1")
    expect(captured.prompts[0]?.body.tools).toEqual({
      read: true,
      edit: false,
      write: false,
      bash: false,
      kagan_intake: true,
    })
    const patch = captured.updates.find((u) => u.kagan.intakeSessionID === "intake-1")
    expect(patch?.kagan.intakeOutcome).toBe("pending")
  })

  test("skips non-board sessions", async () => {
    const { input, captured } = makeInput()
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: {
        type: "session.created",
        properties: { info: { id: "s1", metadata: { kagan: { status: "backlog" } } } },
      },
    } as never)
    expect(captured.creates).toHaveLength(0)
  })

  test("skips intake/validator child sessions to avoid recursion", async () => {
    const { input, captured } = makeInput()
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: {
        type: "session.created",
        properties: { info: { id: "intake-1", metadata: { kagan: { role: "intake", intakeParent: "s1" } } } },
      },
    } as never)
    expect(captured.creates).toHaveLength(0)
  })

  test("does not re-spawn intake once an outcome exists", async () => {
    const { input, captured } = makeInput()
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: {
        type: "session.created",
        properties: {
          info: { id: "s1", metadata: { kagan: { status: "backlog", boardTask: true, intakeOutcome: "ran" } } },
        },
      },
    } as never)
    expect(captured.creates).toHaveLength(0)
  })

  test("degrades to intakeOutcome failed when intake spawn yields no child id", async () => {
    const store: Record<string, SessionData> = { s1: { metadata: { kagan: { status: "backlog", boardTask: true } } } }
    const { input, captured } = makeInput({ store, createId: undefined })
    const hooks = await plugin.server(input, {})
    const info = { id: "s1", metadata: { kagan: { status: "backlog", boardTask: true } } }
    await hooks.event?.({ event: { type: "session.created", properties: { info } } } as never)
    await hooks.event?.({ event: { type: "session.updated", properties: { info } } } as never)
    const failed = captured.updates.find((u) => u.kagan.intakeOutcome === "failed")
    expect(failed).toBeDefined()
  })
})

describe("kagan server — task references", () => {
  const referenced: ListedSession = {
    id: "ref-3",
    title: "Add auth",
    metadata: {
      kagan: {
        status: "done",
        boardTask: true,
        taskNumber: 3,
        intake: { understanding: "Added auth middleware.", decisions: [] },
        report: "Wired the middleware and added tests.",
      },
    },
  }

  test("injects the referenced task summary into the intake prompt", async () => {
    const store: Record<string, SessionData> = {
      s1: {
        title: "Extend auth",
        metadata: { kagan: { status: "backlog", boardTask: true, description: "Build on #3" } },
      },
    }
    const { input, captured } = makeInput({ store, createId: "intake-1", sessions: [referenced] })
    const hooks = await plugin.server(input, {})
    const info = {
      id: "s1",
      title: "Extend auth",
      metadata: { kagan: { status: "backlog", boardTask: true, description: "Build on #3" } },
    }
    await hooks.event?.({ event: { type: "session.created", properties: { info } } } as never)
    await hooks.event?.({ event: { type: "session.updated", properties: { info } } } as never)
    const parts = captured.prompts[0]?.body.parts as Array<{ text?: string }>
    expect(parts[0]?.text).toContain("## Referenced task #3 — Add auth (done)")
    expect(parts[0]?.text).toContain("Added auth middleware.")
    expect(parts[0]?.text).toContain("Wired the middleware and added tests.")
  })

  test("renders the not-found line for an unknown task number", async () => {
    const store: Record<string, SessionData> = {
      s1: { title: "T", metadata: { kagan: { status: "backlog", boardTask: true, description: "see #99" } } },
    }
    const { input, captured } = makeInput({ store, createId: "intake-1", sessions: [] })
    const hooks = await plugin.server(input, {})
    const info = {
      id: "s1",
      title: "T",
      metadata: { kagan: { status: "backlog", boardTask: true, description: "see #99" } },
    }
    await hooks.event?.({ event: { type: "session.created", properties: { info } } } as never)
    await hooks.event?.({ event: { type: "session.updated", properties: { info } } } as never)
    const parts = captured.prompts[0]?.body.parts as Array<{ text?: string }>
    expect(parts[0]?.text).toContain("(#99 not found)")
  })

  test("injects the referenced task summary into the auto-start prompt", async () => {
    const { input, captured } = makeInput({ sessions: [referenced] })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: {
        type: "session.updated",
        properties: {
          info: {
            id: "s1",
            title: "Extend auth",
            metadata: {
              kagan: {
                status: "in_progress",
                boardTask: true,
                worktree: "/wt",
                intakeOutcome: "ran",
                description: "Build on #3",
                lastGatedStatus: "backlog",
              },
            },
          },
        },
      },
    } as never)
    const parts = captured.prompts[0]?.body.parts as Array<{ text?: string }>
    expect(parts[0]?.text).toContain("## Referenced task #3 — Add auth (done)")
  })
})

describe("kagan server — column moves", () => {
  test("reverts an unapproved move to done", async () => {
    const { input, captured } = makeInput({ sessions: [] })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: {
        type: "session.updated",
        properties: { info: { id: "s1", metadata: { kagan: { status: "done", lastGatedStatus: "in_progress" } } } },
      },
    } as never)
    const revert = captured.updates.find((u) => u.kagan.status === "in_progress")
    expect(revert).toBeDefined()
  })

  test("reverts a move to in_progress that exceeds the WIP cap", async () => {
    const { input, captured } = makeInput({
      sessions: [
        { id: "a", metadata: { kagan: { status: "in_progress" } } },
        { id: "b", metadata: { kagan: { status: "in_progress" } } },
      ],
    })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: {
        type: "session.updated",
        properties: {
          info: {
            id: "s1",
            metadata: {
              kagan: {
                status: "in_progress",
                boardTask: true,
                worktree: "/wt",
                intakeOutcome: "ran",
                lastGatedStatus: "backlog",
              },
            },
          },
        },
      },
    } as never)
    const revert = captured.updates.find((u) => u.kagan.status === "backlog")
    expect(revert).toBeDefined()
    // Task sessions live in worktrees, so the WIP cap must scope the list to the project or it undercounts.
    expect(captured.listCalls).toEqual([{ query: { scope: "project" }, throwOnError: true }])
  })

  test("respects an inProgressLimit option override", async () => {
    const { input, captured } = makeInput({
      sessions: [
        { id: "a", metadata: { kagan: { status: "in_progress" } } },
        { id: "b", metadata: { kagan: { status: "in_progress" } } },
      ],
    })
    const hooks = await plugin.server(input, { inProgressLimit: 3 })
    await hooks.event?.({
      event: {
        type: "session.updated",
        properties: {
          info: {
            id: "s1",
            metadata: {
              kagan: {
                status: "in_progress",
                boardTask: true,
                worktree: "/wt",
                intakeOutcome: "ran",
                lastGatedStatus: "backlog",
              },
            },
          },
        },
      },
    } as never)
    const revert = captured.updates.find((u) => u.kagan.status === "backlog")
    expect(revert).toBeUndefined()
  })

  test("auto-starts the root session once on first in_progress entry", async () => {
    const { input, captured } = makeInput({ sessions: [] })
    const model = { providerID: "anthropic", modelID: "claude-opus-4" }
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: {
        type: "session.updated",
        properties: {
          info: {
            id: "s1",
            title: "Task title",
            metadata: {
              kagan: {
                status: "in_progress",
                boardTask: true,
                worktree: "/wt",
                intakeOutcome: "ran",
                description: "Do the actual work here",
                model,
                lastGatedStatus: "backlog",
              },
            },
          },
        },
      },
    } as never)
    const started = captured.updates.find((u) => typeof u.kagan.startedAt === "number")
    expect(started).toBeDefined()
    expect(captured.prompts).toHaveLength(1)
    expect(captured.prompts[0]?.id).toBe("s1")
    const parts = captured.prompts[0]?.body.parts as Array<{ text?: string }>
    expect(parts[0]?.text).toContain("Do the actual work here")
    expect(captured.prompts[0]?.body.model).toEqual(model)
  })

  test("does not auto-start when startedAt is already set", async () => {
    const { input, captured } = makeInput({ sessions: [] })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: {
        type: "session.updated",
        properties: {
          info: {
            id: "s1",
            metadata: {
              kagan: {
                status: "in_progress",
                boardTask: true,
                worktree: "/wt",
                intakeOutcome: "ran",
                startedAt: 1,
                lastGatedStatus: "backlog",
              },
            },
          },
        },
      },
    } as never)
    expect(captured.prompts).toHaveLength(0)
  })

  test("reverts to backlog when the auto-start promptAsync fails, and a later move retries", async () => {
    const { input, captured } = makeInput({
      sessions: [],
      promptError: () => new Error("ProviderModelNotFoundError: bad model"),
    })
    const hooks = await plugin.server(input, {})

    const inProgressInfo = {
      id: "s1",
      title: "Task title",
      metadata: {
        kagan: {
          status: "in_progress",
          boardTask: true,
          worktree: "/wt",
          intakeOutcome: "ran",
          description: "Do the actual work here",
          lastGatedStatus: "backlog",
        },
      },
    }
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: inProgressInfo } },
    } as never)

    expect(captured.prompts).toHaveLength(1)
    const revert = captured.updates.find((u) => u.id === "s1" && u.kagan.status === "backlog")
    expect(revert).toBeDefined()
    expect(revert?.kagan).toMatchObject({ startedAt: undefined, status: "backlog", lastGatedStatus: "backlog" })

    // Simulate the board reflecting the reverted state, then a fresh move to in_progress.
    const retryInfo = {
      id: "s1",
      title: "Task title",
      metadata: {
        kagan: {
          status: "in_progress",
          boardTask: true,
          worktree: "/wt",
          intakeOutcome: "ran",
          description: "Do the actual work here",
          lastGatedStatus: "backlog",
        },
      },
    }
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: retryInfo } },
    } as never)

    expect(captured.prompts).toHaveLength(2)
  })

  test("gates a column move on the very first event when lastGatedStatus already differs (restart survival)", async () => {
    const { input, captured } = makeInput({
      sessions: [
        { id: "a", metadata: { kagan: { status: "in_progress" } } },
        { id: "b", metadata: { kagan: { status: "in_progress" } } },
      ],
    })
    const hooks = await plugin.server(input, {})
    // No session.created was ever delivered to this instance (simulating a plugin restart);
    // the incoming metadata already carries a lastGatedStatus from before the restart.
    await hooks.event?.({
      event: {
        type: "session.updated",
        properties: {
          info: {
            id: "s1",
            metadata: {
              kagan: {
                status: "in_progress",
                boardTask: true,
                worktree: "/wt",
                intakeOutcome: "ran",
                lastGatedStatus: "backlog",
              },
            },
          },
        },
      },
    } as never)
    const revert = captured.updates.find((u) => u.kagan.status === "backlog")
    expect(revert).toBeDefined()
  })
})

describe("kagan server — review entry", () => {
  test("spawns a validator once when a board task enters review", async () => {
    const store: Record<string, SessionData> = { s1: { title: "Task", metadata: boardReview } }
    const { input, captured } = makeInput({ store, createId: "validator-1" })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata: boardReview } } },
    } as never)
    const validatorCreate = captured.creates.find(
      (c) => (c.metadata as { kagan?: { role?: string } })?.kagan?.role === "validator",
    )
    expect(validatorCreate).toBeDefined()
    const patch = captured.updates.find((u) => u.kagan.validatorSessionID === "validator-1")
    expect(patch).toBeDefined()
  })

  test("with check commands configured, patches check evidence and includes it in the validator prompt", async () => {
    const metadata = { kagan: { ...boardReview.kagan, worktree: "/tmp" } }
    const store: Record<string, SessionData> = { s1: { title: "Task", metadata } }
    const { input, captured } = makeInput({
      store,
      createId: "validator-1",
      gitStdout: (args) => {
        if (args[0] === "merge-base") return "abc123"
        if (args.includes("--numstat")) return "1\t1\tfile.txt"
        if (args.includes("--name-status")) return "M\tfile.txt"
        return ""
      },
    })
    const hooks = await plugin.server(input, {
      commands: { check: [{ name: "check", cwd: ".", command: "echo check-ok" }] },
    })
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata } } },
    } as never)

    const checkPatch = captured.updates.find((u) => u.kagan.check)
    expect(checkPatch?.kagan.check).toMatchObject({
      command: "check: echo check-ok",
      exitCode: 0,
      steps: [{ name: "check", cwd: ".", command: "echo check-ok", status: "ran", exitCode: 0, output: "check-ok\n" }],
    })

    const validatorPrompt = captured.prompts.find((p) => p.id === "validator-1")
    const text = (validatorPrompt?.body.parts as Array<{ text?: string }>)?.[0]?.text
    expect(text).toContain("Deterministic check evidence")
    expect(text).toContain("`echo check-ok` exited 0")
    expect(text).toContain("check-ok")
  })

  test("scoped check commands run from changed cwd or repo-relative scope matches", async () => {
    const metadata = { kagan: { ...boardReview.kagan, worktree: "/", baseBranch: "main" } }
    const store: Record<string, SessionData> = { s1: { title: "Task", metadata } }
    const { input, captured } = makeInput({
      store,
      createId: "validator-1",
      gitStdout: (args) => {
        if (args[0] === "merge-base") return "abc123"
        if (args.includes("--numstat")) return "1\t1\ttmp/file.txt\n1\t0\t.github/workflows/check.yml"
        if (args.includes("--name-status")) return "M\ttmp/file.txt\nM\t.github/workflows/check.yml"
        return ""
      },
    })
    const hooks = await plugin.server(input, {
      commands: {
        check: [
          { name: "member", cwd: "tmp", command: "echo member" },
          { name: "workflow", cwd: "var", command: "echo workflow", scope: ["^\\.github/"] },
        ],
      },
    })
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata } } },
    } as never)

    const checkPatch = captured.updates.find((u) => u.kagan.check)
    expect((checkPatch?.kagan.check as { steps?: unknown[] }).steps).toEqual([
      { name: "member", cwd: "tmp", command: "echo member", status: "ran", exitCode: 0, output: "member\n" },
      { name: "workflow", cwd: "var", command: "echo workflow", status: "ran", exitCode: 0, output: "workflow\n" },
    ])
  })

  test("with a failing check command, still spawns the validator and records the failure", async () => {
    const metadata = { kagan: { ...boardReview.kagan, worktree: "/tmp" } }
    const store: Record<string, SessionData> = { s1: { title: "Task", metadata } }
    const { input, captured } = makeInput({
      store,
      createId: "validator-1",
      gitStdout: (args) => {
        if (args[0] === "merge-base") return "abc123"
        if (args.includes("--numstat")) return "1\t1\tfile.txt"
        if (args.includes("--name-status")) return "M\tfile.txt"
        return ""
      },
    })
    const hooks = await plugin.server(input, { commands: { check: [{ name: "check", cwd: ".", command: "exit 3" }] } })
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata } } },
    } as never)

    const checkPatch = captured.updates.find((u) => u.kagan.check)
    expect(checkPatch?.kagan.check).toMatchObject({ command: "check: exit 3", exitCode: 3 })

    const validatorCreate = captured.creates.find(
      (c) => (c.metadata as { kagan?: { role?: string } })?.kagan?.role === "validator",
    )
    expect(validatorCreate).toBeDefined()
  })

  test("with no check commands configured, review entry behaves unchanged", async () => {
    const store: Record<string, SessionData> = { s1: { title: "Task", metadata: boardReview } }
    const { input, captured } = makeInput({ store, createId: "validator-1" })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata: boardReview } } },
    } as never)
    expect(captured.updates.some((u) => u.kagan.check)).toBe(false)
    const validatorCreate = captured.creates.find(
      (c) => (c.metadata as { kagan?: { role?: string } })?.kagan?.role === "validator",
    )
    expect(validatorCreate).toBeDefined()
  })

  test("a check-command error does not abort review entry", async () => {
    const metadata = { kagan: { ...boardReview.kagan, worktree: "/definitely-missing-worktree" } }
    const store: Record<string, SessionData> = { s1: { title: "Task", metadata } }
    const { input, captured } = makeInput({
      store,
      createId: "validator-1",
      gitStdout: (args) => {
        if (args[0] === "merge-base") return "abc123"
        if (args.includes("--numstat")) return "1\t1\tfile.txt"
        if (args.includes("--name-status")) return "M\tfile.txt"
        return ""
      },
    })
    const hooks = await plugin.server(input, {
      commands: { check: [{ name: "check", cwd: ".", command: "echo never-runs" }] },
    })
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata } } },
    } as never)

    const checkPatch = captured.updates.find((u) => u.kagan.check)
    expect(checkPatch?.kagan.check).toMatchObject({ exitCode: null })

    const validatorCreate = captured.creates.find(
      (c) => (c.metadata as { kagan?: { role?: string } })?.kagan?.role === "validator",
    )
    expect(validatorCreate).toBeDefined()
    const sessionPatch = captured.updates.find((u) => u.kagan.validatorSessionID === "validator-1")
    expect(sessionPatch).toBeDefined()
  })

  test("a check-evidence patch failure does not abort validator spawn", async () => {
    const metadata = { kagan: { ...boardReview.kagan, worktree: "/tmp" } }
    const store: Record<string, SessionData> = { s1: { title: "Task", metadata } }
    const { input, captured } = makeInput({
      store,
      createId: "validator-1",
      gitStdout: (args) => {
        if (args[0] === "merge-base") return "abc123"
        if (args.includes("--numstat")) return "1\t1\tfile.txt"
        if (args.includes("--name-status")) return "M\tfile.txt"
        return ""
      },
      updateError: (_id, kagan) => (kagan.check ? new Error("metadata write failed") : undefined),
    })
    const hooks = await plugin.server(input, {
      commands: { check: [{ name: "check", cwd: ".", command: "echo check-ok" }] },
    })

    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata } } },
    } as never)

    expect(captured.updates.some((update) => update.kagan.check)).toBe(false)
    expect(
      captured.creates.some(
        (create) => (create.metadata as { kagan?: { role?: string } })?.kagan?.role === "validator",
      ),
    ).toBe(true)
    expect(captured.prompts.find((prompt) => prompt.id === "validator-1")).toBeDefined()
  })

  test("stamps validatorOutcome failed when the validator spawn returns no id", async () => {
    const store: Record<string, SessionData> = { s1: { title: "Task", metadata: boardReview } }
    const { input, captured } = makeInput({ store, createId: undefined })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata: boardReview } } },
    } as never)
    const failed = captured.updates.find((u) => u.kagan.validatorOutcome === "failed")
    expect(failed).toBeDefined()
  })

  test("passes prior triage into the validator prompt", async () => {
    const metadata = {
      kagan: {
        ...boardReview.kagan,
        priorTriage: [
          {
            id: "f1",
            summary: "Keep the sync audit write",
            category: "bug",
            resolution: "intended",
            note: "The shutdown path must preserve audit ordering.",
          },
        ],
      },
    }
    const store: Record<string, SessionData> = { s1: { title: "Task", metadata } }
    const { input, captured } = makeInput({ store, createId: "validator-1" })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata } } },
    } as never)
    const parts = captured.prompts[0]?.body.parts as Array<{ text?: string }>
    expect(parts[0]?.text).toContain("Do not re-report them or close variations of them")
    expect(parts[0]?.text).toContain("[bug] Keep the sync audit write — ruled intended")
  })

  test("skips the validator when one is already recorded", async () => {
    const metadata = { kagan: { ...boardReview.kagan, validatorSessionID: "validator-0" } }
    const store: Record<string, SessionData> = { s1: { title: "Task", metadata } }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata } } },
    } as never)
    const validatorCreate = captured.creates.find(
      (c) => (c.metadata as { kagan?: { role?: string } })?.kagan?.role === "validator",
    )
    expect(validatorCreate).toBeUndefined()
  })

  test("skips the validator for non-board sessions", async () => {
    const metadata = { kagan: { status: "review" } }
    const store: Record<string, SessionData> = { s1: { metadata } }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata } } },
    } as never)
    expect(captured.creates).toHaveLength(0)
  })
})

describe("kagan server — session.idle", () => {
  const activeRoot = { kagan: { status: "in_progress", boardTask: true, worktree: "/wt", startedAt: 1 } }

  test("moves the root to review when the idle root session is the active iteration", async () => {
    const store: Record<string, SessionData> = { s1: { metadata: activeRoot } }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({ event: { type: "session.idle", properties: { sessionID: "s1" } } } as never)
    const patch = captured.updates.find((u) => u.kagan.status === "review")
    expect(patch?.id).toBe("s1")
  })

  test("captures the idle iteration's final report into the root summary on review entry", async () => {
    const store: Record<string, SessionData> = { s1: { metadata: activeRoot } }
    const messages = {
      s1: [
        { info: { role: "user" }, parts: [{ type: "text", text: "do it" }] },
        { info: { role: "assistant" }, parts: [{ type: "text", text: "Refactored the parser and added tests." }] },
      ],
    }
    const { input, captured } = makeInput({ store, messages })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({ event: { type: "session.idle", properties: { sessionID: "s1" } } } as never)
    const patch = captured.updates.find((u) => u.kagan.status === "review")
    expect(patch?.kagan.report).toBe("Refactored the parser and added tests.")
  })

  test("moves the root to review when an idle worker is the active iteration", async () => {
    const store: Record<string, SessionData> = {
      "worker-1": { parentID: "root-1", metadata: { kagan: { role: "worker", workerParent: "root-1" } } },
      "root-1": { metadata: { kagan: { ...activeRoot.kagan, activeIteration: "worker-1" } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({ event: { type: "session.idle", properties: { sessionID: "worker-1" } } } as never)
    const patch = captured.updates.find((u) => u.kagan.status === "review")
    expect(patch?.id).toBe("root-1")
  })

  test("clears awaitingInput as a backstop when the active iteration goes idle", async () => {
    const withWait = {
      kagan: { ...activeRoot.kagan, awaitingInput: { id: "p1", title: "Run rm -rf?" } },
    }
    const store: Record<string, SessionData> = { s1: { metadata: withWait } }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({ event: { type: "session.idle", properties: { sessionID: "s1" } } } as never)
    const patch = captured.updates.find((u) => u.id === "s1")
    expect(patch?.kagan.status).toBe("review")
    expect(patch?.kagan.awaitingInput).toBeUndefined()
    expect("awaitingInput" in (patch?.kagan ?? {})).toBe(true)
  })

  test("does not move when the idle session is not the active iteration", async () => {
    const store: Record<string, SessionData> = {
      s1: { metadata: { kagan: { ...activeRoot.kagan, activeIteration: "some-other" } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({ event: { type: "session.idle", properties: { sessionID: "s1" } } } as never)
    expect(captured.updates).toHaveLength(0)
  })

  test("ignores intake and validator helper sessions", async () => {
    const store: Record<string, SessionData> = {
      v1: { parentID: "s1", metadata: { kagan: { role: "validator", validatorParent: "s1" } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({ event: { type: "session.idle", properties: { sessionID: "v1" } } } as never)
    expect(captured.updates).toHaveLength(0)
  })

  test("does not move a root whose work never started", async () => {
    const store: Record<string, SessionData> = {
      s1: { metadata: { kagan: { status: "in_progress", boardTask: true, worktree: "/wt" } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({ event: { type: "session.idle", properties: { sessionID: "s1" } } } as never)
    expect(captured.updates).toHaveLength(0)
  })
})

describe("kagan server — permission.updated / permission.replied", () => {
  test("stamps awaitingInput on a board-task session that owns the permission itself", async () => {
    const store: Record<string, SessionData> = {
      s1: { metadata: { kagan: { status: "in_progress", boardTask: true } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: {
        type: "permission.updated",
        properties: { id: "p1", type: "bash", sessionID: "s1", messageID: "m1", title: "Run rm -rf?", metadata: {} },
      },
    } as never)
    const patch = captured.updates.find((u) => u.id === "s1")
    expect(patch?.kagan.awaitingInput).toEqual({ id: "p1", title: "Run rm -rf?" })
  })

  test("resolves a worker child session's permission to the board-task parent", async () => {
    const store: Record<string, SessionData> = {
      "worker-1": { parentID: "root-1", metadata: { kagan: { role: "worker", workerParent: "root-1" } } },
      "root-1": { metadata: { kagan: { status: "in_progress", boardTask: true } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: {
        type: "permission.updated",
        properties: {
          id: "p2",
          type: "bash",
          sessionID: "worker-1",
          messageID: "m1",
          title: "Delete branch?",
          metadata: {},
        },
      },
    } as never)
    const patch = captured.updates.find((u) => u.id === "root-1")
    expect(patch?.kagan.awaitingInput).toEqual({ id: "p2", title: "Delete branch?" })
  })

  test("permission.replied clears awaitingInput on the owning board task", async () => {
    const store: Record<string, SessionData> = {
      s1: { metadata: { kagan: { status: "in_progress", boardTask: true, awaitingInput: { id: "p1", title: "x" } } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: {
        type: "permission.replied",
        properties: { sessionID: "s1", permissionID: "p1", response: "once" },
      },
    } as never)
    const patch = captured.updates.find((u) => u.id === "s1")
    expect(patch?.kagan.awaitingInput).toBeUndefined()
    expect("awaitingInput" in (patch?.kagan ?? {})).toBe(true)
  })

  test("an unresolvable session is a no-op, not a throw", async () => {
    const store: Record<string, SessionData> = { stray: { metadata: {} } }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: {
        type: "permission.updated",
        properties: { id: "p1", type: "bash", sessionID: "stray", messageID: "m1", title: "x", metadata: {} },
      },
    } as never)
    await hooks.event?.({
      event: { type: "permission.replied", properties: { sessionID: "stray", permissionID: "p1", response: "once" } },
    } as never)
    expect(captured.updates).toHaveLength(0)
  })
})

describe("kagan server — tool.execute.before (git push guard)", () => {
  function callGuard(hooks: Awaited<ReturnType<typeof plugin.server>>, command: string, tool = "bash") {
    return hooks["tool.execute.before"]!({ tool, sessionID: "s1", callID: "c1" }, { args: { command } })
  }

  test("denies a git push from a board-task session and mentions the board", async () => {
    const store: Record<string, SessionData> = {
      s1: { metadata: { kagan: { boardTask: true, status: "in_progress" } } },
    }
    const { input } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await expect(callGuard(hooks, "git push")).rejects.toThrow(/board/i)
  })

  test("denies a git push from a helper session identified only by a parent back-pointer", async () => {
    const store: Record<string, SessionData> = {
      s1: { parentID: "root-1", metadata: { kagan: { role: "worker", workerParent: "root-1" } } },
    }
    const { input } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await expect(callGuard(hooks, "git push origin HEAD")).rejects.toThrow(/board/i)
  })

  test("allows a git push from a generic OpenCode session", async () => {
    const store: Record<string, SessionData> = { s1: { metadata: {} } }
    const { input } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await expect(callGuard(hooks, "git push")).resolves.toBeUndefined()
  })

  test("allows non-push bash from a board task", async () => {
    const store: Record<string, SessionData> = {
      s1: { metadata: { kagan: { boardTask: true, status: "in_progress" } } },
    }
    const { input } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await expect(callGuard(hooks, "git status")).resolves.toBeUndefined()
  })

  test("ignores non-bash tool calls even if their args mention push", async () => {
    const store: Record<string, SessionData> = {
      s1: { metadata: { kagan: { boardTask: true, status: "in_progress" } } },
    }
    const { input } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await expect(callGuard(hooks, "git push", "read")).resolves.toBeUndefined()
  })
})

describe("kagan server — tools", () => {
  test("kagan_intake writes intake with refinedPrompt and ran outcome to the parent", async () => {
    const store: Record<string, SessionData> = {
      "child-1": { metadata: { kagan: { role: "intake", intakeParent: "parent-1" } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    const result = await hooks.tool!.kagan_intake!.execute(
      {
        understanding: "Adds a retry wrapper around the flaky webhook call.",
        decisions: [{ id: "d1", question: "Max retries?", assumption: "3", required: true }],
        refinedPrompt: "Wrap the webhook call in a 3-attempt retry with backoff.",
      },
      toolCtx("child-1"),
    )
    const patch = captured.updates.find((u) => u.id === "parent-1")
    expect(patch?.kagan.intake).toEqual({
      understanding: "Adds a retry wrapper around the flaky webhook call.",
      decisions: [{ id: "d1", question: "Max retries?", assumption: "3", required: true }],
      refinedPrompt: "Wrap the webhook call in a 3-attempt retry with backoff.",
    })
    expect(patch?.kagan.intakeOutcome).toBe("ran")
    expect(patch?.kagan.helperError).toBeUndefined()
    expect(result).toEqual({ output: "Recorded intake with 1 decision(s)." })
  })

  test("kagan_intake persists a sanitized mode recommendation", async () => {
    const store: Record<string, SessionData> = {
      "child-1": { metadata: { kagan: { role: "intake", intakeParent: "parent-1" } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.tool!.kagan_intake!.execute(
      {
        understanding: "x",
        decisions: [],
        refinedPrompt: "y",
        mode: { recommended: "assisted", rationale: "No check and high blast radius." },
      },
      toolCtx("child-1"),
    )
    const patch = captured.updates.find((u) => u.id === "parent-1")
    expect(patch?.kagan.intake).toEqual({
      understanding: "x",
      decisions: [],
      refinedPrompt: "y",
      mode: { recommended: "assisted", rationale: "No check and high blast radius." },
    })
  })

  test("kagan_intake drops a malformed mode before persisting", async () => {
    const store: Record<string, SessionData> = {
      "child-1": { metadata: { kagan: { role: "intake", intakeParent: "parent-1" } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.tool!.kagan_intake!.execute(
      {
        understanding: "x",
        decisions: [],
        refinedPrompt: "y",
        mode: { recommended: "full-auto", rationale: "Bad value." },
      },
      toolCtx("child-1"),
    )
    const patch = captured.updates.find((u) => u.id === "parent-1")
    expect(patch?.kagan.intake).toEqual({ understanding: "x", decisions: [], refinedPrompt: "y" })
    expect("mode" in ((patch?.kagan.intake ?? {}) as Record<string, unknown>)).toBe(false)
  })

  test("kagan_intake rejects calls outside an intake session", async () => {
    const { input } = makeInput({ store: { "not-intake": { metadata: {} } } })
    const hooks = await plugin.server(input, {})
    await expect(
      hooks.tool!.kagan_intake!.execute(
        { understanding: "x", decisions: [], refinedPrompt: "y" },
        toolCtx("not-intake"),
      ),
    ).rejects.toThrow("kagan_intake is only available in intake sessions")
  })

  test("kagan_findings writes findings with ran outcome to the parent", async () => {
    const store: Record<string, SessionData> = {
      "child-1": { metadata: { kagan: { role: "validator", validatorParent: "parent-1" } } },
      "parent-1": { metadata: { kagan: { validatorSessionID: "child-1" } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    const findings = [{ id: "f1", summary: "Race in refresh", category: "bug", confidence: 7 }]
    const result = await hooks.tool!.kagan_findings!.execute({ findings }, toolCtx("child-1"))
    const patch = captured.updates.find((u) => u.id === "parent-1")
    expect(patch?.kagan.findings).toEqual(findings)
    expect(patch?.kagan.validatorOutcome).toBe("ran")
    expect(patch?.kagan.helperError).toBeUndefined()
    expect(result).toEqual({ output: "Recorded 1 finding(s)." })
  })

  test("kagan_findings writes findings with detail and location through to the parent", async () => {
    const store: Record<string, SessionData> = {
      "child-1": { metadata: { kagan: { role: "validator", validatorParent: "parent-1" } } },
      "parent-1": { metadata: { kagan: { validatorSessionID: "child-1" } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    const findings = [
      {
        id: "f1",
        summary: "Race in refresh",
        detail: "Two refresh calls can both pass the debounce guard because `seen` is read before it settles.",
        location: "src/store.ts:142",
        category: "bug",
        confidence: 7,
      },
    ]
    await hooks.tool!.kagan_findings!.execute({ findings }, toolCtx("child-1"))
    const patch = captured.updates.find((u) => u.id === "parent-1")
    expect(patch?.kagan.findings).toEqual(findings)
  })

  test("kagan_findings rejects calls outside a validator session", async () => {
    const { input } = makeInput({ store: { "not-validator": { metadata: {} } } })
    const hooks = await plugin.server(input, {})
    await expect(hooks.tool!.kagan_findings!.execute({ findings: [] }, toolCtx("not-validator"))).rejects.toThrow(
      "kagan_findings is only available in validator sessions",
    )
  })

  test("kagan_findings caps confidence and marks outOfDiff for a citation the diff can't verify, leaving in-diff and location-less findings untouched", async () => {
    const store: Record<string, SessionData> = {
      "child-1": { metadata: { kagan: { role: "validator", validatorParent: "parent-1" } } },
      "parent-1": { metadata: { kagan: { validatorSessionID: "child-1", worktree: "/wt", baseBranch: "main" } } },
    }
    const gitStdout = (args: string[]) => {
      if (args[0] === "merge-base") return "basesha\n"
      if (args.includes("--numstat")) return "1\t0\tsrc/a.ts\n"
      if (args.includes("--name-status")) return "M\tsrc/a.ts\n"
      if (args.includes("--patch"))
        return "diff --git a/src/a.ts b/src/a.ts\n@@ -1,2 +1,3 @@\n context\n-old\n+new\n+extra\n"
      return ""
    }
    const { input, captured } = makeInput({ store, gitStdout })
    const hooks = await plugin.server(input, {})
    const findings = [
      { id: "f1", summary: "in diff", location: "src/a.ts:2", category: "bug", confidence: 8 },
      { id: "f2", summary: "no location", category: "uncertainty", confidence: 5 },
      { id: "f3", summary: "out of hunk range", location: "src/a.ts:50", category: "bug", confidence: 9 },
      { id: "f4", summary: "unknown file", location: "src/missing.ts:1", category: "bug", confidence: 9 },
    ]
    await hooks.tool!.kagan_findings!.execute({ findings }, toolCtx("child-1"))
    const patch = captured.updates.find((u) => u.id === "parent-1" && u.kagan.validatorOutcome === "ran")
    expect(patch?.kagan.findings).toEqual([
      findings[0],
      findings[1],
      { ...findings[2], confidence: 2, outOfDiff: true },
      { ...findings[3], confidence: 2, outOfDiff: true },
    ])
  })

  test("kagan_findings persists findings unmodified when the parent task has no worktree to diff", async () => {
    const store: Record<string, SessionData> = {
      "child-1": { metadata: { kagan: { role: "validator", validatorParent: "parent-1" } } },
      "parent-1": { metadata: { kagan: { validatorSessionID: "child-1" } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    const findings = [{ id: "f1", summary: "s", location: "src/missing.ts:1", category: "bug", confidence: 9 }]
    await hooks.tool!.kagan_findings!.execute({ findings }, toolCtx("child-1"))
    const patch = captured.updates.find((u) => u.id === "parent-1" && u.kagan.validatorOutcome === "ran")
    expect(patch?.kagan.findings).toEqual(findings)
  })

  test("kagan_findings does not persist findings from a superseded validator session", async () => {
    const store: Record<string, SessionData> = {
      "child-1": { metadata: { kagan: { role: "validator", validatorParent: "parent-1" } } },
      "parent-1": { metadata: { kagan: { validatorSessionID: "child-2" } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    const findings = [{ id: "f1", summary: "stale", category: "bug", confidence: 9 }]
    const result = await hooks.tool!.kagan_findings!.execute({ findings }, toolCtx("child-1"))
    expect(captured.updates.find((u) => u.id === "parent-1")).toBeUndefined()
    expect(result).toEqual({ output: "Findings not recorded — this review was superseded by a newer iteration." })
  })
})

describe("kagan server — helper failure: spawn-time throw", () => {
  test("intake: auto-retries once within helperRetries, then fails permanently and stays intake-ready", async () => {
    let creates = 0
    const createId = () => {
      creates++
      return `intake-${creates}`
    }
    const store: Record<string, SessionData> = {
      s1: { title: "T", metadata: { kagan: { status: "backlog", boardTask: true } } },
    }
    const { input, captured } = makeInput({
      store,
      createId,
      promptError: (id) => (id.startsWith("intake-") ? new Error("ProviderModelNotFoundError: bad model") : undefined),
    })
    const hooks = await plugin.server(input, {})

    const createdInfo = { id: "s1", title: "T", metadata: { kagan: { status: "backlog", boardTask: true } } }
    await hooks.event?.({ event: { type: "session.created", properties: { info: createdInfo } } } as never)
    await hooks.event?.({ event: { type: "session.updated", properties: { info: createdInfo } } } as never)

    const retryPatch = captured.updates.find(
      (u) => u.id === "s1" && u.kagan.intakeSessionID === undefined && u.kagan.intakeAttempts === 1,
    )
    expect(retryPatch).toBeDefined()
    expect(creates).toBe(1)

    // The host would deliver a fresh session.updated after that patch; simulate it to drive the retry.
    await hooks.event?.({
      event: {
        type: "session.updated",
        properties: {
          info: {
            id: "s1",
            title: "T",
            metadata: { kagan: { status: "backlog", boardTask: true, intakeAttempts: 1 } },
          },
        },
      },
    } as never)

    expect(creates).toBe(2)
    const failedPatch = captured.updates.find((u) => u.id === "s1" && u.kagan.intakeOutcome === "failed")
    expect(failedPatch).toBeDefined()
    expect(failedPatch?.kagan.helperError).toEqual({
      role: "intake",
      message: "ProviderModelNotFoundError: bad model",
    })

    const finalMetadata = { kagan: { status: "backlog", boardTask: true, ...failedPatch?.kagan } }
    expect(intakeReady(finalMetadata)).toBe(true)
  })

  test("validator: auto-retries once within helperRetries, then fails permanently and stays approvable", async () => {
    let creates = 0
    const createId = () => {
      creates++
      return `validator-${creates}`
    }
    const boardReviewStore: Record<string, SessionData> = {
      s1: { title: "Task", metadata: boardReview },
    }
    const { input, captured } = makeInput({
      store: boardReviewStore,
      createId,
      promptError: (id) =>
        id.startsWith("validator-") ? new Error("ProviderModelNotFoundError: bad model") : undefined,
    })
    const hooks = await plugin.server(input, {})

    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata: boardReview } } },
    } as never)

    const retryPatch = captured.updates.find(
      (u) => u.id === "s1" && u.kagan.validatorSessionID === undefined && u.kagan.validatorAttempts === 1,
    )
    expect(retryPatch).toBeDefined()
    expect(creates).toBe(1)

    // onEnterReview re-fetches the session by id, so reflect the retry-cleared state in the store
    // before the host's follow-up session.updated (simulated below) drives the automatic respawn.
    boardReviewStore.s1 = { title: "Task", metadata: { kagan: { ...boardReview.kagan, validatorAttempts: 1 } } }
    await hooks.event?.({
      event: {
        type: "session.updated",
        properties: { info: { id: "s1", metadata: { kagan: { ...boardReview.kagan, validatorAttempts: 1 } } } },
      },
    } as never)

    expect(creates).toBe(2)
    const failedPatch = captured.updates.find((u) => u.id === "s1" && u.kagan.validatorOutcome === "failed")
    expect(failedPatch).toBeDefined()
    expect(failedPatch?.kagan.helperError).toEqual({
      role: "validator",
      message: "ProviderModelNotFoundError: bad model",
    })

    const finalMetadata = { kagan: { ...boardReview.kagan, ...failedPatch?.kagan } }
    expect(helper(finalMetadata, "validator").outcome).toBe("failed")
    expect(approveDenyReason(finalMetadata)).toBeUndefined()
  })

  test("respects a helperRetries option of 0 by failing on the very first spawn failure", async () => {
    const store: Record<string, SessionData> = {
      s1: { title: "T", metadata: { kagan: { status: "backlog", boardTask: true } } },
    }
    const { input, captured } = makeInput({
      store,
      createId: "intake-1",
      promptError: () => new Error("boom"),
    })
    const hooks = await plugin.server(input, { helperRetries: 0 })
    const info = { id: "s1", title: "T", metadata: { kagan: { status: "backlog", boardTask: true } } }
    await hooks.event?.({ event: { type: "session.created", properties: { info } } } as never)
    await hooks.event?.({ event: { type: "session.updated", properties: { info } } } as never)
    const failedPatch = captured.updates.find((u) => u.id === "s1" && u.kagan.intakeOutcome === "failed")
    expect(failedPatch).toBeDefined()
    expect(captured.creates).toHaveLength(1)
  })
})

describe("kagan server — helper failure: session.error event", () => {
  const runningValidator = {
    kagan: {
      status: "review",
      boardTask: true,
      worktree: "/wt",
      validatorSessionID: "v1",
      validatorOutcome: "pending",
      validatorAttempts: 1,
    },
  }

  test("with helperRetries 0, fails immediately and records the extracted message", async () => {
    const store: Record<string, SessionData> = {
      s1: { metadata: runningValidator },
      v1: { parentID: "s1", metadata: { kagan: { role: "validator", validatorParent: "s1" } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, { helperRetries: 0 })
    await hooks.event?.({
      event: {
        type: "session.error",
        properties: {
          sessionID: "v1",
          error: { name: "ProviderAuthError", data: { providerID: "anthropic", message: "invalid api key" } },
        },
      },
    } as never)
    const patch = captured.updates.find((u) => u.id === "s1")
    expect(patch?.kagan.validatorOutcome).toBe("failed")
    expect(patch?.kagan.helperError).toEqual({ role: "validator", message: "invalid api key" })
  })

  test("falls back to the error name when the payload has no data.message", async () => {
    const store: Record<string, SessionData> = {
      s1: { metadata: runningValidator },
      v1: { parentID: "s1", metadata: { kagan: { role: "validator", validatorParent: "s1" } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, { helperRetries: 0 })
    await hooks.event?.({
      event: {
        type: "session.error",
        properties: { sessionID: "v1", error: { name: "MessageOutputLengthError", data: { tokens: 999999 } } },
      },
    } as never)
    const patch = captured.updates.find((u) => u.id === "s1")
    expect(patch?.kagan.helperError).toEqual({ role: "validator", message: "MessageOutputLengthError" })
  })

  test("falls back to 'unknown error' when the payload is missing entirely", async () => {
    const store: Record<string, SessionData> = {
      s1: { metadata: runningValidator },
      v1: { parentID: "s1", metadata: { kagan: { role: "validator", validatorParent: "s1" } } },
    }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, { helperRetries: 0 })
    await hooks.event?.({
      event: { type: "session.error", properties: { sessionID: "v1" } },
    } as never)
    const patch = captured.updates.find((u) => u.id === "s1")
    expect(patch?.kagan.helperError).toEqual({ role: "validator", message: "unknown error" })
  })

  test("ignores errors for sessions with no role and events with no sessionID", async () => {
    const store: Record<string, SessionData> = { s1: { metadata: { kagan: { status: "in_progress" } } } }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: { type: "session.error", properties: { sessionID: "s1", error: { name: "UnknownError" } } },
    } as never)
    await hooks.event?.({ event: { type: "session.error", properties: {} } } as never)
    expect(captured.updates).toHaveLength(0)
  })
})

describe("kagan server — helper event funnel treats intake and validator identically", () => {
  const cases: Array<{
    role: "intake" | "validator"
    sessionField: "intakeSessionID" | "validatorSessionID"
    outcomeField: "intakeOutcome" | "validatorOutcome"
    attemptsField: "intakeAttempts" | "validatorAttempts"
    parentField: "intakeParent" | "validatorParent"
    idleMessage: string
  }> = [
    {
      role: "intake",
      sessionField: "intakeSessionID",
      outcomeField: "intakeOutcome",
      attemptsField: "intakeAttempts",
      parentField: "intakeParent",
      idleMessage: "intake finished without recording an assessment",
    },
    {
      role: "validator",
      sessionField: "validatorSessionID",
      outcomeField: "validatorOutcome",
      attemptsField: "validatorAttempts",
      parentField: "validatorParent",
      idleMessage: "review finished without recording findings",
    },
  ]

  for (const { role, sessionField, outcomeField, attemptsField, parentField, idleMessage } of cases) {
    test(`${role}: session.error while pending clears session/outcome and bumps attempts`, async () => {
      const store: Record<string, SessionData> = {
        s1: {
          metadata: {
            kagan: {
              status: "backlog",
              boardTask: true,
              [sessionField]: "h1",
              [outcomeField]: "pending",
              [attemptsField]: 1,
            },
          },
        },
        h1: { parentID: "s1", metadata: { kagan: { role, [parentField]: "s1" } } },
      }
      const { input, captured } = makeInput({ store })
      const hooks = await plugin.server(input, {})
      await hooks.event?.({
        event: {
          type: "session.error",
          properties: { sessionID: "h1", error: { name: "X", data: { message: "boom" } } },
        },
      } as never)
      const patch = captured.updates.find((u) => u.id === "s1")
      expect(patch?.kagan).toMatchObject({ [sessionField]: undefined, [outcomeField]: undefined, [attemptsField]: 1 })
    })

    test(`${role}: session.idle while pending exhausts retries and records the idle-specific message`, async () => {
      const store: Record<string, SessionData> = {
        s1: {
          metadata: {
            kagan: {
              status: "backlog",
              boardTask: true,
              [sessionField]: "h1",
              [outcomeField]: "pending",
              [attemptsField]: 2,
            },
          },
        },
        h1: { parentID: "s1", metadata: { kagan: { role, [parentField]: "s1" } } },
      }
      const { input, captured } = makeInput({ store })
      const hooks = await plugin.server(input, {})
      await hooks.event?.({ event: { type: "session.idle", properties: { sessionID: "h1" } } } as never)
      const patch = captured.updates.find((u) => u.id === "s1")
      expect(patch?.kagan.helperError).toEqual({ role, message: idleMessage })
      if (role === "intake") {
        const finalMetadata = { kagan: { status: "backlog", boardTask: true, ...patch?.kagan } }
        expect(intakeReady(finalMetadata)).toBe(true)
      }
    })

    test(`${role}: a stale event (idle or error) from a superseded helper session is a no-op`, async () => {
      const store: Record<string, SessionData> = {
        s1: {
          metadata: { kagan: { status: "backlog", boardTask: true, [sessionField]: "h2", [outcomeField]: "pending" } },
        },
        h1: { parentID: "s1", metadata: { kagan: { role, [parentField]: "s1" } } },
      }
      const { input, captured } = makeInput({ store })
      const hooks = await plugin.server(input, {})
      await hooks.event?.({ event: { type: "session.idle", properties: { sessionID: "h1" } } } as never)
      await hooks.event?.({
        event: {
          type: "session.error",
          properties: { sessionID: "h1", error: { name: "X", data: { message: "late failure" } } },
        },
      } as never)
      expect(captured.updates).toHaveLength(0)
    })
  }
})

describe("kagan server — respawn on cleared helper state (manual retry / post-exhaustion)", () => {
  test("does not respawn the validator while an exhausted-failed session id is still recorded", async () => {
    const exhausted = {
      kagan: {
        status: "review",
        boardTask: true,
        worktree: "/wt",
        validatorSessionID: "v-old",
        validatorOutcome: "failed",
        helperError: { role: "validator", message: "boom" },
      },
    }
    const store: Record<string, SessionData> = { s1: { title: "Task", metadata: exhausted } }
    const { input, captured } = makeInput({ store })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata: exhausted } } },
    } as never)
    expect(captured.creates).toHaveLength(0)
  })

  test("cleared validator state on a review task triggers a respawn without a column transition", async () => {
    const exhausted = {
      title: "Task",
      metadata: {
        kagan: { ...boardReview.kagan, validatorSessionID: "v-old", validatorOutcome: "failed" as const },
      },
    }
    const store: Record<string, SessionData> = { s1: exhausted }
    const { input, captured } = makeInput({ store, createId: "validator-new" })
    const hooks = await plugin.server(input, {})
    // Bootstrap: task already in review, validator exhausted-failed — must not respawn.
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata: exhausted.metadata } } },
    } as never)
    expect(captured.creates).toHaveLength(0)

    // Manual retry (or the auto-retry's own clearing patch) resets the helper state in the store;
    // the follow-up session.updated carries the same "review" column, no transition.
    store.s1 = { title: "Task", metadata: boardReview }
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata: boardReview } } },
    } as never)
    expect(captured.creates).toHaveLength(1)
  })

  test("cleared intake state on a backlog task triggers a respawn without a column transition", async () => {
    const exhausted = {
      kagan: { status: "backlog", boardTask: true, intakeSessionID: "intake-old", intakeOutcome: "failed" },
    }
    const store: Record<string, SessionData> = { s1: { metadata: exhausted } }
    const { input, captured } = makeInput({ store, createId: "intake-new" })
    const hooks = await plugin.server(input, {})
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata: exhausted } } },
    } as never)
    expect(captured.creates).toHaveLength(0)

    const metadata = { kagan: { status: "backlog", boardTask: true } }
    store.s1 = { metadata }
    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", metadata } } },
    } as never)
    expect(captured.creates).toHaveLength(1)
  })
})

describe("kagan server — duplicate helper spawn regression", () => {
  function roleCreates(captured: Captured, role: string) {
    return captured.creates.filter((c) => (c.metadata as { kagan?: { role?: string } })?.kagan?.role === role)
  }

  test("session.created followed by its induced session.updated spawns intake exactly once", async () => {
    const store: Record<string, SessionData> = {
      s1: { title: "Add retry", metadata: { kagan: { status: "backlog", boardTask: true } } },
    }
    const { input, captured } = makeInput({ store, createId: "intake-1" })
    const hooks = await plugin.server(input, {})

    await hooks.event?.({
      event: {
        type: "session.created",
        properties: {
          info: { id: "s1", title: "Add retry", metadata: { kagan: { status: "backlog", boardTask: true } } },
        },
      },
    } as never)

    // The lastGatedStatus bootstrap patch above induces this session.updated; OpenCode delivers it
    // with the snapshot as it stood right after that patch — lastGatedStatus set, but no intake
    // claim yet, since spawnIntake's own patch had not run when this event was captured.
    await hooks.event?.({
      event: {
        type: "session.updated",
        properties: {
          info: {
            id: "s1",
            title: "Add retry",
            metadata: { kagan: { status: "backlog", boardTask: true, lastGatedStatus: "backlog" } },
          },
        },
      },
    } as never)

    expect(roleCreates(captured, "intake")).toHaveLength(1)
  })

  test("two concurrent session.updated backlog events for the same task spawn intake exactly once", async () => {
    const store: Record<string, SessionData> = {
      s1: { title: "Add retry", metadata: { kagan: { status: "backlog", boardTask: true } } },
    }
    const { input, captured } = makeInput({ store, createId: "intake-1" })
    const hooks = await plugin.server(input, {})
    const info = { id: "s1", title: "Add retry", metadata: { kagan: { status: "backlog", boardTask: true } } }

    await Promise.all([
      hooks.event?.({ event: { type: "session.updated", properties: { info } } } as never),
      hooks.event?.({ event: { type: "session.updated", properties: { info } } } as never),
    ])

    expect(roleCreates(captured, "intake")).toHaveLength(1)
  })

  test("a session.updated induced by the check-result patch racing a duplicate delivery spawns the validator exactly once", async () => {
    const metadata = { kagan: { ...boardReview.kagan, worktree: "/tmp" } }
    const store: Record<string, SessionData> = { s1: { title: "Task", metadata } }
    const { input, captured } = makeInput({
      store,
      createId: "validator-1",
      gitStdout: (args) => {
        if (args[0] === "merge-base") return "abc123"
        if (args.includes("--numstat")) return "1\t1\tfile.txt"
        if (args.includes("--name-status")) return "M\tfile.txt"
        return ""
      },
    })
    const hooks = await plugin.server(input, {
      commands: { check: [{ name: "check", cwd: ".", command: "echo check-ok" }] },
    })

    await Promise.all([
      hooks.event?.({ event: { type: "session.updated", properties: { info: { id: "s1", metadata } } } } as never),
      hooks.event?.({ event: { type: "session.updated", properties: { info: { id: "s1", metadata } } } } as never),
    ])

    expect(roleCreates(captured, "validator")).toHaveLength(1)
  })

  test("spawn-failure auto-retry survives the clear-patch event re-entering before the patch resolves", async () => {
    const store: Record<string, SessionData> = {
      s1: {
        title: "Add retry",
        metadata: { kagan: { status: "backlog", boardTask: true, lastGatedStatus: "backlog" } },
      },
    }
    let child = 0
    let hooks: Awaited<ReturnType<typeof plugin.server>> | undefined
    const { input, captured } = makeInput({
      store,
      createId: () => `intake-${++child}`,
      promptError: (id) => (id === "intake-1" ? new Error("boom") : undefined),
      // OpenCode commits a session.update and notifies event listeners before the update call
      // resolves; replaying that ordering is the point of this test.
      onUpdate: async (id) => {
        await hooks?.event?.({
          event: { type: "session.updated", properties: { info: { id, ...store[id] } } },
        } as never)
      },
    })
    hooks = await plugin.server(input, {})

    await hooks.event?.({
      event: { type: "session.updated", properties: { info: { id: "s1", ...store.s1 } } },
    } as never)

    const intakes = roleCreates(captured, "intake")
    expect(intakes).toHaveLength(2)
    expect(captured.prompts.filter((p) => p.id === "intake-2")).toHaveLength(1)
  })
})
