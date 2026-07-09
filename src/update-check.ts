const REGISTRY_DIST_TAGS = "https://registry.npmjs.org/-/package/@kagan-sh/kagan/dist-tags"
const CHECK_TTL_MS = 24 * 60 * 60 * 1000
const FETCH_TIMEOUT_MS = 3000
const LAST_CHECK_KEY = "kagan:update:lastCheck"
const LATEST_KEY = "kagan:update:latest"

type UpdateKv = {
  get: <Value = unknown>(key: string, fallback?: Value) => Value
  set: (key: string, value: unknown) => void
}

// Only clean numeric releases (x.y.z) are comparable. Dev/prerelease builds — the repo's own
// "0.0.0-development" before semantic-release rewrites it, or any "-beta" tag — return undefined so
// local development installs never surface an update banner and a prerelease `latest` can't false-positive.
export function parseRelease(version: string): [number, number, number] | undefined {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version.trim())
  if (!match) return undefined
  return [Number(match[1]), Number(match[2]), Number(match[3])]
}

export function isNewerRelease(latest: string, current: string): boolean {
  const next = parseRelease(latest)
  const now = parseRelease(current)
  if (!next || !now) return false
  const [nextMajor, nextMinor, nextPatch] = next
  const [nowMajor, nowMinor, nowPatch] = now
  if (nextMajor !== nowMajor) return nextMajor > nowMajor
  if (nextMinor !== nowMinor) return nextMinor > nowMinor
  return nextPatch > nowPatch
}

async function fetchLatestVersion(fetchImpl: typeof fetch, timeoutMs: number): Promise<string | undefined> {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeoutMs)
  try {
    const response = await fetchImpl(REGISTRY_DIST_TAGS, { signal: controller.signal })
    if (!response.ok) return undefined
    const data = (await response.json()) as { latest?: unknown }
    return typeof data.latest === "string" ? data.latest : undefined
  } catch {
    return undefined
  } finally {
    clearTimeout(timer)
  }
}

// Returns the latest published version, hitting npm at most once per TTL window and caching the
// result in kv across restarts. On a failed fetch it keeps serving the cached value and leaves the
// timestamp untouched so the next board load retries rather than going dark for a full day.
export async function resolveLatestVersion(
  kv: UpdateKv,
  now: number,
  deps: { fetchImpl?: typeof fetch; ttlMs?: number } = {},
): Promise<string | undefined> {
  const ttl = deps.ttlMs ?? CHECK_TTL_MS
  const cached = kv.get<string | undefined>(LATEST_KEY, undefined)
  const lastCheck = kv.get<number>(LAST_CHECK_KEY, 0)
  // A future lastCheck means the clock moved backward since the last check; refetch rather than
  // trust it, otherwise the stale cache would be served until wall-clock time catches back up.
  if (lastCheck <= now && now - lastCheck < ttl) return cached

  const latest = await fetchLatestVersion(deps.fetchImpl ?? fetch, FETCH_TIMEOUT_MS)
  if (latest === undefined) return cached
  kv.set(LATEST_KEY, latest)
  kv.set(LAST_CHECK_KEY, now)
  return latest
}

export async function checkForUpdate(input: {
  kv: UpdateKv
  currentVersion: string
  now: number
  onUpdate: (latest: string) => void
  fetchImpl?: typeof fetch
  ttlMs?: number
}): Promise<void> {
  if (!parseRelease(input.currentVersion)) return
  const latest = await resolveLatestVersion(input.kv, input.now, {
    fetchImpl: input.fetchImpl,
    ttlMs: input.ttlMs,
  })
  if (latest && isNewerRelease(latest, input.currentVersion)) input.onUpdate(latest)
}
