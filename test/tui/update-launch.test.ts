import { describe, expect, test } from "bun:test"
import type { TuiPluginApi, TuiPluginMeta } from "@opencode-ai/plugin/tui"
import type { UpdateStatus } from "../../src/tui/updates/check"
import { runUpdateDiscovery } from "../../src/tui/updates/launch"

function mockKv() {
  const store: Record<string, unknown> = {}
  return {
    get: <Value>(key: string, fallback?: Value) => (key in store ? (store[key] as Value) : (fallback as Value)),
    set: (key: string, value: unknown) => {
      store[key] = value
    },
  }
}

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

function registryFetch(latest: string): typeof fetch {
  return (async () => ({ ok: true, json: async () => ({ latest }) })) as unknown as typeof fetch
}

describe("runUpdateDiscovery", () => {
  test("records availability without staging or config mutation", async () => {
    const statuses: UpdateStatus[] = []
    let staged = false
    const api = {
      kv: mockKv(),
      lifecycle: { signal: new AbortController().signal },
      plugins: { add: async () => ((staged = true), true) },
    } as unknown as TuiPluginApi
    await runUpdateDiscovery({
      api,
      meta: meta(),
      currentVersion: "0.1.0",
      now: 1,
      setUpdateStatus: (status) => statuses.push(status),
      fetchImpl: registryFetch("0.2.0"),
    })
    expect(statuses).toEqual([{ kind: "available", version: "0.2.0" }])
    expect(staged).toBe(false)
  })

  test("does nothing for current and file installs", async () => {
    for (const candidate of [meta(), meta({ source: "file", spec: "file:///tmp/kagan" })]) {
      const statuses: UpdateStatus[] = []
      await runUpdateDiscovery({
        api: { kv: mockKv(), lifecycle: { signal: new AbortController().signal } } as unknown as TuiPluginApi,
        meta: candidate,
        currentVersion: "0.1.0",
        now: 1,
        setUpdateStatus: (status) => statuses.push(status),
        fetchImpl: registryFetch("0.1.0"),
      })
      expect(statuses).toEqual([])
    }
  })
})
