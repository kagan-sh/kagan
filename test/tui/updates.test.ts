import { describe, expect, test } from "bun:test"
import {
  checkForUpdate,
  isNewerRelease,
  isUpdateEligibleInstall,
  parseRelease,
  resolveLatestRelease,
} from "../../src/tui/updates/check"

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

function registryFetch(input: { latest?: unknown; fail?: boolean; calls?: string[] }): typeof fetch {
  return (async (url: string | URL | Request) => {
    input.calls?.push(String(url))
    if (input.fail) throw new Error("network down")
    return { ok: true, json: async () => ({ latest: input.latest ?? "0.2.0" }) }
  }) as unknown as typeof fetch
}

const HOUR = 60 * 60 * 1000

describe("release parsing", () => {
  test("accepts only clean numeric releases", () => {
    expect(parseRelease("0.1.3")).toBe("0.1.3")
    for (const raw of ["0.0.0-development", "0.3.0-beta.1", "1.2", "v1.2.3", "latest", ""]) {
      expect(parseRelease(raw)).toBeUndefined()
    }
  })

  test("compares segments numerically", () => {
    expect(isNewerRelease("0.1.10", "0.1.3")).toBe(true)
    expect(isNewerRelease("0.2.0", "0.1.99")).toBe(true)
    expect(isNewerRelease("0.1.3", "0.1.3")).toBe(false)
    expect(isNewerRelease("latest", "0.1.3")).toBe(false)
  })
})

describe("update eligibility", () => {
  test("accepts stable npm bare, latest, and exact specs", () => {
    for (const spec of ["@kagan-sh/kagan", "@kagan-sh/kagan@latest", "@kagan-sh/kagan@0.1.0"]) {
      expect(isUpdateEligibleInstall({ source: "npm", spec, version: "0.1.0" })).toBe(true)
    }
  })

  test("rejects file, development, prerelease, and ranged installs", () => {
    for (const input of [
      { source: "file", spec: "file:///tmp/kagan", version: "0.1.0" },
      { source: "npm", spec: "@kagan-sh/kagan", version: "0.0.0-development" },
      { source: "npm", spec: "@kagan-sh/kagan", version: "0.2.0-beta.1" },
      { source: "npm", spec: "@kagan-sh/kagan@^0.1.0", version: "0.1.0" },
    ]) {
      expect(isUpdateEligibleInstall(input)).toBe(false)
    }
  })
})

describe("resolveLatestRelease", () => {
  test("fetches latest and caches it for one hour", async () => {
    const kv = mockKv()
    const calls: string[] = []
    const fetchImpl = registryFetch({ calls })
    expect(await resolveLatestRelease(kv, HOUR + 1, { fetchImpl })).toBe("0.2.0")
    expect(await resolveLatestRelease(kv, HOUR * 2, { fetchImpl })).toBe("0.2.0")
    expect(calls).toHaveLength(1)
  })

  test("forced checks bypass a fresh cache", async () => {
    const kv = mockKv({ "kagan:update:latest": "0.1.0", "kagan:update:lastCheck": HOUR })
    const calls: string[] = []
    expect(
      await resolveLatestRelease(kv, HOUR + 1, {
        fetchImpl: registryFetch({ latest: "0.2.0", calls }),
        force: true,
      }),
    ).toBe("0.2.0")
    expect(calls).toHaveLength(1)
  })

  test("refetches future timestamps and keeps failures silent", async () => {
    const kv = mockKv({ "kagan:update:latest": "0.1.0", "kagan:update:lastCheck": HOUR * 10 })
    expect(await resolveLatestRelease(kv, HOUR, { fetchImpl: registryFetch({ latest: "0.2.0" }) })).toBe("0.2.0")
    expect(await resolveLatestRelease(mockKv(), HOUR, { fetchImpl: registryFetch({ fail: true }) })).toBeUndefined()
  })
})

describe("checkForUpdate", () => {
  const base = {
    currentVersion: "0.1.0",
    source: "npm" as const,
    spec: "@kagan-sh/kagan",
    now: HOUR + 1,
  }

  test("classifies newer and current releases", async () => {
    expect(await checkForUpdate({ ...base, kv: mockKv(), fetchImpl: registryFetch({ latest: "0.2.0" }) })).toEqual({
      kind: "available",
      version: "0.2.0",
    })
    expect(await checkForUpdate({ ...base, kv: mockKv(), fetchImpl: registryFetch({ latest: "0.1.0" }) })).toEqual({
      kind: "current",
    })
  })

  test("does not query ineligible installations", async () => {
    const calls: string[] = []
    expect(
      await checkForUpdate({
        ...base,
        source: "file",
        spec: "file:///tmp/kagan",
        kv: mockKv(),
        fetchImpl: registryFetch({ calls }),
      }),
    ).toEqual({ kind: "ineligible" })
    expect(calls).toHaveLength(0)
  })
})
