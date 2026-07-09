import { describe, expect, test } from "bun:test"
import { checkForUpdate, isNewerRelease, parseRelease, resolveLatestVersion } from "../src/update-check"

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

describe("parseRelease", () => {
  test("accepts clean numeric releases", () => {
    expect(parseRelease("0.1.3")).toEqual([0, 1, 3])
    expect(parseRelease(" 12.0.45 ")).toEqual([12, 0, 45])
  })

  test("rejects dev, prerelease, and garbage", () => {
    for (const raw of ["0.0.0-development", "0.3.0-beta.1", "1.2", "v1.2.3", "latest", ""]) {
      expect(parseRelease(raw)).toBeUndefined()
    }
  })
})

describe("isNewerRelease", () => {
  test("compares segments numerically, not lexically", () => {
    expect(isNewerRelease("0.1.10", "0.1.3")).toBe(true)
    expect(isNewerRelease("0.2.0", "0.1.99")).toBe(true)
    expect(isNewerRelease("1.0.0", "0.9.9")).toBe(true)
  })

  test("is false for equal or older", () => {
    expect(isNewerRelease("0.1.3", "0.1.3")).toBe(false)
    expect(isNewerRelease("0.1.2", "0.1.3")).toBe(false)
  })

  test("is false when either side is not a clean release", () => {
    expect(isNewerRelease("0.2.0", "0.0.0-development")).toBe(false)
    expect(isNewerRelease("0.3.0-beta.1", "0.1.3")).toBe(false)
  })
})

describe("resolveLatestVersion", () => {
  test("fetches and caches when no prior check exists", async () => {
    const kv = mockKv()
    const latest = await resolveLatestVersion(kv, DAY + 1, { fetchImpl: fetchReturning("0.2.0") })
    expect(latest).toBe("0.2.0")
    expect(kv.store["kagan:update:latest"]).toBe("0.2.0")
    expect(kv.store["kagan:update:lastCheck"]).toBe(DAY + 1)
  })

  test("serves cached value without fetching inside the TTL window", async () => {
    const kv = mockKv({ "kagan:update:latest": "0.2.0", "kagan:update:lastCheck": DAY })
    let called = false
    const spyFetch = (async () => {
      called = true
      return { ok: true, json: async () => ({ latest: "0.9.9" }) }
    }) as unknown as typeof fetch
    const latest = await resolveLatestVersion(kv, DAY + HOUR, { fetchImpl: spyFetch })
    expect(latest).toBe("0.2.0")
    expect(called).toBe(false)
  })

  test("refetches once the TTL has elapsed", async () => {
    const kv = mockKv({ "kagan:update:latest": "0.2.0", "kagan:update:lastCheck": 0 })
    const latest = await resolveLatestVersion(kv, DAY + 1, { fetchImpl: fetchReturning("0.3.0") })
    expect(latest).toBe("0.3.0")
    expect(kv.store["kagan:update:lastCheck"]).toBe(DAY + 1)
  })

  test("refetches when the stored timestamp is in the future (clock moved backward)", async () => {
    const kv = mockKv({ "kagan:update:latest": "0.2.0", "kagan:update:lastCheck": DAY * 10 })
    const latest = await resolveLatestVersion(kv, DAY, { fetchImpl: fetchReturning("0.3.0") })
    expect(latest).toBe("0.3.0")
    expect(kv.store["kagan:update:lastCheck"]).toBe(DAY)
  })

  test("keeps the cached value and leaves the timestamp untouched on fetch failure", async () => {
    const kv = mockKv({ "kagan:update:latest": "0.2.0", "kagan:update:lastCheck": 0 })
    const latest = await resolveLatestVersion(kv, DAY + 1, { fetchImpl: fetchThatThrows })
    expect(latest).toBe("0.2.0")
    expect(kv.store["kagan:update:lastCheck"]).toBe(0)
  })

  test("treats a non-ok response and a malformed body as a miss", async () => {
    const kvNotOk = mockKv()
    expect(await resolveLatestVersion(kvNotOk, DAY + 1, { fetchImpl: fetchReturning("0.2.0", false) })).toBeUndefined()
    expect(kvNotOk.store["kagan:update:lastCheck"]).toBeUndefined()

    const kvBadBody = mockKv()
    expect(await resolveLatestVersion(kvBadBody, DAY + 1, { fetchImpl: fetchReturning(42) })).toBeUndefined()
  })
})

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
