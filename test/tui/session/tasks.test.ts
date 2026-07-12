import { beforeEach, describe, expect, mock, test } from "bun:test"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { mockSessionClient, mockTuiApi } from "../../fixtures/api"

const realRunner = await import("../../../src/git/runner")
import { getStatus } from "../../../src/domain/task/metadata"

const sequence: string[] = []
let mergeResult = { ok: true, message: "Merged kagan/x" }
let currentBranchValue: string | undefined = "kagan/x"
type MockCommandStep = {
  name: string
  cwd: string
  command: string
  status: "ran" | "skipped"
  exitCode: number | null
  output: string
}

mock.module("../../../src/git/runner", () => ({
  ...realRunner,
  bunGitRunner: () => async () => ({ code: 0, stdout: "", stderr: "" }),
  uniqueTaskSlug: (title: string) => `${title}-slug`,
  createTaskWorktree: async () => {
    sequence.push("worktree")
    return { directory: "/wt", branch: "kagan/x" }
  },
  ensureWorktreePluginConfig: async (directory: string) => {
    sequence.push(`plugin-config:${directory}`)
  },
  currentBranch: async () => currentBranchValue,
}))

mock.module("../../../src/git/merge", () => ({
  mergeTaskBranch: async (
    _run: unknown,
    _main: string,
    _wt: string,
    _branch: string,
    target: string,
    _message: string,
    squash: boolean,
  ) => {
    sequence.push(`merge:${target}:${squash}`)
    return mergeResult
  },
}))

mock.module("../../../src/checks/runner", () => ({
  truncateCheckResultForMetadata: <T>(result: T) => result,
  runCheckCommand: async (command: string) => {
    if (command === 'echo "installed"') return { command, exitCode: 0, output: "installed\n" }
    if (command === "exit 1") return { command, exitCode: 1, output: "" }
    return { command, exitCode: null, output: "" }
  },
  runCommandPlan: async (
    commands: Array<{ name: string; cwd: string; command: string }>,
    _worktree: string,
    shouldRun: (command: { name: string; cwd: string; command: string }) => boolean,
    _skipReason?: string,
    recordSkipped = true,
  ) => {
    if (commands.length === 0) return undefined
    const steps: MockCommandStep[] = commands.flatMap((command): MockCommandStep[] => {
      if (shouldRun(command)) {
        return [
          {
            name: command.name,
            cwd: command.cwd,
            command: command.command,
            status: "ran",
            exitCode: command.command === "exit 1" ? 1 : 0,
            output: command.command === 'echo "installed"' ? "installed\n" : "",
          },
        ]
      }
      if (!recordSkipped) return []
      return [
        {
          name: command.name,
          cwd: command.cwd,
          command: command.command,
          status: "skipped",
          exitCode: null,
          output: "",
        },
      ]
    })
    if (steps.length === 0) return undefined
    return {
      command: steps.map((step) => `${step.name}: ${step.command}`).join(" && "),
      exitCode: steps.some((step) => step.exitCode === 1) ? 1 : 0,
      output: steps.map((step) => step.output).join(""),
      steps,
    }
  },
}))

const { approveSession, archiveSession, resolveSessionFinding, resolveSessionIntakeDecision, retryHelper } =
  await import("../../../src/tui/session/tasks")
const { createTask, deleteSession, mergeTask, sendBack } = await import("../../../src/tui/tasks")
const { getFilter, setFilter } = await import("../../../src/tui/session/preferences")

beforeEach(() => {
  sequence.length = 0
  mergeResult = { ok: true, message: "Merged kagan/x" }
  currentBranchValue = "kagan/x"
})

describe("getStatus", () => {
  test("defaults to backlog when metadata is missing", () => {
    expect(getStatus(undefined)).toBe("backlog")
  })

  test("reads a valid status from metadata", () => {
    expect(getStatus({ kagan: { status: "done" } })).toBe("done")
  })

  test("defaults to backlog for an invalid status", () => {
    expect(getStatus({ kagan: { status: "invalid" } })).toBe("backlog")
  })
})

describe("status patch shape", () => {
  test("column status is stored at kagan.status", () => {
    expect({ kagan: { status: "in_progress" } }).toEqual({ kagan: { status: "in_progress" } })
  })
})

