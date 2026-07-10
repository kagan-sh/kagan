import { describe, expect, test } from "bun:test"
import { checkForUpdate, isNewerRelease, parseRelease, resolveLatestManifest } from "../../src/tui/updates"

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

function registryFetch(input: {
  latest?: unknown
  engine?: unknown
  failTags?: boolean
  failManifest?: boolean
  calls?: string[]
}): typeof fetch {
  return (async (url: string | URL | Request) => {
    const value = String(url)
    input.calls?.push(value)
    const manifest = !value.endsWith("/dist-tags")
    if ((!manifest && input.failTags) || (manifest && input.failManifest)) throw new Error("network down")
    return {
      ok: true,
      json: async () => (manifest ? { engines: { opencode: input.engine } } : { latest: input.latest ?? "0.2.0" }),
    }
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
    expect(isNewerRelease("1.0.0", "0.9.9")).toBe(true)
    expect(isNewerRelease("0.1.3", "0.1.3")).toBe(false)
    expect(isNewerRelease("0.1.2", "0.1.3")).toBe(false)
    expect(isNewerRelease("latest", "0.1.3")).toBe(false)
  })
})

describe("resolveLatestManifest", () => {
  test("fetches latest and its engine range, then caches for one hour", async () => {
    const kv = mockKv()
    const calls: string[] = []
    const fetchImpl = registryFetch({ latest: "0.2.0", engine: ">=1.17.13 <1.18.0", calls })
    expect(await resolveLatestManifest(kv, "0.1.0", HOUR + 1, { fetchImpl })).toEqual({
      version: "0.2.0",
      requiredOpenCode: ">=1.17.13 <1.18.0",
    })
    expect(calls).toHaveLength(2)

    expect(await resolveLatestManifest(kv, "0.1.0", HOUR * 2, { fetchImpl })).toEqual({
      version: "0.2.0",
      requiredOpenCode: ">=1.17.13 <1.18.0",
    })
    expect(calls).toHaveLength(2)
  })

  test("does not fetch a manifest when latest equals current", async () => {
    const calls: string[] = []
    const result = await resolveLatestManifest(mockKv(), "0.2.0", HOUR + 1, {
      fetchImpl: registryFetch({ latest: "0.2.0", calls }),
    })
    expect(result).toEqual({ version: "0.2.0" })
    expect(calls).toHaveLength(1)
  })

  test("refetches when the cached timestamp is in the future", async () => {
    const kv = mockKv({
      "kagan:update:manifest": { version: "0.1.0" },
      "kagan:update:lastCheck": HOUR * 10,
    })
    const calls: string[] = []
    expect(
      await resolveLatestManifest(kv, "0.1.0", HOUR, {
        fetchImpl: registryFetch({ latest: "0.2.0", engine: ">=1.17.13", calls }),
      }),
    ).toEqual({ version: "0.2.0", requiredOpenCode: ">=1.17.13" })
    expect(calls).toHaveLength(2)
    expect(kv.store["kagan:update:lastCheck"]).toBe(HOUR)
  })

  test("stays quiet and does not refresh the timestamp after registry or manifest failure", async () => {
    for (const fetchImpl of [
      registryFetch({ failTags: true }),
      registryFetch({ latest: "0.2.0", failManifest: true }),
    ]) {
      const kv = mockKv({
        "kagan:update:manifest": { version: "0.1.9", requiredOpenCode: ">=1.0.0" },
        "kagan:update:lastCheck": 0,
      })
      expect(await resolveLatestManifest(kv, "0.1.0", HOUR + 1, { fetchImpl })).toBeUndefined()
      expect(kv.store["kagan:update:lastCheck"]).toBe(0)
    }
  })
})

describe("checkForUpdate", () => {
  const base = {
    currentVersion: "0.1.0",
    openCodeVersion: "1.17.18",
    source: "npm" as const,
    spec: "@kagan-sh/kagan",
    now: HOUR + 1,
  }

  test("classifies compatible latest as ready for bare and explicit latest specs", async () => {
    for (const spec of ["@kagan-sh/kagan", "@kagan-sh/kagan@latest"]) {
      expect(
        await checkForUpdate({
          ...base,
          spec,
          kv: mockKv(),
          fetchImpl: registryFetch({ latest: "0.2.0", engine: ">=1.17.13 <1.18.0" }),
        }),
      ).toEqual({ kind: "ready", version: "0.2.0" })
    }
  })

  test("classifies incompatible latest with its required OpenCode range", async () => {
    expect(
      await checkForUpdate({
        ...base,
        kv: mockKv(),
        fetchImpl: registryFetch({ latest: "0.2.0", engine: ">=1.18.0" }),
      }),
    ).toEqual({ kind: "blocked", version: "0.2.0", requiredOpenCode: ">=1.18.0" })
  })

  test("rejects a missing or invalid engine range", async () => {
    for (const engine of [undefined, "not a range"]) {
      expect(
        await checkForUpdate({
          ...base,
          kv: mockKv(),
          fetchImpl: registryFetch({ latest: "0.2.0", engine }),
        }),
      ).toBeUndefined()
    }
  })

  test("does not query npm for exact pins, file installs, or development builds", async () => {
    for (const input of [
      { ...base, spec: "@kagan-sh/kagan@0.1.0" },
      { ...base, source: "file" as const, spec: "file:///tmp/kagan" },
      { ...base, currentVersion: "0.0.0-development" },
      { ...base, openCodeVersion: "development" },
    ]) {
      const calls: string[] = []
      expect(await checkForUpdate({ ...input, kv: mockKv(), fetchImpl: registryFetch({ calls }) })).toBeUndefined()
      expect(calls).toHaveLength(0)
    }
  })
})
