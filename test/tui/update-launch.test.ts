import { afterEach, describe, expect, test } from "bun:test"
import { lstat, mkdir, mkdtemp, readFile, rename, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import type { TuiPluginApi, TuiPluginMeta } from "@opencode-ai/plugin/tui"
import type { UpdateStatus } from "../../src/tui/updates"
import type { FileSystem } from "../../src/tui/update-paths"
import { runAutomaticUpdateLaunch } from "../../src/tui/update-launch"

const roots: string[] = []
const HOUR = 60 * 60 * 1000

afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })))
})

function registryFetch(latest = "0.2.0", engine = ">=1.17.13 <1.18.0"): typeof fetch {
  return (async (url: string | URL | Request) => {
    const value = String(url)
    const manifest = !value.endsWith("/dist-tags")
    return {
      ok: true,
      json: async () => (manifest ? { engines: { opencode: engine } } : { latest }),
    }
  }) as unknown as typeof fetch
}

function mockKv(initial: Record<string, unknown> = {}) {
  const store: Record<string, unknown> = { ...initial }
  return {
    store,
    get: <Value>(key: string, fallback?: Value) => (key in store ? (store[key] as Value) : (fallback as Value)),
    set: (key: string, value: unknown) => {
      store[key] = value
    },
  }
}

function meta(target: string): TuiPluginMeta {
  return {
    id: "kagan",
    source: "npm",
    spec: "@kagan-sh/kagan",
    target,
    state: "same",
    first_time: 0,
    last_time: 0,
    time_changed: 0,
    load_count: 1,
    fingerprint: "test",
  }
}

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "kagan-launch-"))
  roots.push(root)
  const scope = join(root, "opencode", "packages", "@kagan-sh")
  const current = join(scope, "kagan@latest")
  const target = join(current, "node_modules", "@kagan-sh", "kagan")
  await mkdir(target, { recursive: true })
  await writeFile(join(target, "package.json"), JSON.stringify({ name: "@kagan-sh/kagan", version: "0.1.0" }))
  return { scope, target }
}

async function writeWrapper(wrapper: string, version: string) {
  const target = join(wrapper, "node_modules", "@kagan-sh", "kagan")
  await mkdir(target, { recursive: true })
  await writeFile(join(target, "package.json"), JSON.stringify({ name: "@kagan-sh/kagan", version }))
}

describe("runAutomaticUpdateLaunch", () => {
  test("runs cleanup before check, then prepare, status, and toast", async () => {
    const layout = await fixture()
    const backup = join(layout.scope, "kagan@latest.kagan-backup")
    await writeWrapper(backup, "0.1.0")
    const order: string[] = []
    const statuses: UpdateStatus[] = []
    const toasts: UpdateStatus[] = []
    let cleanupSeen = false
    const fs = {
      lstat,
      readFile,
      rename,
      writeFile,
      rm: async (...args: Parameters<typeof rm>) => {
        if (!cleanupSeen) {
          order.push("cleanup")
          cleanupSeen = true
        }
        return rm(...args)
      },
    }
    const kv = mockKv()
    const fetchImpl = (async (url: string | URL | Request) => {
      order.push("check")
      return registryFetch()(url)
    }) as typeof fetch
    const api = {
      kv,
      app: { version: "1.17.18" },
      route: { current: { name: "home" } },
      lifecycle: { signal: new AbortController().signal },
      plugins: {
        add: async () => {
          order.push("prepare")
          const preparedTarget = join(layout.scope, "kagan@0.2.0", "node_modules", "@kagan-sh", "kagan")
          await mkdir(preparedTarget, { recursive: true })
          await writeFile(
            join(preparedTarget, "package.json"),
            JSON.stringify({ name: "@kagan-sh/kagan", version: "0.2.0" }),
          )
          return true
        },
      },
      ui: { toast: () => {} },
    } as unknown as TuiPluginApi

    await runAutomaticUpdateLaunch({
      api,
      meta: meta(layout.target),
      currentVersion: "0.1.0",
      now: HOUR + 1,
      setUpdateStatus: (status) => {
        order.push("status")
        statuses.push(status)
      },
      showToast: (_api, _version, status) => {
        order.push("toast")
        toasts.push(status)
      },
      fetchImpl,
      fs,
    })

    expect(order[0]).toBe("cleanup")
    expect(order.indexOf("check")).toBeLessThan(order.indexOf("prepare"))
    expect(order).toEqual(expect.arrayContaining(["prepare", "status", "toast"]))
    expect(statuses).toEqual([{ kind: "ready", version: "0.2.0" }])
    expect(toasts).toEqual([{ kind: "ready", version: "0.2.0" }])
  })

  test("continues to version check when cleanup fails and surfaces broken status", async () => {
    const layout = await fixture()
    const statuses: UpdateStatus[] = []
    const checks: string[] = []
    const kv = mockKv()
    const fetchImpl = (async (url: string | URL | Request) => {
      checks.push(String(url))
      return registryFetch()(url)
    }) as typeof fetch
    const api = {
      kv,
      app: { version: "1.17.18" },
      route: { current: { name: "home" } },
      lifecycle: { signal: new AbortController().signal },
      plugins: { add: async () => true },
      ui: { toast: () => {} },
    } as unknown as TuiPluginApi

    await runAutomaticUpdateLaunch({
      api,
      meta: meta(layout.target),
      currentVersion: "0.1.0",
      now: HOUR + 1,
      setUpdateStatus: (status) => statuses.push(status),
      showToast: () => {
        throw new Error("toast should not run")
      },
      fetchImpl,
      fs: {
        lstat: async () => ({ isFile: () => true, isDirectory: () => false }),
        readFile: async () => Buffer.from("{invalid"),
        rename: async () => {},
        rm: async () => {
          throw new Error("cleanup failed")
        },
        writeFile: async () => {},
      } as unknown as FileSystem,
    })

    expect(checks.length).toBeGreaterThan(0)
    expect(statuses).toEqual([{ kind: "broken" }])
  })

  test("keeps ready footer status when prepare fails", async () => {
    const layout = await fixture()
    const statuses: UpdateStatus[] = []
    const kv = mockKv()
    const api = {
      kv,
      app: { version: "1.17.18" },
      route: { current: { name: "home" } },
      lifecycle: { signal: new AbortController().signal },
      plugins: { add: async () => false },
      ui: { toast: () => {} },
    } as unknown as TuiPluginApi

    await runAutomaticUpdateLaunch({
      api,
      meta: meta(layout.target),
      currentVersion: "0.1.0",
      now: HOUR + 1,
      setUpdateStatus: (status) => statuses.push(status),
      fetchImpl: registryFetch(),
    })

    expect(statuses).toEqual([{ kind: "ready", version: "0.2.0" }])
  })
})