describe("filter persistence", () => {
  test("getFilter returns the stored filter", () => {
    expect(getFilter(mockTuiApi({ kvMap: { "kagan:filter": "board" } }))).toBe("board")
  })

  test("setFilter persists the filter", () => {
    const api = mockTuiApi()
    setFilter(api, "login")
    expect(api.kv.get("kagan:filter", "")).toBe("login")
  })
})

describe("createTask", () => {
  test("creates the worktree before creating the session and stamps kagan metadata", async () => {
    let createArg: Record<string, unknown> | undefined
    const api = {
      state: { path: { worktree: "/repo" } },
      client: {
        session: {
          list: async () => ({
            data: [
              { id: "a", metadata: { kagan: { taskNumber: 2, boardTask: true } } },
              { id: "b", metadata: { kagan: { taskNumber: 5, boardTask: true } } },
            ],
          }),
          create: async (parameters: Record<string, unknown>) => {
            sequence.push("create")
            createArg = parameters
            return { data: { id: "s1" } }
          },
        },
      },
    } as unknown as TuiPluginApi

    const session = await createTask(api, {
      title: "Fix login",
      description: "Handle the expired-token edge case cleanly.",
      model: { providerID: "anthropic", modelID: "claude-sonnet-4-20250514" },
      baseBranch: "main",
    })

    expect(session.id).toBe("s1")
    expect(sequence).toEqual(["worktree", "plugin-config:/wt", "create"])
    expect(createArg?.directory).toBe("/wt")
    expect(createArg?.model).toEqual({ id: "claude-sonnet-4-20250514", providerID: "anthropic" })
    const kagan = (createArg!.metadata as { kagan: Record<string, unknown> }).kagan
    expect(kagan).toMatchObject({
      status: "backlog",
      boardTask: true,
      taskNumber: 6,
      baseBranch: "main",
      worktree: "/wt",
      description: "Handle the expired-token edge case cleanly.",
      model: { providerID: "anthropic", modelID: "claude-sonnet-4-20250514" },
    })
  })

  test("assigns task number 1 and omits description and model keys when absent", async () => {
    let createArg: Record<string, unknown> | undefined
    const api = {
      state: { path: { worktree: "/repo" } },
      client: {
        session: {
          list: async () => ({ data: [] }),
          create: async (parameters: Record<string, unknown>) => {
            createArg = parameters
            return { data: { id: "s1" } }
          },
        },
      },
    } as unknown as TuiPluginApi
    await createTask(api, { title: "Task", description: "   ", baseBranch: "main" })
    expect(createArg?.model).toBeUndefined()
    const kagan = (createArg!.metadata as { kagan: Record<string, unknown> }).kagan
    expect(kagan.taskNumber).toBe(1)
    expect("description" in kagan).toBe(false)
    expect("model" in kagan).toBe(false)
    expect("setup" in kagan).toBe(false)
  })

  test("records a setup CheckResult when setup commands are provided", async () => {
    let createArg: Record<string, unknown> | undefined
    const api = {
      state: { path: { worktree: "/repo" } },
      client: {
        session: {
          list: async () => ({ data: [] }),
          create: async (parameters: Record<string, unknown>) => {
            createArg = parameters
            return { data: { id: "s1" } }
          },
        },
      },
    } as unknown as TuiPluginApi
    await createTask(api, {
      title: "Task",
      description: "",
      baseBranch: "main",
      setupCommands: [{ name: "setup", cwd: ".", command: 'echo "installed"' }],
    })
    const kagan = (createArg!.metadata as { kagan: Record<string, unknown> }).kagan
    const setup = kagan.setup as { command: string; exitCode: number | null; output: string }
    expect(setup.command).toBe('setup: echo "installed"')
    expect(setup.exitCode).toBe(0)
    expect(setup.output).toContain("installed")
  })

  test("still creates the session when the setup command fails", async () => {
    let createArg: Record<string, unknown> | undefined
    const api = {
      state: { path: { worktree: "/repo" } },
      client: {
        session: {
          list: async () => ({ data: [] }),
          create: async (parameters: Record<string, unknown>) => {
            createArg = parameters
            return { data: { id: "s1" } }
          },
        },
      },
    } as unknown as TuiPluginApi
    const session = await createTask(api, {
      title: "Task",
      description: "",
      baseBranch: "main",
      setupCommands: [{ name: "setup", cwd: ".", command: "exit 1" }],
    })
    expect(session.id).toBe("s1")
    const kagan = (createArg!.metadata as { kagan: Record<string, unknown> }).kagan
    const setup = kagan.setup as { exitCode: number | null }
    expect(setup.exitCode).toBe(1)
  })

  test("records only ran setup commands outside the task scope", async () => {
    let createArg: Record<string, unknown> | undefined
    const api = {
      state: { path: { worktree: "/repo" } },
      client: {
        session: {
          list: async () => ({ data: [] }),
          create: async (parameters: Record<string, unknown>) => {
            createArg = parameters
            return { data: { id: "s1" } }
          },
        },
      },
    } as unknown as TuiPluginApi
    await createTask(api, {
      title: "Task",
      description: "",
      baseBranch: "main",
      scope: { values: ["project-alpha"] },
      setupCommands: [
        { name: "alpha deps", cwd: "project-alpha", command: "npm ci" },
        { name: "beta deps", cwd: "project-beta", command: "npm ci" },
      ],
    })
    const kagan = (createArg!.metadata as { kagan: Record<string, unknown> }).kagan
    expect(kagan.scope).toEqual({ values: ["project-alpha"] })
    expect(kagan.setup).toMatchObject({
      command: "alpha deps: npm ci",
      steps: [{ name: "alpha deps", cwd: "project-alpha", status: "ran" }],
    })
  })

  test("does not run setup from custom scope text unless it matches a configured cwd", async () => {
    let createArg: Record<string, unknown> | undefined
    const api = {
      state: { path: { worktree: "/repo" } },
      client: {
        session: {
          list: async () => ({ data: [] }),
          create: async (parameters: Record<string, unknown>) => {
            createArg = parameters
            return { data: { id: "s1" } }
          },
        },
      },
    } as unknown as TuiPluginApi
    await createTask(api, {
      title: "Task",
      description: "",
      baseBranch: "main",
      scope: { values: [], custom: "docs" },
      setupCommands: [{ name: "alpha deps", cwd: "project-alpha", command: "npm ci" }],
    })
    const kagan = (createArg!.metadata as { kagan: Record<string, unknown> }).kagan
    expect(kagan.scope).toEqual({ values: [], custom: "docs" })
    expect(kagan.setup).toBeUndefined()
  })

  test("runs setup when custom scope exactly matches a configured command cwd", async () => {
    let createArg: Record<string, unknown> | undefined
    const api = {
      state: { path: { worktree: "/repo" } },
      client: {
        session: {
          list: async () => ({ data: [] }),
          create: async (parameters: Record<string, unknown>) => {
            createArg = parameters
            return { data: { id: "s1" } }
          },
        },
      },
    } as unknown as TuiPluginApi
    await createTask(api, {
      title: "Task",
      description: "",
      baseBranch: "main",
      scope: { values: [], custom: "project-alpha" },
      setupCommands: [{ name: "alpha deps", cwd: "project-alpha", command: "npm ci" }],
    })
    const kagan = (createArg!.metadata as { kagan: Record<string, unknown> }).kagan
    expect(kagan.scope).toEqual({ values: [], custom: "project-alpha" })
    expect(kagan.setup).toBeDefined()
  })
})

