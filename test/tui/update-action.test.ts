import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test"
import type { TuiPluginApi, TuiPluginMeta, TuiToast } from "@opencode-ai/plugin/tui"
import type { UpdateCheck, UpdateStatus } from "../../src/tui/updates/check"
import { ROUTE } from "../../src/tui/types"

mock.module("../../../package.json", () => ({ version: "0.1.0" }))

const realRunner = await import("../../src/tui/updates/runner")
const realCheck = await import("../../src/tui/updates/check")
const realCheckForUpdate = realCheck.checkForUpdate
const realRunGlobalPluginUpdate = realRunner.runGlobalPluginUpdate

let checkOverride: (() => Promise<UpdateCheck>) | undefined
let runOverride: (() => Promise<{ ok: boolean; output: string; exitCode: number | null }>) | undefined

mock.module("../../src/tui/updates/check", () => ({
  ...realCheck,
  checkForUpdate: async (...args: Parameters<typeof realCheckForUpdate>) =>
    checkOverride ? checkOverride() : realCheckForUpdate(...args),
}))

mock.module("../../src/tui/updates/runner", () => ({
  ...realRunner,
  runGlobalPluginUpdate: async (...args: Parameters<typeof realRunGlobalPluginUpdate>) =>
    runOverride ? runOverride() : realRunGlobalPluginUpdate(...args),
}))

const { createUpdateController } = await import("../../src/tui/updates/action")

function meta(overrides: Partial<TuiPluginMeta> = {}): TuiPluginMeta {
  return {
    id: "kagan",
    source: "npm",
    spec: "@kagan-sh/kagan",
    target: "/tmp/kagan",
    state: "same",
    first_time: 0,
    last_time: 0,
    time_changed: 0,
    load_count: 1,
    fingerprint: "test",
    ...overrides,
  }
}

function fixture(
  input: {
    stage?: () => Promise<boolean>
    meta?: TuiPluginMeta
  } = {},
) {
  const statuses: UpdateStatus[] = []
  const notices: TuiToast[] = []
  const toasts: TuiToast[] = []
  const staged: string[] = []
  let resolveConfirm: ((ok: boolean) => void) | undefined
  const api = {
    route: { current: { name: ROUTE } },
    plugins: {
      add: async (spec: string) => {
        staged.push(spec)
        return input.stage ? input.stage() : true
      },
    },
    state: { path: { directory: "/repo" } },
    ui: {
      toast: (toast: TuiToast) => toasts.push(toast),
      dialog: {
        clear: () => {},
        replace: (render: () => unknown) => {
          render()
        },
      },
      DialogConfirm: (props: { onConfirm: () => void; onCancel: () => void }) => {
        resolveConfirm = (ok) => (ok ? props.onConfirm() : props.onCancel())
        return null
      },
    },
  } as unknown as TuiPluginApi
  const controller = createUpdateController({
    api,
    meta: input.meta ?? meta(),
    store: {
      setUpdateStatus: (status) => statuses.push(status),
      notify: (toast) => notices.push(toast),
    },
  })
  return {
    controller,
    statuses,
    notices,
    toasts,
    staged,
    confirm: () => resolveConfirm?.(true),
    cancel: () => resolveConfirm?.(false),
  }
}

describe("createUpdateController", () => {
  let runCalls = 0

  beforeEach(() => {
    runCalls = 0
    checkOverride = async () => ({ kind: "available", version: "0.2.0" })
    runOverride = async () => {
      runCalls++
      return { ok: true, output: "", exitCode: 0 }
    }
  })

  afterEach(() => {
    checkOverride = undefined
    runOverride = undefined
  })

  test("declining confirmation does not stage or run an update", async () => {
    const test = fixture()
    const done = test.controller.run()
    await Bun.sleep(0)
    test.cancel()
    await done
    expect(test.statuses).toEqual([{ kind: "available", version: "0.2.0" }])
    expect(test.staged).toEqual([])
    expect(runCalls).toBe(0)
  })

  test("staging failure skips the canonical command and remains actionable", async () => {
    const test = fixture({ stage: async () => false })
    const done = test.controller.run()
    await Bun.sleep(0)
    test.confirm()
    await done
    expect(runCalls).toBe(0)
    expect(test.statuses.at(-1)).toEqual({ kind: "available", version: "0.2.0" })
    expect(test.notices.at(-1)?.variant).toBe("error")
  })

  test("canonical command failure remains actionable", async () => {
    runOverride = async () => {
      runCalls++
      return { ok: false, output: "config failed", exitCode: 1 }
    }
    const test = fixture()
    const done = test.controller.run()
    await Bun.sleep(0)
    test.confirm()
    await done
    expect(test.staged).toEqual(["@kagan-sh/kagan@0.2.0"])
    expect(test.statuses.at(-1)).toEqual({ kind: "available", version: "0.2.0" })
    expect(test.notices.at(-1)?.message).toBe("config failed")
  })

  test("success requests restart", async () => {
    const test = fixture()
    const done = test.controller.run()
    await Bun.sleep(0)
    test.confirm()
    await done
    expect(test.staged).toEqual(["@kagan-sh/kagan@0.2.0"])
    expect(runCalls).toBe(1)
    expect(test.statuses.at(-1)).toEqual({ kind: "restart", version: "0.2.0" })
    expect(test.notices.at(-1)?.message).toContain("Restart OpenCode")
  })

  test("ineligible installs explain npm-only updates instead of reporting a check failure", async () => {
    checkOverride = async () => ({ kind: "ineligible" })
    const test = fixture({
      meta: meta({ source: "file", spec: "file:///tmp/kagan-pinned" }),
    })
    await test.controller.run()
    expect(test.notices.at(-1)).toEqual({
      variant: "info",
      title: "Kagan",
      message: "Updates apply only to global npm installs.",
    })
  })

  test("a second update request while the confirmation is open reuses the pending flow", async () => {
    const test = fixture()
    const first = test.controller.run()
    // Second invocation while the first confirmation dialog is still open must not open a rival
    // dialog or start a concurrent install.
    const second = test.controller.run()
    expect(second).toBe(first)
    await Bun.sleep(0)
    test.confirm()
    await Promise.all([first, second])
    expect(test.staged).toEqual(["@kagan-sh/kagan@0.2.0"])
    expect(runCalls).toBe(1)
  })
})

describe("runGlobalPluginUpdate", () => {
  test("runs the current executable with exact global force arguments and bounds output", async () => {
    let command: string[] = []
    let cwd = ""
    const result = await realRunGlobalPluginUpdate("0.2.0", "/repo", {
      execPath: "/bin/opencode",
      spawn: (args, options) => {
        command = args
        cwd = options.cwd
        return {
          exited: Promise.resolve(0),
          stdout: new Response("x".repeat(3000)).body!,
          stderr: new Response("").body!,
          kill: () => {},
        }
      },
    })
    expect(command).toEqual(["/bin/opencode", "plugin", "@kagan-sh/kagan@0.2.0", "--global", "--force"])
    expect(cwd).toBe("/repo")
    expect(result.ok).toBe(true)
    expect(result.output).toHaveLength(2000)
  })

  test("rejects invalid versions without spawning", async () => {
    let spawned = false
    const result = await realRunGlobalPluginUpdate("latest", "/repo", {
      spawn: () => {
        spawned = true
        throw new Error("unreachable")
      },
    })
    expect(spawned).toBe(false)
    expect(result.ok).toBe(false)
  })
})
