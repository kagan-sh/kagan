import { describe, expect, test } from "bun:test"
import { checkForUpdate } from "../../src/tui/updates"

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

function fetchReturning(latest: unknown, ok = true): typeof fetch {
  return (async () => ({
    ok,
    json: async () => ({ latest }),
  })) as unknown as typeof fetch
}

const fetchThatThrows: typeof fetch = (async () => {
  throw new Error("network down")
}) as unknown as typeof fetch

const HOUR = 60 * 60 * 1000
const DAY = 24 * HOUR

describe("checkForUpdate", () => {
  test("notifies once when a newer release is published", async () => {
    const kv = mockKv()
    const seen: string[] = []
    await checkForUpdate({
      kv,
      currentVersion: "0.1.3",
      now: DAY + 1,
      onUpdate: (latest) => seen.push(latest),
      fetchImpl: fetchReturning("0.1.10"),
    })
    expect(seen).toEqual(["0.1.10"])
  })

  test("stays silent when already on the latest release", async () => {
    const kv = mockKv()
    const seen: string[] = []
    await checkForUpdate({
      kv,
      currentVersion: "0.1.3",
      now: DAY + 1,
      onUpdate: (latest) => seen.push(latest),
      fetchImpl: fetchReturning("0.1.3"),
    })
    expect(seen).toEqual([])
  })

  test("notifies for a major-version bump", async () => {
    const kv = mockKv()
    const seen: string[] = []
    await checkForUpdate({
      kv,
      currentVersion: "1.9.9",
      now: DAY + 1,
      onUpdate: (latest) => seen.push(latest),
      fetchImpl: fetchReturning("2.0.0"),
    })
    expect(seen).toEqual(["2.0.0"])
  })

  test("notifies for a minor-version bump", async () => {
    const kv = mockKv()
    const seen: string[] = []
    await checkForUpdate({
      kv,
      currentVersion: "1.1.9",
      now: DAY + 1,
      onUpdate: (latest) => seen.push(latest),
      fetchImpl: fetchReturning("1.2.0"),
    })
    expect(seen).toEqual(["1.2.0"])
  })

  test("serves the cached latest without fetching inside the TTL window", async () => {
    const kv = mockKv({ "kagan:update:latest": "2.0.0", "kagan:update:lastCheck": DAY })
    let called = false
    const spyFetch = (async () => {
      called = true
      return { ok: true, json: async () => ({ latest: "9.9.9" }) }
    }) as unknown as typeof fetch
    const seen: string[] = []
    await checkForUpdate({
      kv,
      currentVersion: "1.0.0",
      now: DAY + HOUR,
      onUpdate: (latest) => seen.push(latest),
      fetchImpl: spyFetch,
    })
    expect(called).toBe(false)
    expect(seen).toEqual(["2.0.0"])
  })

  test("refetches when the stored lastCheck is in the future (clock moved backward)", async () => {
    const kv = mockKv({ "kagan:update:latest": "1.0.0", "kagan:update:lastCheck": DAY * 3 })
    let called = false
    const spyFetch = (async () => {
      called = true
      return { ok: true, json: async () => ({ latest: "2.0.0" }) }
    }) as unknown as typeof fetch
    const seen: string[] = []
    await checkForUpdate({
      kv,
      currentVersion: "1.0.0",
      now: DAY,
      onUpdate: (latest) => seen.push(latest),
      fetchImpl: spyFetch,
    })
    expect(called).toBe(true)
    expect(seen).toEqual(["2.0.0"])
    expect(kv.store["kagan:update:lastCheck"]).toBe(DAY)
    expect(kv.store["kagan:update:latest"]).toBe("2.0.0")
  })

  test("keeps the cached latest and preserves lastCheck when the fetch fails, so the next call retries", async () => {
    const kv = mockKv({ "kagan:update:latest": "2.0.0", "kagan:update:lastCheck": 0 })
    let failingCalls = 0
    const failing = (async () => {
      failingCalls++
      throw new Error("network down")
    }) as unknown as typeof fetch
    const seen: string[] = []
    await checkForUpdate({
      kv,
      currentVersion: "1.0.0",
      now: DAY * 2,
      onUpdate: (latest) => seen.push(latest),
      fetchImpl: failing,
    })
    expect(failingCalls).toBe(1)
    expect(seen).toEqual(["2.0.0"])
    expect(kv.store["kagan:update:lastCheck"]).toBe(0)

    await checkForUpdate({
      kv,
      currentVersion: "1.0.0",
      now: DAY * 2,
      onUpdate: (latest) => seen.push(latest),
      fetchImpl: fetchReturning("3.0.0"),
    })
    expect(failingCalls).toBe(1)
    expect(seen).toEqual(["2.0.0", "3.0.0"])
    expect(kv.store["kagan:update:latest"]).toBe("3.0.0")
    expect(kv.store["kagan:update:lastCheck"]).toBe(DAY * 2)
  })

  test("never fetches for a dev/prerelease install", async () => {
    const kv = mockKv()
    let called = false
    const spyFetch = (async () => {
      called = true
      return { ok: true, json: async () => ({ latest: "9.9.9" }) }
    }) as unknown as typeof fetch
    const seen: string[] = []
    await checkForUpdate({
      kv,
      currentVersion: "0.0.0-development",
      now: DAY + 1,
      onUpdate: (latest) => seen.push(latest),
      fetchImpl: spyFetch,
    })
    expect(called).toBe(false)
    expect(seen).toEqual([])
  })
})
