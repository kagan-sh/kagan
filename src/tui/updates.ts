import { clean, gt, satisfies, valid, validRange } from "semver"

const REGISTRY_DIST_TAGS = "https://registry.npmjs.org/-/package/@kagan-sh/kagan/dist-tags"
const REGISTRY_MANIFEST = "https://registry.npmjs.org/@kagan-sh%2Fkagan"
const CHECK_TTL_MS = 60 * 60 * 1000
const FETCH_TIMEOUT_MS = 3000
const LAST_CHECK_KEY = "kagan:update:lastCheck"
const MANIFEST_KEY = "kagan:update:manifest"
export const KAGAN_PACKAGE = "@kagan-sh/kagan"

export type UpdateStatus =
  | { kind: "ready"; version: string }
  | { kind: "blocked"; version: string; requiredOpenCode: string }
  | { kind: "broken" }
  | undefined

type UpdateKv = {
  get: <Value = unknown>(key: string, fallback?: Value) => Value
  set: (key: string, value: unknown) => void
}

// Automatic updates intentionally exclude development and prerelease builds from both sides.
export function parseRelease(version: string): string | undefined {
  const normalized = clean(version.trim())
  return normalized === version.trim() && /^\d+\.\d+\.\d+$/.test(normalized) ? normalized : undefined
}

function isAutomaticUpdateSpec(spec: string): boolean {
  return spec === KAGAN_PACKAGE || spec === `${KAGAN_PACKAGE}@latest`
}

export function isAutomaticUpdateInstall(input: { source: string; spec: string; version: string }): boolean {
  return input.source === "npm" && isAutomaticUpdateSpec(input.spec) && parseRelease(input.version) !== undefined
}

export function isNewerRelease(latest: string, current: string): boolean {
  const next = parseRelease(latest)
  const now = parseRelease(current)
  return Boolean(next && now && gt(next, now))
}

async function fetchJson(fetchImpl: typeof fetch, url: string, timeoutMs: number): Promise<unknown> {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeoutMs)
  try {
    const response = await fetchImpl(url, { signal: controller.signal })
    if (!response.ok) return undefined
    return await response.json()
  } catch {
    return undefined
  } finally {
    clearTimeout(timer)
  }
}

type LatestManifest = { version: string; requiredOpenCode?: string }

function readCachedManifest(value: unknown): LatestManifest | undefined {
  if (!value || typeof value !== "object") return
  const candidate = value as Record<string, unknown>
  if (typeof candidate.version !== "string" || !parseRelease(candidate.version)) return
  if (candidate.requiredOpenCode !== undefined && typeof candidate.requiredOpenCode !== "string") return
  if (typeof candidate.requiredOpenCode === "string" && !validRange(candidate.requiredOpenCode)) return
  return { version: candidate.version, requiredOpenCode: candidate.requiredOpenCode }
}

export async function resolveLatestManifest(
  kv: UpdateKv,
  currentVersion: string,
  now: number,
  deps: { fetchImpl?: typeof fetch } = {},
): Promise<LatestManifest | undefined> {
  const cached = readCachedManifest(kv.get(MANIFEST_KEY))
  const lastCheck = kv.get<number>(LAST_CHECK_KEY, 0)
  if (lastCheck <= now && now - lastCheck < CHECK_TTL_MS) return cached

  const fetchImpl = deps.fetchImpl ?? fetch
  const tags = (await fetchJson(fetchImpl, REGISTRY_DIST_TAGS, FETCH_TIMEOUT_MS)) as { latest?: unknown } | undefined
  const latest = typeof tags?.latest === "string" ? parseRelease(tags.latest) : undefined
  if (!latest) return

  let manifest: LatestManifest = { version: latest }
  if (latest !== currentVersion) {
    const data = (await fetchJson(fetchImpl, `${REGISTRY_MANIFEST}/${latest}`, FETCH_TIMEOUT_MS)) as
      | { engines?: { opencode?: unknown } }
      | undefined
    const requiredOpenCode = data?.engines?.opencode
    if (typeof requiredOpenCode !== "string" || !validRange(requiredOpenCode)) return
    manifest = { version: latest, requiredOpenCode: requiredOpenCode.trim() }
  }

  kv.set(MANIFEST_KEY, manifest)
  kv.set(LAST_CHECK_KEY, now)
  return manifest
}

export type CheckInput = {
  kv: UpdateKv
  currentVersion: string
  openCodeVersion: string
  source: "file" | "npm" | "internal"
  spec: string
  now: number
  fetchImpl?: typeof fetch
}

export async function checkForUpdate(input: CheckInput): Promise<UpdateStatus> {
  if (
    !isAutomaticUpdateInstall({ source: input.source, spec: input.spec, version: input.currentVersion }) ||
    !valid(input.openCodeVersion)
  ) {
    return
  }

  const manifest = await resolveLatestManifest(input.kv, input.currentVersion, input.now, {
    fetchImpl: input.fetchImpl,
  })
  if (!manifest || !isNewerRelease(manifest.version, input.currentVersion) || !manifest.requiredOpenCode) return
  if (satisfies(input.openCodeVersion, manifest.requiredOpenCode)) {
    return { kind: "ready", version: manifest.version }
  }
  return { kind: "blocked", version: manifest.version, requiredOpenCode: manifest.requiredOpenCode }
}