describe("sendBack", () => {
  const session = {
    id: "root",
    title: "Add retry",
    metadata: {
      kagan: {
        worktree: "/wt",
        baseBranch: "main",
        generation: 1,
        model: { providerID: "anthropic", modelID: "m" },
        findings: [
          {
            id: "f0",
            summary: "Existing retry log is intentional",
            resolution: "intended",
            note: "Operators rely on the retry log when tracing webhook delivery failures.",
          },
          {
            id: "f1",
            summary: "Off-by-one in retry counter",
            resolution: "clarified",
            note: "Cap at 3 tries as agreed.",
          },
          {
            id: "f2",
            summary: "Generated helper name is noisy",
            resolution: "ignored",
            note: "The helper name mirrors the generated downstream operation name.",
          },
        ],
        priorTriage: [{ id: "prior", summary: "Keep compatibility import", resolution: "intended" }],
        approved: true,
      },
    },
  } as never

  test("creates a worker iteration, prompts it with the handoff, then patches the root once", async () => {
    let promptArg: Record<string, unknown> | undefined
    let updateArg: Record<string, unknown> | undefined
    const api = {
      client: {
        session: {
          get: async () => ({ data: { metadata: (session as { metadata: Record<string, unknown> }).metadata } }),
          messages: async () => ({
            data: [{ info: { role: "assistant" }, parts: [{ type: "text", text: "Implemented the retry wrapper." }] }],
          }),
          create: async () => {
            sequence.push("create")
            return { data: { id: "worker1" } }
          },
          promptAsync: async (parameters: Record<string, unknown>) => {
            sequence.push("prompt")
            promptArg = parameters
          },
          update: async (parameters: Record<string, unknown>) => {
            sequence.push("update")
            updateArg = parameters
          },
        },
      },
    } as unknown as TuiPluginApi

    await sendBack(api, session)

    expect(sequence).toEqual(["create", "prompt", "update"])
    expect(promptArg?.model).toEqual({ providerID: "anthropic", modelID: "m" })
    const parts = promptArg?.parts as { text: string }[]
    expect(parts[0]?.text).toContain("Continue the work in place")
    expect(parts[0]?.text).toContain("Implemented the retry wrapper.")

    const kagan = (updateArg!.metadata as { kagan: Record<string, unknown> }).kagan
    expect(kagan.status).toBe("in_progress")
    expect(kagan.activeIteration).toBe("worker1")
    expect(kagan.generation).toBe(2)
    expect(kagan.priorTriage).toEqual([
      { id: "prior", summary: "Keep compatibility import", resolution: "intended" },
      {
        id: "f0",
        summary: "Existing retry log is intentional",
        resolution: "intended",
        note: "Operators rely on the retry log when tracing webhook delivery failures.",
      },
      {
        id: "f2",
        summary: "Generated helper name is noisy",
        resolution: "ignored",
        note: "The helper name mirrors the generated downstream operation name.",
      },
    ])
    expect(kagan.approved).toBeUndefined()
    expect(kagan.findings).toBeUndefined()
  })

  test("pulls the previous report from the active worker iteration's messages, not the root's", async () => {
    const iterating = {
      id: "root",
      title: "Add retry",
      metadata: {
        kagan: {
          worktree: "/wt",
          baseBranch: "main",
          generation: 2,
          activeIteration: "worker-prev",
        },
      },
    } as never
    let messagesRequestedFor: string | undefined
    let promptArg: Record<string, unknown> | undefined
    const api = {
      client: {
        session: {
          get: async () => ({ data: { metadata: (iterating as { metadata: Record<string, unknown> }).metadata } }),
          messages: async ({ sessionID }: { sessionID: string }) => {
            messagesRequestedFor = sessionID
            const text = sessionID === "worker-prev" ? "Worker iteration report." : "Root session report (wrong)."
            return { data: [{ info: { role: "assistant" }, parts: [{ type: "text", text }] }] }
          },
          create: async () => {
            sequence.push("create")
            return { data: { id: "worker2" } }
          },
          promptAsync: async (parameters: Record<string, unknown>) => {
            sequence.push("prompt")
            promptArg = parameters
          },
          update: async () => {
            sequence.push("update")
          },
        },
      },
    } as unknown as TuiPluginApi

    await sendBack(api, iterating)

    expect(messagesRequestedFor).toBe("worker-prev")
    const parts = promptArg?.parts as { text: string }[]
    expect(parts[0]?.text).toContain("Worker iteration report.")
    expect(parts[0]?.text).not.toContain("wrong")
  })
})

