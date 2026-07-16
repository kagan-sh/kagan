import { describe, expect, mock, test } from "bun:test"
import type { TuiPluginApi, TuiPluginMeta, TuiToast } from "@opencode-ai/plugin/tui"
import { createUpdateController } from "../../src/tui/updates/action"
import type { UpdateCheck, UpdateStatus } from "../../src/tui/updates/check"
import { runGlobalPluginUpdate } from "../../src/tui/updates/runner"
import { ROUTE } from "../../src/tui/types"

mock.module("../../../package.json", () => ({ version: "0.1.0" }))

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
    run?: () => Promise<{ ok: boolean; output: string; exitCode: number | null }>
    meta?: TuiPluginMeta
    check?: (force: boolean) => Promise<UpdateCheck>
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
    ui: { toast: (toast: TuiToast) => toasts.push(toast) },
  } as unknown as TuiPluginApi
  const controller = createUpdateController({
    api,
    meta: input.meta ?? meta(),
    store: {
      setUpdateStatus: (status) => statuses.push(status),
      notify: (toast) => notices.push(toast),
    },
    check: input.check ?? (async () => ({ kind: "available", version: "0.2.0" })),
    confirm: (_current, _target) => new Promise<boolean>((resolve) => (resolveConfirm = resolve)),
    runCommand: async () => (input.run ? input.run() : { ok: true, output: "", exitCode: 0 }),
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
  test("declining confirmation does not stage or run an update", async () => {
    let ran = false
    const test = fixture({ run: async () => ((ran = true), { ok: true, output: "", exitCode: 0 }) })
    const done = test.controller.run()
    await Bun.sleep(0)
    test.cancel()
    await done
    expect(test.statuses).toEqual([{ kind: "available", version: "0.2.0" }])
    expect(test.staged).toEqual([])
    expect(ran).toBe(false)
  })

  test("staging failure skips the canonical command and remains actionable", async () => {
    let ran = false
    const test = fixture({
      stage: async () => false,
      run: async () => ((ran = true), { ok: true, output: "", exitCode: 0 }),
    })
    const done = test.controller.run()
    await Bun.sleep(0)
    test.confirm()
    await done
    expect(ran).toBe(false)
    expect(test.statuses.at(-1)).toEqual({ kind: "available", version: "0.2.0" })
    expect(test.notices.at(-1)?.variant).toBe("error")
  })

  test("canonical command failure remains actionable", async () => {
    const test = fixture({ run: async () => ({ ok: false, output: "config failed", exitCode: 1 }) })
    const done = test.controller.run()
    await Bun.sleep(0)
    test.confirm()
    await done
    expect(test.staged).toEqual(["@kagan-sh/kagan@0.2.0"])
    expect(test.statuses.at(-1)).toEqual({ kind: "available", version: "0.2.0" })
    expect(test.notices.at(-1)?.message).toBe("config failed")
  })

  test("success requests restart", async () => {
    let runs = 0
    const test = fixture({ run: async () => ({ ok: true, output: String(++runs), exitCode: 0 }) })
    const done = test.controller.run()
    await Bun.sleep(0)
    test.confirm()
    await done
    expect(test.staged).toEqual(["@kagan-sh/kagan@0.2.0"])
    expect(runs).toBe(1)
    expect(test.statuses.at(-1)).toEqual({ kind: "restart", version: "0.2.0" })
    expect(test.notices.at(-1)?.message).toContain("Restart OpenCode")
  })

  test("ineligible installs explain npm-only updates instead of reporting a check failure", async () => {
    const test = fixture({
      meta: meta({ source: "file", spec: "file:///tmp/kagan-pinned" }),
      check: async () => ({ kind: "ineligible" }),
    })
    await test.controller.run()
    expect(test.notices.at(-1)).toEqual({
      variant: "info",
      title: "Kagan",
      message: "Updates apply only to global npm installs.",
    })
  })

  test("a second update request while the confirmation is open reuses the pending flow", async () => {
    let runs = 0
    const test = fixture({ run: async () => ({ ok: true, output: String(++runs), exitCode: 0 }) })
    const first = test.controller.run()
    // Second invocation while the first confirmation dialog is still open must not open a rival
    // dialog or start a concurrent install.
    const second = test.controller.run()
    expect(second).toBe(first)
    await Bun.sleep(0)
    test.confirm()
    await Promise.all([first, second])
    expect(test.staged).toEqual(["@kagan-sh/kagan@0.2.0"])
    expect(runs).toBe(1)
  })
})

describe("runGlobalPluginUpdate", () => {
  test("runs the current executable with exact global force arguments and bounds output", async () => {
    let command: string[] = []
    let cwd = ""
    const result = await runGlobalPluginUpdate("0.2.0", "/repo", {
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
    const result = await runGlobalPluginUpdate("latest", "/repo", {
      spawn: () => {
        spawned = true
        throw new Error("unreachable")
      },
    })
    expect(spawned).toBe(false)
    expect(result.ok).toBe(false)
  })
})