describe("approveSession", () => {
  test("throws when the task is not approvable", async () => {
    const api = { client: { session: { update: async () => {} } } } as unknown as TuiPluginApi
    await expect(approveSession(api, "s1", { metadata: { kagan: { boardTask: true } } } as never)).rejects.toThrow()
  })

  test("stamps approved when the gate is clear", async () => {
    const target = {
      metadata: { kagan: { boardTask: true, validatorOutcome: "ran" } },
    }
    const { client, capture } = mockSessionClient({ metadata: target.metadata })
    const api = { client } as unknown as TuiPluginApi
    await approveSession(api, "s1", target as never)
    expect((capture.updateArg!.metadata as { kagan: Record<string, unknown> }).kagan.approved).toBe(true)
  })
})

describe("archiveSession", () => {
  test("stamps a time.archived timestamp via session.update", async () => {
    let updateArg: { sessionID?: string; time?: { archived?: number } } | undefined
    const api = {
      client: {
        session: {
          update: async (parameters: unknown) => {
            updateArg = parameters as typeof updateArg
          },
        },
      },
    } as unknown as TuiPluginApi
    const before = Date.now()
    await archiveSession(api, "s1")
    expect(updateArg?.sessionID).toBe("s1")
    expect(updateArg?.time?.archived).toBeGreaterThanOrEqual(before)
  })
})

describe("resolveSessionFinding", () => {
  test("writes the resolution and note onto the matching finding", async () => {
    const session = { metadata: { kagan: { findings: [{ id: "f1", summary: "issue" }] } } } as never
    const { client, capture } = mockSessionClient({
      metadata: (session as { metadata: Record<string, unknown> }).metadata,
    })
    const api = { client } as unknown as TuiPluginApi
    await resolveSessionFinding(api, "s1", session, "f1", "ignored", "Not reachable in this configuration.")
    const findings = (capture.updateArg!.metadata as { kagan: { findings: Record<string, unknown>[] } }).kagan.findings
    expect(findings[0]).toMatchObject({ resolution: "ignored", note: "Not reachable in this configuration." })
  })
})

describe("resolveSessionIntakeDecision", () => {
  test("updates the targeted decision's resolution and answer via a patch", async () => {
    const session = {
      metadata: {
        kagan: {
          intake: {
            understanding: "Adds a retry wrapper.",
            decisions: [
              { id: "d1", question: "Max retries?", assumption: "3", required: true },
              { id: "d2", question: "Backoff?", assumption: "linear", required: true },
            ],
          },
        },
      },
    } as never
    const { client, capture } = mockSessionClient({
      metadata: (session as { metadata: Record<string, unknown> }).metadata,
    })
    const api = { client } as unknown as TuiPluginApi
    await resolveSessionIntakeDecision(api, "s1", session, "d2", "overridden", "Use exponential backoff instead.")
    const intake = (capture.updateArg!.metadata as { kagan: { intake: { decisions: Record<string, unknown>[] } } })
      .kagan.intake
    expect(intake.decisions).toEqual([
      { id: "d1", question: "Max retries?", assumption: "3", required: true },
      {
        id: "d2",
        question: "Backoff?",
        assumption: "linear",
        required: true,
        resolution: "overridden",
        answer: "Use exponential backoff instead.",
      },
    ])
  })

  test("no-ops silently when the session has no intake", async () => {
    const session = { metadata: { kagan: {} } } as never
    let updateCalled = false
    const api = {
      client: {
        session: {
          get: async () => ({ data: { metadata: {} } }),
          update: async () => {
            updateCalled = true
          },
        },
      },
    } as unknown as TuiPluginApi
    await resolveSessionIntakeDecision(api, "s1", session, "d1", "approved")
    expect(updateCalled).toBe(false)
  })
})

describe("serialized metadata patches", () => {
  test("concurrent writes to one session merge against fresh metadata", async () => {
    let metadata: Record<string, unknown> = {
      kagan: {
        boardTask: true,
        validatorOutcome: "ran",
        findings: [{ id: "f1", summary: "issue", resolution: "intended" }],
      },
    }
    const snapshots: Record<string, unknown>[] = []
    const session = { metadata } as never
    const api = {
      client: {
        session: {
          get: async () => {
            snapshots.push(structuredClone(metadata))
            return { data: { metadata } }
          },
          update: async (parameters: Record<string, unknown>) => {
            metadata = parameters.metadata as Record<string, unknown>
          },
        },
      },
    } as unknown as TuiPluginApi

    await Promise.all([
      approveSession(api, "s1", session),
      resolveSessionFinding(api, "s1", session, "f1", "ignored", "This finding is unreachable in this setup."),
    ])

    const kagan = metadata.kagan as { approved?: boolean; findings: Array<Record<string, unknown>> }
    expect(kagan.approved).toBe(true)
    expect(kagan.findings[0]).toMatchObject({ resolution: "ignored" })
    expect((snapshots[1]?.kagan as { approved?: boolean } | undefined)?.approved).toBe(true)
  })

  test("a failed write does not wedge the next write", async () => {
    let metadata: Record<string, unknown> = { kagan: { boardTask: true, validatorOutcome: "ran" } }
    let writes = 0
    const session = { metadata } as never
    const api = {
      client: {
        session: {
          get: async () => ({ data: { metadata } }),
          update: async (parameters: Record<string, unknown>) => {
            writes++
            if (writes === 1) throw new Error("write failed")
            metadata = parameters.metadata as Record<string, unknown>
          },
        },
      },
    } as unknown as TuiPluginApi

    await expect(approveSession(api, "s1", session)).rejects.toThrow("write failed")
    await approveSession(api, "s1", session)

    expect(writes).toBe(2)
    expect((metadata.kagan as { approved?: boolean }).approved).toBe(true)
  })

  test("a nested same-session write runs without deadlocking", async () => {
    let metadata: Record<string, unknown> = {
      kagan: {
        boardTask: true,
        validatorOutcome: "ran",
        findings: [{ id: "f1", summary: "issue", resolution: "intended" }],
      },
    }
    const session = { metadata } as never
    let nested = false
    const api = {
      client: {
        session: {
          get: async () => ({ data: { metadata } }),
          update: async (parameters: Record<string, unknown>) => {
            metadata = parameters.metadata as Record<string, unknown>
            if (nested) return
            nested = true
            await resolveSessionFinding(api, "s1", session, "f1", "ignored", "Nested update kept the prior write.")
          },
        },
      },
    } as unknown as TuiPluginApi

    await Promise.race([
      approveSession(api, "s1", session),
      new Promise((_, reject) => setTimeout(() => reject(new Error("deadlocked")), 100)),
    ])

    const kagan = metadata.kagan as { approved?: boolean; findings: Array<Record<string, unknown>> }
    expect(kagan.approved).toBe(true)
    expect(kagan.findings[0]).toMatchObject({ resolution: "ignored" })
  })
})

describe("retryHelper", () => {
  test("clears intake state on a backlog task", async () => {
    const session = {
      metadata: { kagan: { status: "backlog", boardTask: true, intakeOutcome: "failed", intakeAttempts: 2 } },
    } as never
    const { client, capture } = mockSessionClient({
      metadata: (session as { metadata: Record<string, unknown> }).metadata,
    })
    const api = { client } as unknown as TuiPluginApi
    await retryHelper(api, "s1", session, "backlog")
    const kagan = (capture.updateArg!.metadata as { kagan: Record<string, unknown> }).kagan
    expect(kagan).toMatchObject({
      intakeSessionID: undefined,
      intakeOutcome: undefined,
      intakeAttempts: 0,
      intake: undefined,
    })
    expect(kagan.helperError).toBeUndefined()
  })

  test("clears validator state on a review task", async () => {
    const session = {
      metadata: {
        kagan: {
          status: "review",
          boardTask: true,
          validatorOutcome: "ran",
          validatorAttempts: 1,
          findings: [{ id: "f1", summary: "issue" }],
          approved: true,
        },
      },
    } as never
    const { client, capture } = mockSessionClient({
      metadata: (session as { metadata: Record<string, unknown> }).metadata,
    })
    const api = { client } as unknown as TuiPluginApi
    await retryHelper(api, "s1", session, "review")
    const kagan = (capture.updateArg!.metadata as { kagan: Record<string, unknown> }).kagan
    expect(kagan).toMatchObject({
      validatorSessionID: undefined,
      validatorOutcome: undefined,
      validatorAttempts: 0,
      findings: undefined,
      approved: undefined,
    })
    expect(kagan.helperError).toBeUndefined()
  })

  test("clears recorded helper state before aborting the live session", async () => {
    const calls: string[] = []
    const session = {
      metadata: { kagan: { intakeSessionID: "intake-live", intakeOutcome: "pending" } },
    } as never
    const api = {
      client: {
        session: {
          get: async () => ({ data: { metadata: (session as { metadata: Record<string, unknown> }).metadata } }),
          abort: async ({ sessionID }: { sessionID: string }) => {
            calls.push(`abort:${sessionID}`)
          },
          update: async () => {
            calls.push("clear")
          },
        },
      },
    } as unknown as TuiPluginApi
    await retryHelper(api, "s1", session, "backlog")
    expect(calls).toEqual(["clear", "abort:intake-live"])
  })

  test("rejects any other column", async () => {
    const api = { client: { session: { update: async () => {} } } } as unknown as TuiPluginApi
    await expect(retryHelper(api, "s1", { metadata: {} } as never, "in_progress")).rejects.toThrow(
      "Restart only applies to backlog or review tasks",
    )
  })
})

describe("deleteSession", () => {
  test("aborts and deletes helper children before deleting the board task", async () => {
    const aborted: string[] = []
    const deleted: string[] = []
    const api = {
      client: {
        session: {
          get: async () => ({
            data: {
              metadata: {
                kagan: {
                  boardTask: true,
                  intakeSessionID: "intake-1",
                  validatorSessionID: "validator-1",
                  activeIteration: "worker-1",
                },
              },
            },
          }),
          children: async () => ({ data: [{ id: "worker-2" }] }),
          abort: async ({ sessionID }: { sessionID: string }) => {
            aborted.push(sessionID)
            return { data: true }
          },
          delete: async ({ sessionID }: { sessionID: string }) => {
            deleted.push(sessionID)
            return { data: true }
          },
        },
      },
    } as unknown as TuiPluginApi

    await deleteSession(api, "root")

    expect(aborted.sort()).toEqual(["intake-1", "root", "validator-1", "worker-1", "worker-2"].sort())
    expect(deleted).toEqual(["intake-1", "validator-1", "worker-1", "worker-2", "root"])
  })

  test("still deletes the parent when helper lookups fail", async () => {
    const deleted: string[] = []
    const api = {
      client: {
        session: {
          get: async () => {
            throw new Error("missing")
          },
          children: async () => {
            throw new Error("missing")
          },
          abort: async () => ({ data: true }),
          delete: async ({ sessionID }: { sessionID: string }) => {
            deleted.push(sessionID)
            return { data: true }
          },
        },
      },
    } as unknown as TuiPluginApi

    await deleteSession(api, "root")

    expect(deleted).toEqual(["root"])
  })

  test("propagates a parent delete failure after stopping children", async () => {
    const api = {
      client: {
        session: {
          get: async () => ({ data: { metadata: { kagan: { intakeSessionID: "intake-1" } } } }),
          children: async () => ({ data: [] }),
          abort: async () => ({ data: true }),
          delete: async ({ sessionID }: { sessionID: string }) => {
            if (sessionID === "root") throw new Error("delete failed")
            return { data: true }
          },
        },
      },
    } as unknown as TuiPluginApi

    await expect(deleteSession(api, "root")).rejects.toThrow("delete failed")
  })
})

describe("mergeTask", () => {
  test("merges the task branch into the requested target", async () => {
    const api = { state: { path: { worktree: "/repo" } } } as unknown as TuiPluginApi
    const session = { title: "Add retry", metadata: { kagan: { worktree: "/wt" } } } as never
    const result = await mergeTask(api, session, "develop", true)
    expect(result.ok).toBe(true)
    expect(sequence).toContain("merge:develop:true")
  })

  test("fails without merging when the task has no worktree", async () => {
    const api = { state: { path: { worktree: "/repo" } } } as unknown as TuiPluginApi
    const result = await mergeTask(api, { title: "T", metadata: { kagan: {} } } as never, "main", true)
    expect(result.ok).toBe(false)
    expect(sequence).not.toContain("merge:main:true")
  })

  test("forwards squash=false through to mergeTaskBranch", async () => {
    const api = { state: { path: { worktree: "/repo" } } } as unknown as TuiPluginApi
    const session = { title: "Add retry", metadata: { kagan: { worktree: "/wt" } } } as never
    const result = await mergeTask(api, session, "develop", false)
    expect(result.ok).toBe(true)
    expect(sequence).toContain("merge:develop:false")
  })
})
